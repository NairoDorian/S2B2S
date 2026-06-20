# ONNX Export: Parakeet Models → sherpa-onnx streaming format

Exports Parakeet models to sherpa-onnx buffered streaming format using
scripts from [k2-fsa/sherpa-onnx PR #3575](https://github.com/k2-fsa/sherpa-onnx/pull/3575).

## Models to export

| Model           | NeMo Checkpoint                                                                                     | Size   | Output                                        |
| --------------- | --------------------------------------------------------------------------------------------------- | ------ | --------------------------------------------- |
| Unified EN 0.6B | [nvidia/parakeet-unified-en-0.6b](https://huggingface.co/nvidia/parakeet-unified-en-0.6b)           | 2.6 GB | encoder/decoder/joiner.int8.onnx + tokens.txt |
| EOU 120M v1     | [nvidia/parakeet_realtime_eou_120m-v1](https://huggingface.co/nvidia/parakeet_realtime_eou_120m-v1) | TBD    | TBD (export script to be created)             |

## Setup

```powershell
# Windows
powershell -File setup_venv.ps1  # creates venv + installs NeMo + deps (~15 min)

# macOS/Linux
bash setup_venv.sh
```

## Export Unified 0.6B

```powershell
powershell -File export_unified.ps1
```

Output goes to `sherpa-onnx-nemo-parakeet-unified-en-0.6b-int8-streaming-560ms/`.

## Usage with S2B2S

Copy the output folder to `models/STT/` and the unified_parakeet_server.py
auto-detects the sherpa-onnx format (tokens.txt present) and routes through
sherpa-onnx's OnlineRecognizer instead of the manual ONNX path.

## Dependencies

- NVIDIA NeMo (`nemo_toolkit[asr]` from GitHub main)
- kaldi-native-fbank, librosa, onnx, onnxruntime, soundfile, numpy<2
