#!/usr/bin/env python3
"""
HTTP server for Parakeet ONNX STT models — offline + streaming RNNT inference.

Uses onnxruntime (>=1.26.0) for the latest optimizations including the Nemotron Conformer MHA fusion.

Supports model families via auto-detection:

  Unified (eschmidbauer/parakeet-unified-en-0.6b-onnx):
    tokenizer.model  (SentencePiece)
    encoder.onnx / encoder.int8.onnx
    decoder_joint.onnx / decoder_joint.int8.onnx
    Mel normalization: per_feature, decoder layers: 2, blank_id: 1024

  EOU (ysdede/parakeet-realtime-eou-120m-v1-onnx):
    vocab.txt         (word-list tokenizer)
    encoder-model.{quant}.onnx / decoder_joint-model.{quant}.onnx
    Mel normalization: none (raw log-mel), decoder layers: 1, blank_id: from config.json
    Emits <EOU> token for end-of-utterance detection (streaming mode)

  Sherpa-onnx (Nemotron 3.5 ASR and future models):
    tokens.txt         (simple token list)
    encoder.int8.onnx / decoder.int8.onnx / joiner.int8.onnx
    Full pipeline via sherpa-onnx OnlineRecognizer — mel, encoder cache,
    RNNT decoder, beam search, tokenizer, endpoint detection.
    Auto-detected when tokens.txt is present.

Quantization: each model directory contains one encoder + one decoder ONNX.
The server auto-detects whichever files are present.

Endpoints:
    POST /transcribe   body = raw float32le audio bytes (16kHz, mono)  →  {"text": "..."}

    POST /stream_start       →  {"status": "ok"}
    POST /stream_feed        body = raw float32le audio bytes           →  {"text": "...", "eou": bool}
    GET  /stream_status      →  {"text": "...", "eou": bool}
    POST /stream_end         →  {"text": "...", "eou": bool}

    GET  /health             →  {"status": "ok"}
"""
from __future__ import annotations

import argparse
import json
import os
import sys
import traceback
from http.server import HTTPServer, BaseHTTPRequestHandler

import numpy as np

# ---------------------------------------------------------------------------
# Signal-processing constants (shared across all Parakeet-family models)
# ---------------------------------------------------------------------------
SAMPLE_RATE = 16000
FEATURE_SIZE = 128
N_FFT = 512
HOP_LENGTH = 160
WIN_LENGTH = 400
PREEMPHASIS_COEF = 0.97
MAX_SYMBOLS_PER_STEP = 10

F_SP = 200.0 / 3.0
MIN_LOG_HZ = 1000.0
MIN_LOG_MEL = MIN_LOG_HZ / F_SP
LOG_STEP = 0.06875177742094912


# ============================================================================
# Mel spectrogram  (Slaney scale, matches parakeet-rs / speech-recognition)
# ============================================================================
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
    normalize: bool = True,
) -> np.ndarray:
    if audio.ndim > 1:
        audio = audio.mean(axis=-1)
    audio = np.append(audio[0:1], audio[1:] - PREEMPHASIS_COEF * audio[:-1])
    pad = N_FFT // 2
    n_frames = 1 + (len(audio) + pad * 2 - N_FFT) // HOP_LENGTH
    audio_padded = np.pad(audio, (pad, pad + HOP_LENGTH), mode='reflect')
    frames = np.zeros((n_frames, N_FFT), dtype=np.float32)
    window = np.hanning(N_FFT).astype(np.float32)
    for i in range(n_frames):
        start = i * HOP_LENGTH
        frames[i] = audio_padded[start:start + N_FFT] * window
    spec = np.abs(np.fft.rfft(frames, n=N_FFT)).astype(np.float32)
    mel_spec = spec @ mel_basis.T
    log_zero_guard = 2.0 ** -24
    mel_spec = np.log(mel_spec + log_zero_guard)
    if normalize:
        n_frames_out = mel_spec.shape[0]
        if n_frames_out > 1:
            mean = mel_spec.mean(axis=0, keepdims=True)
            std = mel_spec.std(axis=0, ddof=1, keepdims=True) + 1e-5
            mel_spec = (mel_spec - mean) / std
    return mel_spec.astype(np.float32)


