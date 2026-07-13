#!/usr/bin/env python3
"""
HTTP server for Qwen3-TTS — keeps the model loaded in GPU/RAM for low-latency synthesis.

Exposes a Piper-compatible HTTP contract, supporting:
- Custom voice generation (pre-defined speakers like Aiden, Ashley, etc.)
- Voice cloning (In-Context Learning using ref_audio + ref_text)
- Voice design (Text-instructed voice generation)

Usage:
    python qwen3_server.py --port 51237
"""

import argparse
import io
import json
import os
import sys
import traceback
import wave
from http.server import HTTPServer, BaseHTTPRequestHandler
import numpy as np
import torch

try:
    from qwentts_cpp import QwenTTS
except ImportError:
    QwenTTS = None

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------
DEFAULT_MODEL = "Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice"
DEFAULT_VOICE = "Aiden"
DEFAULT_REF_TEXT = "I'm confused why some people have super short timelines, yet at the same time are bullish on scaling up reinforcement learning atop LLMs."
SAMPLE_RATE = 24000

# ---------------------------------------------------------------------------
# Globals set at startup
# ---------------------------------------------------------------------------
MODEL = None
AVAILABLE_VOICES = []
MODEL_TYPE = "custom_voice"

def resolve_local_models_dir():
    """Find the local TTS models directory relative to this script."""
    script_dir = os.path.dirname(os.path.abspath(__file__))
    candidates = [
        os.path.join(script_dir, "models", "TTS"),
        os.path.join(os.getcwd(), "models", "TTS"),
        os.path.join(script_dir, "models"),
        os.path.join(os.getcwd(), "models"),
    ]
    for p in candidates:
        if os.path.isdir(p):
            return p
    return None

def load_model(model_name=DEFAULT_MODEL, device="cuda", models_dir=None, backend="ggml"):
    """Load Qwen3-TTS model via faster-qwen3-tts or qwentts_cpp once."""
    if backend == "ggml":
        if QwenTTS is None:
            raise ImportError("qwentts_cpp package not found or fails to load. Please verify local compilation.")
            
        if sys.platform == "win32":
            # Ensure CUDA and system paths are loaded for ctypes DLL loader
            cuda_bin = r"C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v13.3\bin"
            if os.path.isdir(cuda_bin):
                try:
                    os.add_dll_directory(cuda_bin)
                except Exception:
                    pass
            cuda_path = os.environ.get("CUDA_PATH")
            if cuda_path:
                cuda_path_bin = os.path.join(cuda_path, "bin")
                if os.path.isdir(cuda_path_bin):
                    try:
                        os.add_dll_directory(cuda_path_bin)
                    except Exception:
                        pass
        
        print(f"[qwen3_server] Initializing {model_name} (backend: ggml, quant: Q8_0, cache: {models_dir})...", flush=True)
        # Note: We use quant="Q8" (Q8_0) since K-quants (like Q4_K_M) are not supported by qwentts.cpp CUDA row-getting kernels.
        model = QwenTTS.from_pretrained(
            model_name,
            quant="Q8",
            cache_dir=models_dir
        )
        return model

    from faster_qwen3_tts import FasterQwen3TTS

    if models_dir and os.path.isdir(models_dir):
        os.environ.setdefault("HF_HOME", models_dir)

    dtype = torch.bfloat16 if torch.cuda.is_available() and torch.cuda.is_bf16_supported() else torch.float16
    print(f"[qwen3_server] Initializing {model_name} on {device} with {dtype} (backend: {backend})...", flush=True)

    model = FasterQwen3TTS.from_pretrained(
        model_name,
        device=device,
        dtype=dtype,
        attn_implementation="eager",
        backend=backend
    )

    # Try warm up
    try:
        model._warmup(prefill_len=100)
    except Exception as e:
        print(f"[qwen3_server] Warmup warning: {e}", flush=True)

    return model

def infer_model_type(model):
    """Detect whether this is a CustomVoice, VoiceDesign, or Base model."""
    if QwenTTS is not None and isinstance(model, QwenTTS):
        if model.speaker_names():
            return "custom_voice"
        return "base"

    # Look into model config or naming
    inner_model = getattr(getattr(model, "model", None), "model", None)
    config = getattr(inner_model, "config", None) if inner_model else None
    model_type = getattr(config, "tts_model_type", None)
    
    if model_type:
        return model_type
        
    model_name_lower = str(getattr(model, "model_name", "")).lower()
    if "voicedesign" in model_name_lower:
        return "voice_design"
    if "customvoice" in model_name_lower:
        return "custom_voice"
    return "base"

