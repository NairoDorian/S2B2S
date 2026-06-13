#!/usr/bin/env python3
"""
HTTP server for Parakeet Unified ONNX STT — keeps both encoder + decoder ONNX models loaded in RAM.

Uses onnxruntime (>=1.26.0) for the latest optimizations including the Nemotron Conformer MHA fusion.

Model directory structure (fp32 variant):
    models/STT/parakeet-unified-en-0.6b-fp32/
        encoder.onnx           (~2.0 GB)
        encoder.onnx.data      (external weights)
        decoder_joint.onnx     (~400 MB)
        decoder_joint.onnx.data
        tokenizer.model        (SentencePiece)

Model directory structure (int8 variant):
    models/STT/parakeet-unified-en-0.6b-int8/
        encoder.int8.onnx      (~500 MB, self-contained)
        decoder_joint.int8.onnx (~130 MB, self-contained)
        tokenizer.model

Usage:
    python unified_parakeet_server.py --port 51235 --model-dir ./models/STT/parakeet-unified-en-0.6b-int8

Endpoints:
    POST /transcribe    body = raw float32le audio bytes (16kHz, mono)  →  {"text": "..."}
    GET  /health         →  {"status": "ok"}
"""
from __future__ import annotations

import argparse
import json
import os
import struct
import sys
import traceback
from http.server import HTTPServer, BaseHTTPRequestHandler

import numpy as np

# ---------------------------------------------------------------------------
# Constants (mirrors parakeet-rs)
# ---------------------------------------------------------------------------
SAMPLE_RATE = 16000
FEATURE_SIZE = 128       # mel bins
N_FFT = 512
HOP_LENGTH = 160
WIN_LENGTH = 400
PREEMPHASIS_COEF = 0.97
SUBSAMPLING_FACTOR = 8
DECODER_LSTM_DIM = 640
DECODER_LSTM_LAYERS = 2
MAX_SYMBOLS_PER_STEP = 10
VOCAB_SIZE = 1025
BLANK_ID = 1024

F_SP = 200.0 / 3.0
MIN_LOG_HZ = 1000.0
MIN_LOG_MEL = MIN_LOG_HZ / F_SP
LOG_STEP = 0.06875177742094912   # 1 / ln(HZ2MEL_SCALE) where HZ2MEL_SCALE = 700


# ---------------------------------------------------------------------------
# Mel spectrogram (match parakeet-rs exactly)
# ---------------------------------------------------------------------------
def _hz_to_mel_slaney(hz: float) -> float:
    if hz < MIN_LOG_HZ:
        return hz / F_SP
    return MIN_LOG_MEL + np.log(hz / MIN_LOG_HZ) / LOG_STEP


def _mel_to_hz_slaney(mel_val: float) -> float:
    if mel_val < MIN_LOG_MEL:
        return mel_val * F_SP
    return MIN_LOG_HZ * np.exp((mel_val - MIN_LOG_MEL) * LOG_STEP)


def _create_mel_filterbank(n_fft: int, n_mels: int, sample_rate: int) -> np.ndarray:
    freq_bins = n_fft // 2 + 1
    filterbank = np.zeros((n_mels, freq_bins), dtype=np.float32)

    fmax = sample_rate / 2.0
    mel_min = _hz_to_mel_slaney(0.0)
    mel_max = _hz_to_mel_slaney(fmax)

    mel_points = np.array([
        _mel_to_hz_slaney(mel_min + (mel_max - mel_min) * i / (n_mels + 1))
        for i in range(n_mels + 2)
    ])
    fft_freqs = np.arange(freq_bins, dtype=np.float64) * sample_rate / n_fft
    fdiff = np.diff(mel_points)

    for i in range(n_mels):
        lower = (fft_freqs - mel_points[i]) / fdiff[i]
        upper = (mel_points[i + 2] - fft_freqs) / fdiff[i + 1]
        filterbank[i] = np.maximum(0.0, np.minimum(lower, upper))

    for i in range(n_mels):
        enorm = 2.0 / (mel_points[i + 2] - mel_points[i])
        filterbank[i] *= enorm

    return filterbank