# ============================================================================
# Tokenizer
# ============================================================================
class _SentencePieceTokenizer:
    def __init__(self, model_dir: str):
        import sentencepiece as spm
        tok_path = os.path.join(model_dir, "tokenizer.model")
        if not os.path.isfile(tok_path):
            raise FileNotFoundError(f"tokenizer.model not found in {model_dir}")
        self._sp = spm.SentencePieceProcessor()
        self._sp.Load(tok_path)

    def decode(self, ids: list[int]) -> str:
        if not ids:
            return ""
        return self._sp.decode(ids)


class _VocabTxtTokenizer:
    """Word-list tokenizer for EOU models. Tracks <EOU>/<EOB> control tokens."""
    def __init__(self, model_dir: str):
        tok_path = os.path.join(model_dir, "vocab.txt")
        if not os.path.isfile(tok_path):
            raise FileNotFoundError(f"vocab.txt not found in {model_dir}")
        with open(tok_path, encoding="utf-8") as f:
            lines = [ln.strip() for ln in f if ln.strip()]
        idx_vocab = all(len(ln.split()) == 2 and ln.split()[1].isdigit() for ln in lines)
        if idx_vocab:
            entries = [(ln.split()[0], int(ln.split()[1])) for ln in lines]
            max_id = max(idx for _, idx in entries)
            id_to_token: list[str] = [""] * (max_id + 1)
            for token, idx in entries:
                id_to_token[idx] = token
        else:
            id_to_token = lines
        self._id_to_token = id_to_token
        self.vocab_size = len(id_to_token)
        self._blank_id = (
            id_to_token.index("<blk>") if "<blk>" in id_to_token else self.vocab_size
        )
        self._sanitized = [tok.replace("\u2581", " ") for tok in id_to_token]
        import re
        self._ctrl_ids = {
            i for i, tok in enumerate(id_to_token)
            if re.fullmatch(r"<[^>\s]+>", tok) and tok != "<blk>"
        }
        self._eou_id = (
            id_to_token.index("<EOU>") if "<EOU>" in id_to_token else None
        )

    @property
    def blank_id(self) -> int:
        return self._blank_id

    @property
    def eou_id(self) -> int | None:
        return self._eou_id

    def decode(self, ids: list[int], skip_control: bool = True) -> str:
        text = "".join(
            self._sanitized[id] for id in ids
            if id != self._blank_id
            and (not skip_control or id not in self._ctrl_ids)
            and id < len(self._sanitized)
        )
        return text.strip()


# ============================================================================
# Sherpa-onnx model loading  (tokens.txt = sherpa-onnx format)
# ============================================================================
SHERPA_RECOGNIZER = None
SHERPA_SAMPLE_RATE = 16000