def get_supported_speakers(model):
    """Retrieve speakers supported by CustomVoice."""
    if QwenTTS is not None and isinstance(model, QwenTTS):
        return model.speaker_names()

    for candidate in (model, getattr(model, "model", None), getattr(getattr(model, "model", None), "model", None)):
        get_speakers = getattr(candidate, "get_supported_speakers", None)
        if callable(get_speakers):
            speakers = get_speakers()
            if speakers:
                return [str(s) for s in speakers if s]
    return ["Aiden", "Ashley", "Ben", "Cora", "Daniel", "Elsa", "Felix", "Grace", "Hale", "Iris", "Jack", "Katherine"]

def synthesize(text, voice, length_scale, voice_wav=None, voice_text=None, instruct=None):
    """Run inference, return WAV bytes."""
    global MODEL, MODEL_TYPE, SAMPLE_RATE
    
    # Coalesce inputs
    text = text.strip()
    if not text:
        text = "Hello."

    if QwenTTS is not None and isinstance(MODEL, QwenTTS):
        # Native GGML backend
        print(f"[qwen3_server] Synthesizing via native C++ GGML backend...", flush=True)
        
        ref_audio_24k = None
        ref_text = None
        if voice_wav and os.path.isfile(voice_wav):
            import librosa
            print(f"[qwen3_server] Loading ref audio: {voice_wav}", flush=True)
            ref_audio_24k, _ = librosa.load(voice_wav, sr=24000)
            ref_text = voice_text if voice_text else DEFAULT_REF_TEXT

        audio, sr = MODEL.synthesize(
            text=text,
            lang="english", # standard qwentts_cpp defaults to english/chinese
            speaker=voice if not voice_wav else None,
            ref_audio_24k=ref_audio_24k,
            ref_text=ref_text,
            instruct=instruct
        )
        
        buf = io.BytesIO()
        w = wave.open(buf, "wb")
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(sr)
        
        # Clip and convert to int16 PCM
        int16 = np.clip(audio * 32767, -32768, 32767).astype(np.int16)
        w.writeframes(int16.tobytes())
        w.close()
        return buf.getvalue()

    # PyTorch fallback backend
    chunks = []
    sr = SAMPLE_RATE

    if voice_wav and os.path.isfile(voice_wav):
        # Voice cloning / ICL
        ref_text = voice_text if voice_text else DEFAULT_REF_TEXT
        print(f"[qwen3_server] Synthesizing voice clone. Ref audio: {voice_wav}", flush=True)
        gen = MODEL.generate_voice_clone_streaming(
            text=text,
            language="auto",
            ref_audio=voice_wav,
            ref_text=ref_text,
            non_streaming_mode=True
        )
    elif MODEL_TYPE == "voice_design" or (instruct and instruct.strip()):
        # Voice design
        inst_prompt = instruct if instruct else f"A speech by {voice}."
        print(f"[qwen3_server] Synthesizing voice design. Instruct: {inst_prompt}", flush=True)
        gen = MODEL.generate_voice_design_streaming(
            text=text,
            instruct=inst_prompt,
            language="auto",
            non_streaming_mode=True
        )
    else:
        # Custom voice
        print(f"[qwen3_server] Synthesizing custom voice. Speaker: {voice}", flush=True)
        gen = MODEL.generate_custom_voice_streaming(
            text=text,
            speaker=voice,
            language="auto",
            non_streaming_mode=True
        )

    for item in gen:
        if isinstance(item, tuple):
            audio_chunk, chunk_sr, _ = item
            chunks.append(np.asarray(audio_chunk, dtype=np.float32).squeeze())
            sr = chunk_sr
        else:
            audio = getattr(item, "audio", None)
            if audio is not None:
                chunks.append(np.asarray(audio, dtype=np.float32).squeeze())
                sr = getattr(item, "sample_rate", sr)

    if not chunks:
        raise ValueError("Synthesis yielded zero audio chunks")

    waveform = np.concatenate(chunks)
    
    # Clip and convert to int16 PCM
    int16 = np.clip(waveform * 32767, -32768, 32767).astype(np.int16)
    pcm = int16.tobytes()

    buf = io.BytesIO()
    w = wave.open(buf, "wb")
    w.setnchannels(1)
    w.setsampwidth(2)
    w.setframerate(sr)
    w.writeframes(pcm)
    w.close()
    
    return buf.getvalue()

