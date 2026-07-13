import os
import sys

# Add standard Windows CUDA bin directory to the DLL search path
cuda_bin = r"C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v13.3\bin"
if os.path.isdir(cuda_bin):
    os.add_dll_directory(cuda_bin)

# Setup HF_HOME to point to S2B2S models dir to keep downloads contained
models_dir = os.path.abspath(r"models\TTS")
os.environ["HF_HOME"] = models_dir
print(f"Using HF_HOME: {models_dir}")

try:
    from qwentts_cpp import QwenTTS
    print("SUCCESS: qwentts_cpp imported!")
except Exception as e:
    print("FAILED importing qwentts_cpp:", e)
    sys.exit(1)

try:
    print("Loading model from pretrained (quant=Q4_K_M)...")
    # This downloads the Q8_0 GGUF weights from Serveurperso/Qwen3-TTS-GGUF
    tts = QwenTTS.from_pretrained("Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice", quant="Q8")
    print("SUCCESS: Model loaded successfully!")
    print("Available speakers:", tts.speaker_names())
    
    print("Synthesizing audio: 'Hello, testing local compilation of Qwen3-TTS'...")
    audio, sr = tts.synthesize(text="Hello, testing local compilation of Qwen3-TTS.", speaker="Aiden", lang="english")
    print(f"SUCCESS: Synthesized {len(audio)} samples at {sr} Hz!")
    
    # Save the output WAV
    import soundfile as sf
    sf.write("scratch_qwen3_test.wav", audio, sr)
    print("SUCCESS: Saved output to scratch_qwen3_test.wav!")
except Exception as e:
    print("FAILED during model execution:", e)