def extract_features(
    audio: np.ndarray,
    mel_basis: np.ndarray,
    sample_rate: int = SAMPLE_RATE,
) -> np.ndarray:
    """Compute 128-dimensional log-mel spectrogram with per-feature normalization."""
    # Stereo → mono
    if audio.ndim > 1:
        audio = audio.mean(axis=-1)

    # Pre-emphasis
    audio = np.append(audio[0:1], audio[1:] - PREEMPHASIS_COEF * audio[:-1])

    # STFT (mirror rustfft exact frames, no scipy/librosa dependency)
    pad = N_FFT // 2
    n_frames = 1 + (len(audio) + pad * 2 - N_FFT) // HOP_LENGTH
    # Pad with reflect (match parakeet-rs "right" padding)
    audio_padded = np.pad(audio, (pad, pad + HOP_LENGTH), mode='reflect')
    frames = np.zeros((n_frames, N_FFT), dtype=np.float32)
    window = np.hanning(N_FFT).astype(np.float32)
    for i in range(n_frames):
        start = i * HOP_LENGTH
        frames[i] = audio_padded[start:start + N_FFT] * window

    spec = np.abs(np.fft.rfft(frames, n=N_FFT)).astype(np.float32)  # (n_frames, freq_bins)

    # Mel filterbank
    mel_spec = spec @ mel_basis.T  # (n_frames, n_mels)

    # Log with additive guard (NeMo convention: 2^-24)
    log_zero_guard = 2.0 ** -24
    mel_spec = np.log(mel_spec + log_zero_guard)

    n_frames_out, n_feats = mel_spec.shape
    if n_frames_out > 1:
        mean = mel_spec.mean(axis=0, keepdims=True)
        std = mel_spec.std(axis=0, ddof=1, keepdims=True) + 1e-5
        mel_spec = (mel_spec - mean) / std

    return mel_spec.astype(np.float32)  # (n_frames, FEATURE_SIZE)


# ---------------------------------------------------------------------------
# Model
# ---------------------------------------------------------------------------
MODEL: dict | None = None   # global: dict with encoder_session, decoder_session, mel_basis, tokenizer


def load_model(model_dir: str):
    import onnxruntime as ort

    # Find encoder
    for enc_name in ("encoder.onnx", "encoder.int8.onnx"):
        enc_path = os.path.join(model_dir, enc_name)
        if os.path.isfile(enc_path):
            break
    else:
        raise FileNotFoundError(f"No encoder found in {model_dir}")

    # Find decoder
    for dec_name in ("decoder_joint.onnx", "decoder_joint.int8.onnx"):
        dec_path = os.path.join(model_dir, dec_name)
        if os.path.isfile(dec_path):
            break
    else:
        raise FileNotFoundError(f"No decoder_joint found in {model_dir}")

    print(f"[unified_server] Loading encoder: {enc_path}", file=sys.stderr, flush=True)
    so = ort.SessionOptions()
    so.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL
    encoder = ort.InferenceSession(enc_path, so, providers=["CPUExecutionProvider"])

    print(f"[unified_server] Loading decoder: {dec_path}", file=sys.stderr, flush=True)
    decoder = ort.InferenceSession(dec_path, so, providers=["CPUExecutionProvider"])

    mel_basis = _create_mel_filterbank(N_FFT, FEATURE_SIZE, SAMPLE_RATE)

    return {"encoder": encoder, "decoder": decoder, "mel_basis": mel_basis}


# ---------------------------------------------------------------------------
# SentencePiece tokenizer (minimal protobuf parser)
# ---------------------------------------------------------------------------
def load_tokenizer(model_dir: str):
    import sentencepiece as spm
    tok_path = os.path.join(model_dir, "tokenizer.model")
    if not os.path.isfile(tok_path):
        raise FileNotFoundError(f"tokenizer.model not found in {model_dir}")
    sp = spm.SentencePieceProcessor()
    sp.Load(tok_path)
    return sp


