#!/usr/bin/env python3
"""
HTTP server for sherpa-onnx STT models (Nemotron 3.5 ASR and future models).

Uses sherpa-onnx's OnlineRecognizer which handles the full pipeline:
mel spectrogram, encoder cache, decoder, joiner, beam search, tokenizer.

Usage:
    python sherpa_onnx_server.py --port 51235 --model-dir ./models/STT/nemotron-3.5-asr-0.6b-int8

Endpoints:
    POST /transcribe   body = raw float32le audio bytes (16kHz, mono) → {"text": "..."}
    POST /stream_start       → {"status": "ok"}
    POST /stream_feed        body = raw float32le audio bytes → {"text": "...", "eou": bool}
    POST /stream_end          → {"text": "...", "eou": bool}
    GET  /stream_status      → {"text": "...", "eou": bool}
    GET  /health             → {"status": "ok"}
"""
from __future__ import annotations

import argparse
import json
import os
import sys
import traceback
from http.server import HTTPServer, BaseHTTPRequestHandler

import numpy as np
import sherpa_onnx

# ---------------------------------------------------------------------------
# State
# ---------------------------------------------------------------------------
RECOGNIZER: sherpa_onnx.OnlineRecognizer | None = None
STREAM: sherpa_onnx.OnlineStream | None = None
STREAM_RESULT: str = ""
STREAM_EOU: bool = False
SAMPLE_RATE: int = 16000


# ---------------------------------------------------------------------------
# Model loading
# ---------------------------------------------------------------------------
def load_model(model_dir: str):
    global RECOGNIZER, SAMPLE_RATE

    tokens = os.path.join(model_dir, "tokens.txt")
    encoder = os.path.join(model_dir, "encoder.int8.onnx")
    decoder = os.path.join(model_dir, "decoder.int8.onnx")
    joiner = os.path.join(model_dir, "joiner.int8.onnx")

    for path, name in [(tokens, "tokens.txt"), (encoder, "encoder"),
                        (decoder, "decoder"), (joiner, "joiner")]:
        if not os.path.isfile(path):
            raise FileNotFoundError(f"{name} not found at {path}")

    print(f"[sherpa_server] Loading encoder: {encoder}", file=sys.stderr, flush=True)
    print(f"[sherpa_server] Loading decoder: {decoder}", file=sys.stderr, flush=True)
    print(f"[sherpa_server] Loading joiner:  {joiner}", file=sys.stderr, flush=True)

    RECOGNIZER = sherpa_onnx.OnlineRecognizer.from_transducer(
        tokens=tokens,
        encoder=encoder,
        decoder=decoder,
        joiner=joiner,
        num_threads=2,
        sample_rate=16000,
        feature_dim=80,
        decoding_method="greedy_search",
        provider="cpu",
        enable_endpoint_detection=True,
        rule1_min_trailing_silence=2.4,
        rule2_min_trailing_silence=1.2,
        rule3_min_utterance_length=20.0,
    )
    SAMPLE_RATE = int(RECOGNIZER.config.feat_config.sampling_rate)

    print(f"[sherpa_server] Model loaded. sample_rate={SAMPLE_RATE}",
          file=sys.stderr, flush=True)


# ---------------------------------------------------------------------------
# Offline transcription
# ---------------------------------------------------------------------------
def transcribe(audio: bytes) -> str:
    if RECOGNIZER is None:
        raise RuntimeError("Model not loaded")

    samples = np.frombuffer(audio, dtype=np.float32).copy()
    if len(samples) == 0:
        return ""

    s = RECOGNIZER.create_stream()
    s.accept_waveform(SAMPLE_RATE, samples)
    tail = np.zeros(int(0.5 * SAMPLE_RATE), dtype=np.float32)
    s.accept_waveform(SAMPLE_RATE, tail)
    s.input_finished()

    while RECOGNIZER.is_ready(s):
        RECOGNIZER.decode_streams([s])

    return RECOGNIZER.get_result(s)


# ---------------------------------------------------------------------------
# Streaming
# ---------------------------------------------------------------------------
def stream_reset():
    global STREAM, STREAM_RESULT, STREAM_EOU
    STREAM = RECOGNIZER.create_stream()
    STREAM_RESULT = ""
    STREAM_EOU = False