def _load_sherpa_model(model_dir: str):
    global SHERPA_RECOGNIZER, SHERPA_SAMPLE_RATE
    import sherpa_onnx

    tokens = os.path.join(model_dir, "tokens.txt")
    encoder = _find_onnx(model_dir, ["encoder.int8.onnx", "encoder.onnx"])
    decoder = _find_onnx(model_dir, ["decoder.int8.onnx", "decoder.onnx"])
    joiner = _find_onnx(model_dir, ["joiner.int8.onnx", "joiner.onnx"])

    print(f"[unified_server] sherpa encoder: {os.path.basename(encoder)}", file=sys.stderr, flush=True)
    print(f"[unified_server] sherpa decoder: {os.path.basename(decoder)}", file=sys.stderr, flush=True)
    print(f"[unified_server] sherpa joiner:  {os.path.basename(joiner)}", file=sys.stderr, flush=True)

    SHERPA_RECOGNIZER = sherpa_onnx.OnlineRecognizer.from_transducer(
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
    SHERPA_SAMPLE_RATE = int(SHERPA_RECOGNIZER.config.feat_config.sampling_rate)
    print(f"[unified_server] sherpa model loaded. sample_rate={SHERPA_SAMPLE_RATE}",
          file=sys.stderr, flush=True)

    return {
        "mode": "sherpa",
        "encoder": SHERPA_RECOGNIZER,
    }


def _find_onnx(model_dir: str, candidates: list[str]) -> str:
    for name in candidates:
        path = os.path.join(model_dir, name)
        if os.path.isfile(path):
            return path
    raise FileNotFoundError(f"No ONNX found in {model_dir} (tried {candidates})")


# ============================================================================
# Model loading  (manual ONNX path for Unified / EOU)
# ============================================================================
MODEL: dict | None = None


def load_model(model_dir: str):
    # --- Sherpa-onnx path (auto-detected: tokens.txt + encoder/decoder/joiner ONNX) ---
    tokens_path = os.path.join(model_dir, "tokens.txt")
    if os.path.isfile(tokens_path):
        return _load_sherpa_model(model_dir)

    import onnxruntime as ort

    has_vocab_txt = os.path.isfile(os.path.join(model_dir, "vocab.txt"))
    has_sp_model = os.path.isfile(os.path.join(model_dir, "tokenizer.model"))

    if has_vocab_txt and not has_sp_model:
        family = "eou"
        encoder_candidates = [
            "encoder-model.fp16.onnx", "encoder-model.onnx", "encoder-model.int8.onnx",
        ]
        decoder_candidates = [
            "decoder_joint-model.int8.onnx", "decoder_joint-model.fp16.onnx",
            "decoder_joint-model.onnx",
        ]
        tokenizer = _VocabTxtTokenizer(model_dir)
        blank_id = tokenizer.blank_id
        eou_id = tokenizer.eou_id
        config_path = os.path.join(model_dir, "config.json")
        config = {}
        if os.path.isfile(config_path):
            import json as _json
            with open(config_path) as f:
                config = _json.load(f)
        pred_layers = int(config.get("pred_layers", 1) or 1)
        pred_hidden = int(config.get("pred_hidden", 640) or 640)
        mel_normalize = False
        print(f"[unified_server] EOU model (vocab={tokenizer.vocab_size}, "
              f"blank={blank_id}, eou={eou_id}, layers={pred_layers})",
              file=sys.stderr, flush=True)
    elif has_sp_model:
        family = "unified"
        encoder_candidates = ["encoder.onnx", "encoder.int8.onnx"]
        decoder_candidates = ["decoder_joint.onnx", "decoder_joint.int8.onnx"]
        tokenizer = _SentencePieceTokenizer(model_dir)
        blank_id = 1024
        eou_id = None
        pred_layers = 2
        pred_hidden = 640
        mel_normalize = True
        config = {}
        print("[unified_server] Unified model (SentencePiece)", file=sys.stderr, flush=True)
    else:
        raise FileNotFoundError(
            f"No tokenizer found in {model_dir} (need tokenizer.model or vocab.txt)"
        )

    enc_path = None
    for name in encoder_candidates:
        candidate = os.path.join(model_dir, name)
        if os.path.isfile(candidate):
            enc_path = candidate
            break
    if enc_path is None:
        raise FileNotFoundError(f"No encoder found in {model_dir}")

    dec_path = None
    for name in decoder_candidates:
        candidate = os.path.join(model_dir, name)
        if os.path.isfile(candidate):
            dec_path = candidate
            break
    if dec_path is None:
        raise FileNotFoundError(f"No decoder found in {model_dir}")

    print(f"[unified_server] encoder: {os.path.basename(enc_path)}", file=sys.stderr, flush=True)
    so = ort.SessionOptions()
    so.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL
    encoder = ort.InferenceSession(enc_path, so, providers=["CPUExecutionProvider"])
    print(f"[unified_server] decoder: {os.path.basename(dec_path)}", file=sys.stderr, flush=True)
    decoder = ort.InferenceSession(dec_path, so, providers=["CPUExecutionProvider"])

    # Inspect decoder ONNX inputs to determine expected dtypes — different
    # ONNX exports use different conventions and we must match exactly.
    decoder_inputs = {inp.name: inp for inp in decoder.get_inputs()}
    _targets_dtype = np.int32  # default
    _has_target_length = False
    for name, meta in decoder_inputs.items():
        if name == "targets":
            onnx_type = meta.type
            if "float" in onnx_type:
                _targets_dtype = np.float32
            elif "int32" in onnx_type:
                _targets_dtype = np.int32
            elif "int64" in onnx_type:
                _targets_dtype = np.int64  # parakeet-rs uses this
        if name == "target_length":
            _has_target_length = True
    print(f"[unified_server] decoder targets dtype={_targets_dtype.__name__}, "
          f"has_target_length={_has_target_length}",
          file=sys.stderr, flush=True)

    mel_basis = _create_mel_filterbank(N_FFT, FEATURE_SIZE, SAMPLE_RATE)

    return {
        "encoder": encoder, "decoder": decoder, "mel_basis": mel_basis,
        "tokenizer": tokenizer,
        "config": {
            "blank_id": blank_id, "eou_id": eou_id,
            "pred_layers": pred_layers, "pred_hidden": pred_hidden,
            "mel_normalize": mel_normalize, "family": family,
            "targets_dtype": _targets_dtype,
            "has_target_length": _has_target_length,
        },
    }


# ============================================================================
# Offline transcription  (full audio → full text)
# ============================================================================
def transcribe(audio: bytes) -> str:
    if MODEL is None:
        raise RuntimeError("Model not loaded")

    # Sherpa-onnx path
    if MODEL.get("mode") == "sherpa":
        r = SHERPA_RECOGNIZER
        samples = np.frombuffer(audio, dtype=np.float32).copy()
        if len(samples) == 0:
            return ""
        s = r.create_stream()
        s.accept_waveform(SHERPA_SAMPLE_RATE, samples)
        tail = np.zeros(int(0.5 * SHERPA_SAMPLE_RATE), dtype=np.float32)
        s.accept_waveform(SHERPA_SAMPLE_RATE, tail)
        s.input_finished()
        while r.is_ready(s):
            r.decode_streams([s])
        return r.get_result(s)

    # Manual ONNX path
    samples = np.frombuffer(audio, dtype=np.float32).copy()
    if len(samples) == 0:
        return ""
    cfg = MODEL["config"]
    features = extract_features(samples, MODEL["mel_basis"], normalize=cfg["mel_normalize"])
    encoded, frame_count = _encoder_forward(MODEL["encoder"], features)
    if frame_count == 0:
        return ""
    result = _decode_frames(
        MODEL, encoded, frame_count, blank_id=cfg["blank_id"],
        n_layers=cfg["pred_layers"], hidden=cfg["pred_hidden"],
        targets_dtype=cfg["targets_dtype"],
    )
    tokens = result["tokens"]
    if not tokens:
        return ""
    return MODEL["tokenizer"].decode(tokens)


# ============================================================================
# Shared RNNT frame-by-frame decoder  (used by both offline and streaming)
# ============================================================================
def _encoder_forward(encoder, features):
    """Run encoder and return (encoded, frame_count). Handles 1 or 2 output models."""
    enc_input = features.T[np.newaxis, :, :]  # (1, 128, T)
    enc_len = np.array([features.shape[0]], dtype=np.int64)
    enc_out = encoder.run(None, {"audio_signal": enc_input, "length": enc_len})
    encoded = enc_out[0]  # (1, D, T_enc)
    if len(enc_out) >= 2:
        frame_count = min(int(enc_out[1][0]), encoded.shape[2])
    else:
        frame_count = encoded.shape[2]
    return encoded, frame_count


def _decode_frames(
    model: dict,
    encoded: np.ndarray,       # (1, D, T_enc)
    frame_count: int,
    blank_id: int,
    n_layers: int,
    hidden: int,
    start_frame: int = 0,
    decoder_state: dict | None = None,
    stop_on_eou: bool = False,
    targets_dtype: np.dtype = np.dtype(np.int32),
):
    """
    Greedy RNNT decoder over encoder frames [start_frame, frame_count).

    Returns (tokens: list[int], last_frame: int, decoder_state: dict, found_eou: bool).

    targets_dtype: np.int32 for EOU models, np.float32 for Unified (SentencePiece) models.
    """
    decoder = model["decoder"]
    tokenizer = model["tokenizer"]
    cfg = model["config"]
    eou_id = cfg.get("eou_id")
    has_target_length = cfg.get("has_target_length", True)

    if decoder_state is None:
        state_1 = np.zeros((n_layers, 1, hidden), dtype=np.float32)
        state_2 = np.zeros((n_layers, 1, hidden), dtype=np.float32)
        last_token = np.array([[blank_id]], dtype=np.int64)
    else:
        state_1 = decoder_state["state_1"]
        state_2 = decoder_state["state_2"]
        last_token = decoder_state["last_token"]

    target_length = np.array([1], dtype=np.int32)
    tokens: list[int] = []
    found_eou = False
    last_processed = start_frame

    for frame_idx in range(start_frame, frame_count):
        frame = encoded[0:1, :, frame_idx:frame_idx + 1]  # (1, D, 1)
        last_processed = frame_idx

        for _ in range(MAX_SYMBOLS_PER_STEP):
            feed = {
                "encoder_outputs": frame,
                "targets": last_token.astype(targets_dtype),
                "input_states_1": state_1,
                "input_states_2": state_2,
            }
            if has_target_length:
                feed["target_length"] = target_length
            d_out = decoder.run(None, feed)
            logits = d_out[0][0, 0, :]
            state_1 = d_out[1]
            state_2 = d_out[2]
            token_id = int(np.argmax(logits))
            if token_id == blank_id:
                break
            tokens.append(token_id)
            last_token = np.array([[token_id]], dtype=np.int64)
            if stop_on_eou and eou_id is not None and token_id == eou_id:
                found_eou = True
                break

        if found_eou:
            break

    return {
        "tokens": tokens,
        "last_frame": last_processed + (0 if found_eou else 1),
        "decoder_state": {
            "state_1": state_1.copy(),
            "state_2": state_2.copy(),
            "last_token": last_token.copy(),
        },
        "found_eou": found_eou,
    }


# ============================================================================
# Streaming state  (single-stream: one client at a time)
# ============================================================================
STREAM: dict = {
    "audio_samples": [],
    "tokens": [],
    "decoder_state": None,
    "decoded_frame": 0,
    "found_eou": False,
}

SHERPA_STREAM = None
STREAM_RESULT_TEXT = ""
STREAM_EOU_FLAG = False


def _stream_feed_sherpa(audio_bytes: bytes) -> dict:
    global STREAM_RESULT_TEXT, STREAM_EOU_FLAG
    if SHERPA_RECOGNIZER is None or SHERPA_STREAM is None:
        return {"text": "", "eou": False}
    samples = np.frombuffer(audio_bytes, dtype=np.float32).copy()
    if len(samples) == 0:
        return {"text": STREAM_RESULT_TEXT, "eou": STREAM_EOU_FLAG}
    SHERPA_STREAM.accept_waveform(SHERPA_SAMPLE_RATE, samples)
    while SHERPA_RECOGNIZER.is_ready(SHERPA_STREAM):
        SHERPA_RECOGNIZER.decode_streams([SHERPA_STREAM])
        STREAM_RESULT_TEXT = SHERPA_RECOGNIZER.get_result(SHERPA_STREAM)
        if SHERPA_RECOGNIZER.is_endpoint(SHERPA_STREAM):
            STREAM_EOU_FLAG = True
    return {"text": STREAM_RESULT_TEXT, "eou": STREAM_EOU_FLAG}


def _stream_reset():
    global SHERPA_STREAM, STREAM_RESULT_TEXT, STREAM_EOU_FLAG
    STREAM["audio_samples"] = []
    STREAM["tokens"] = []
    STREAM["decoder_state"] = None
    STREAM["decoded_frame"] = 0
    STREAM["found_eou"] = False
    STREAM_RESULT_TEXT = ""
    STREAM_EOU_FLAG = False
    if MODEL and MODEL.get("mode") == "sherpa":
        SHERPA_STREAM = SHERPA_RECOGNIZER.create_stream()


def _stream_feed(audio_bytes: bytes) -> dict:
    if MODEL is None:
        raise RuntimeError("Model not loaded")

    if MODEL.get("mode") == "sherpa":
        return _stream_feed_sherpa(audio_bytes)

    new_samples = np.frombuffer(audio_bytes, dtype=np.float32).copy()
    if len(new_samples) == 0 and STREAM["found_eou"]:
        return _stream_result()

    STREAM["audio_samples"].extend(new_samples.tolist())

    cfg = MODEL["config"]
    full_audio = np.array(STREAM["audio_samples"], dtype=np.float32)
    min_samples = int(SAMPLE_RATE * 0.16)  # 160ms minimum
    if len(full_audio) < min_samples:
        return {"text": _decode_stream_tokens(), "eou": False}

    # Recompute mel + encoder on full accumulated audio
    features = extract_features(full_audio, MODEL["mel_basis"], normalize=cfg["mel_normalize"])
    encoded, frame_count = _encoder_forward(MODEL["encoder"], features)

    if frame_count <= STREAM["decoded_frame"]:
        return {"text": _decode_stream_tokens(), "eou": STREAM["found_eou"]}

    # Continue decoding from where we left off
    result = _decode_frames(
        MODEL, encoded, frame_count,
        blank_id=cfg["blank_id"],
        n_layers=cfg["pred_layers"],
        hidden=cfg["pred_hidden"],
        start_frame=STREAM["decoded_frame"],
        decoder_state=STREAM["decoder_state"],
        stop_on_eou=True,
        targets_dtype=cfg["targets_dtype"],
    )

    STREAM["tokens"].extend(result["tokens"])
    STREAM["decoder_state"] = result["decoder_state"]
    STREAM["decoded_frame"] = result["last_frame"]
    if result["found_eou"]:
        STREAM["found_eou"] = True

    return _stream_result()


def _decode_stream_tokens() -> str:
    if not STREAM["tokens"]:
        return ""
    return MODEL["tokenizer"].decode(STREAM["tokens"])


def _stream_result() -> dict:
    return {"text": _decode_stream_tokens(), "eou": STREAM["found_eou"]}


# ============================================================================
# HTTP handler
# ============================================================================
class UnifiedParakeetHandler(BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        print(f"[unified_server] {args[0]}", file=sys.stderr, flush=True)

    def _json(self, code: int, data: dict):
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(data).encode())

    def do_GET(self):
        if self.path in ("/health", "/"):
            self._json(200, {"status": "ok"})
        elif self.path == "/stream_status":
            try:
                self._json(200, _stream_result())
            except Exception:
                self.send_response(500)
                self.end_headers()
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
                _stream_reset()
                self._json(200, {"status": "ok"})

            elif self.path == "/stream_feed":
                result = _stream_feed(body)
                self._json(200, result)

            elif self.path == "/stream_end":
                # Feed any remaining audio, then finalise
                if body:
                    result = _stream_feed(body)
                else:
                    result = _stream_result()
                self._json(200, result)

            else:
                self.send_response(404)
                self.end_headers()

        except Exception:
            self.send_response(500)
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            tb = traceback.format_exc()
            print(f"[unified_server] Error:\n{tb}", file=sys.stderr, flush=True)
            self.wfile.write(tb.encode())


# ============================================================================
# Main
# ============================================================================
def resolve_model_dir(model_dir_arg: str | None) -> str:
    if model_dir_arg and os.path.isdir(model_dir_arg):
        return model_dir_arg
    script_dir = os.path.dirname(os.path.abspath(__file__))
    cwd = os.getcwd()
    for base in [script_dir, cwd]:
        for sub in [
            "models/STT/parakeet-unified-en-0.6b-int8",
            "models/STT/parakeet-unified-en-0.6b-fp32",
            "models/STT/parakeet-eou-120m-en-fp32int8",
            "models/STT/parakeet-eou-120m-en-fp16",
            "models/STT/parakeet-eou-120m-en-fp32",
        ]:
            candidate = os.path.join(base, sub)
            if os.path.isdir(candidate):
                return candidate
    raise FileNotFoundError(
        "No Parakeet ONNX model directory found. "
        "Download the model first or pass --model-dir."
    )


def main():
    parser = argparse.ArgumentParser(description="Parakeet ONNX STT HTTP server")
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--model-dir", default=None, help="Path to model directory")
    args = parser.parse_args()

    global MODEL
    model_dir = resolve_model_dir(args.model_dir)
    print(f"[unified_server] Model directory: {model_dir}", file=sys.stderr, flush=True)
    MODEL = load_model(model_dir)
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