# ---------------------------------------------------------------------------
# RNN-T greedy decoder
# ---------------------------------------------------------------------------
def transcribe(audio: bytes) -> str:
    """Run full inference pipeline: audio bytes → mel → encoder → decoder → text."""
    if MODEL is None:
        raise RuntimeError("Model not loaded")

    # Decode raw float32le audio
    samples = np.frombuffer(audio, dtype=np.float32).copy()
    if len(samples) == 0:
        return ""

    # Mel spectrogram
    features = extract_features(samples, MODEL["mel_basis"])  # (T_mel, 128)

    # Encoder: expects [1, 128, T] and [1] length
    encoder = MODEL["encoder"]
    enc_input = features.T[np.newaxis, :, :]  # (1, 128, T_mel)
    enc_len = np.array([features.shape[0]], dtype=np.int64)
    enc_out = encoder.run(None, {"audio_signal": enc_input, "length": enc_len})
    encoded = enc_out[0]   # (1, D, T_enc)
    enc_len_out = int(enc_out[1][0])

    frame_count = min(enc_len_out, encoded.shape[2])
    if frame_count == 0:
        return ""

    hidden_dim = encoded.shape[1]

    # RNN-T greedy decoding
    decoder = MODEL["decoder"]
    tokenizer = MODEL["tokenizer"]

    state_1 = np.zeros((DECODER_LSTM_LAYERS, 1, DECODER_LSTM_DIM), dtype=np.float32)
    state_2 = np.zeros((DECODER_LSTM_LAYERS, 1, DECODER_LSTM_DIM), dtype=np.float32)
    last_token = np.array([[BLANK_ID]], dtype=np.int64)
    target_length = np.array([1], dtype=np.int32)
    tokens = []

    for frame_idx in range(frame_count):
        frame = encoded[0:1, :, frame_idx:frame_idx + 1]  # (1, D, 1)

        for _ in range(MAX_SYMBOLS_PER_STEP):
            d_out = decoder.run(
                None,
                {
                    "encoder_outputs": frame,
                    "targets": last_token,
                    "target_length": target_length,
                    "input_states_1": state_1,
                    "input_states_2": state_2,
                },
            )
            logits = d_out[0][0, 0, :]  # (vocab_size,)
            state_1 = d_out[1]
            state_2 = d_out[2]

            token_id = int(np.argmax(logits))

            if token_id == BLANK_ID:
                break

            tokens.append(token_id)
            last_token = np.array([[token_id]], dtype=np.int64)

    if not tokens:
        return ""

    text = tokenizer.decode(tokens)
    return text


# ---------------------------------------------------------------------------
# HTTP handler
# ---------------------------------------------------------------------------
class UnifiedParakeetHandler(BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        print(f"[unified_server] {args[0]}", file=sys.stderr, flush=True)

    def do_GET(self):
        if self.path in ("/health", "/"):
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps({"status": "ok"}).encode())
        else:
            self.send_response(404)
            self.end_headers()

    def do_POST(self):
        try:
            content_length = int(self.headers.get("Content-Length", 0))
            audio = self.rfile.read(content_length)

            text = transcribe(audio)

            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps({"text": text}).encode())

        except Exception:
            self.send_response(500)
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            tb = traceback.format_exc()
            print(f"[unified_server] Error:\n{tb}", file=sys.stderr, flush=True)
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
        for sub in ["models/STT/parakeet-unified-en-0.6b-int8",
                     "models/STT/parakeet-unified-en-0.6b-fp32"]:
            candidate = os.path.join(base, sub)
            if os.path.isdir(candidate):
                return candidate

    raise FileNotFoundError("No unified parakeet model directory found. "
                            "Download the model first or pass --model-dir.")


def main():
    parser = argparse.ArgumentParser(description="Parakeet Unified ONNX STT HTTP server")
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--model-dir", default=None, help="Path to model directory")
    args = parser.parse_args()

    global MODEL

    model_dir = resolve_model_dir(args.model_dir)
    print(f"[unified_server] Model directory: {model_dir}", file=sys.stderr, flush=True)

    # Load model eagerly
    MODEL = load_model(model_dir)
    MODEL["tokenizer"] = load_tokenizer(model_dir)

    print(f"[unified_server] Model loaded. Port: {args.port}", file=sys.stderr, flush=True)

    server = HTTPServer((args.host, args.port), UnifiedParakeetHandler)
    print(f"[unified_server] Listening on http://{args.host}:{args.port}", file=sys.stderr, flush=True)

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("[unified_server] Shutting down", file=sys.stderr, flush=True)
        server.server_close()


if __name__ == "__main__":
    main()