def stream_feed(audio: bytes) -> dict:
    global STREAM_RESULT, STREAM_EOU
    if RECOGNIZER is None or STREAM is None:
        raise RuntimeError("Stream not started")

    samples = np.frombuffer(audio, dtype=np.float32).copy()
    if len(samples) == 0:
        return {"text": STREAM_RESULT, "eou": STREAM_EOU}

    STREAM.accept_waveform(SAMPLE_RATE, samples)

    while RECOGNIZER.is_ready(STREAM):
        RECOGNIZER.decode_streams([STREAM])
        STREAM_RESULT = RECOGNIZER.get_result(STREAM)

        if RECOGNIZER.is_endpoint(STREAM):
            STREAM_EOU = True

    return {"text": STREAM_RESULT, "eou": STREAM_EOU}


def stream_end() -> dict:
    global STREAM_RESULT, STREAM_EOU
    if RECOGNIZER is None or STREAM is None:
        return {"text": STREAM_RESULT, "eou": STREAM_EOU}

    tail = np.zeros(int(0.5 * SAMPLE_RATE), dtype=np.float32)
    STREAM.accept_waveform(SAMPLE_RATE, tail)
    STREAM.input_finished()

    while RECOGNIZER.is_ready(STREAM):
        RECOGNIZER.decode_streams([STREAM])
        STREAM_RESULT = RECOGNIZER.get_result(STREAM)

    if RECOGNIZER.is_endpoint(STREAM):
        STREAM_EOU = True

    return {"text": STREAM_RESULT, "eou": STREAM_EOU}


# ---------------------------------------------------------------------------
# HTTP handler
# ---------------------------------------------------------------------------
class SherpaOnnxHandler(BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        print(f"[sherpa_server] {args[0]}", file=sys.stderr, flush=True)

    def _json(self, code: int, data: dict):
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(data).encode())

    def do_GET(self):
        if self.path in ("/health", "/"):
            self._json(200, {"status": "ok"})
        elif self.path == "/stream_status":
            self._json(200, {"text": STREAM_RESULT, "eou": STREAM_EOU})
        else:
            self.send_response(404)
            self.end_headers()

    def do_POST(self):
        try:
            content_length = int(self.headers.get("Content-Length", 0))
            body = self.rfile.read(content_length)

            if self.path == "/transcribe":
                text = transcribe(body)
                self._json(200, {"text": text})

            elif self.path == "/stream_start":
                stream_reset()
                self._json(200, {"status": "ok"})

            elif self.path == "/stream_feed":
                result = stream_feed(body)
                self._json(200, result)

            elif self.path == "/stream_end":
                result = stream_end()
                self._json(200, result)

            else:
                self.send_response(404)
                self.end_headers()

        except Exception:
            self.send_response(500)
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            tb = traceback.format_exc()
            print(f"[sherpa_server] Error:\n{tb}", file=sys.stderr, flush=True)
            self.wfile.write(tb.encode())


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
def resolve_model_dir(model_dir_arg: str | None) -> str:
    if model_dir_arg and os.path.isdir(model_dir_arg):
        return model_dir_arg
    script_dir = os.path.dirname(os.path.abspath(__file__))
    cwd = os.getcwd()
    for base in [script_dir, cwd]:
        for sub in ["models/STT/nemotron-3.5-asr-0.6b-int8"]:
            candidate = os.path.join(base, sub)
            if os.path.isdir(candidate):
                return candidate
    raise FileNotFoundError("No sherpa-onnx model directory found. "
                            "Download the model first or pass --model-dir.")


def main():
    parser = argparse.ArgumentParser(description="Sherpa-ONNX STT HTTP server")
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--model-dir", default=None)
    args = parser.parse_args()

    model_dir = resolve_model_dir(args.model_dir)
    print(f"[sherpa_server] Model directory: {model_dir}", file=sys.stderr, flush=True)

    load_model(model_dir)

    print(f"[sherpa_server] Listening on http://{args.host}:{args.port}",
          file=sys.stderr, flush=True)

    server = HTTPServer((args.host, args.port), SherpaOnnxHandler)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("[sherpa_server] Shutting down", file=sys.stderr, flush=True)
        server.server_close()


if __name__ == "__main__":
    main()