# ---------------------------------------------------------------------------
# HTTP Handler
# ---------------------------------------------------------------------------
class Qwen3Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        print(f"[qwen3_server] {args[0]}", file=sys.stderr, flush=True)

    def do_GET(self):
        if self.path in ("/voices", "/health", "/"):
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            body = json.dumps({"voices": AVAILABLE_VOICES, "default": DEFAULT_VOICE})
            self.wfile.write(body.encode())
        else:
            self.send_response(404)
            self.end_headers()

    def do_POST(self):
        try:
            content_length = int(self.headers.get("Content-Length", 0))
            body = self.rfile.read(content_length)
            req = json.loads(body)

            text = req.get("text", "")
            voice = req.get("voice", DEFAULT_VOICE)
            length_scale = req.get("length_scale", 1.0)
            voice_wav = req.get("voice_wav")
            voice_text = req.get("voice_text")
            instruct = req.get("instruct")

            if not text.strip():
                self.send_response(400)
                self.end_headers()
                self.wfile.write(b"empty text")
                return

            # Check if voice is supported (or if cloning is requested via voice_wav)
            if not voice_wav and AVAILABLE_VOICES and voice not in AVAILABLE_VOICES:
                print(f"[qwen3_server] Unknown voice '{voice}', falling back to {DEFAULT_VOICE}", file=sys.stderr, flush=True)
                voice = DEFAULT_VOICE

            wav_bytes = synthesize(
                text=text,
                voice=voice,
                length_scale=length_scale,
                voice_wav=voice_wav,
                voice_text=voice_text,
                instruct=instruct
            )

            self.send_response(200)
            self.send_header("Content-Type", "audio/wav")
            self.send_header("Content-Length", str(len(wav_bytes)))
            self.end_headers()
            self.wfile.write(wav_bytes)

        except Exception:
            self.send_response(500)
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            tb = traceback.format_exc()
            print(f"[qwen3_server] Synthesis error:\n{tb}", file=sys.stderr, flush=True)
            self.wfile.write(tb.encode())

def main():
    parser = argparse.ArgumentParser(description="Qwen3-TTS HTTP server")
    parser.add_argument("--port", type=int, required=True, help="TCP port to listen on")
    parser.add_argument("--host", default="127.0.0.1", help="Bind address")
    parser.add_argument("--model", default=DEFAULT_MODEL, help=f"Model name (default: {DEFAULT_MODEL})")
    parser.add_argument("--device", default="cuda", help="Inference device (cuda / cpu)")
    parser.add_argument("--models-dir", default=None, help="Directory for storing downloaded models")
    parser.add_argument("--backend", default="ggml", choices=["ggml", "torch"], help="Inference backend (default: ggml)")
    args = parser.parse_args()

    global MODEL, AVAILABLE_VOICES, MODEL_TYPE

    # Use cuda if available
    device = args.device
    if device == "cuda" and not torch.cuda.is_available():
        print("[qwen3_server] CUDA requested but not available. Falling back to cpu.", file=sys.stderr, flush=True)
        device = "cpu"

    models_dir = args.models_dir or resolve_local_models_dir()
    if models_dir:
        print(f"[qwen3_server] Models dir: {models_dir}", file=sys.stderr, flush=True)

    MODEL = load_model(model_name=args.model, device=device, models_dir=models_dir, backend=args.backend)
    MODEL_TYPE = infer_model_type(MODEL)
    AVAILABLE_VOICES = get_supported_speakers(MODEL)

    print(f"[qwen3_server] Model {args.model} loaded. Type={MODEL_TYPE}.", file=sys.stderr, flush=True)
    print(f"[qwen3_server] Voices: {AVAILABLE_VOICES}", file=sys.stderr, flush=True)

    server = HTTPServer((args.host, args.port), Qwen3Handler)
    print(f"[qwen3_server] Listening on http://{args.host}:{args.port}", file=sys.stderr, flush=True)

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("[qwen3_server] Shutting down", file=sys.stderr, flush=True)
        server.server_close()

if __name__ == "__main__":
    main()
