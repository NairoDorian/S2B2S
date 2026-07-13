# Compilation and Setup Report: Qwen3-TTS GGML C++ Backend (CUDA 13.3 + Windows 11)

This report documents the findings, exact steps, and command lines used to compile and install the high-performance native **Qwen3-TTS C++ GGML backend** (`qwentts.cpp` and `qwentts-cpp-python` wrapper) on **Windows 11 with CUDA 13.3**.

---

## 1. The Core Finding: Platform Restrictions
* **The Problem**: The pre-built wheels for `qwentts-cpp-python` hosted on PyPI and Hugging Face (`datasets/andito/qwentts-cpp-python-wheels`) only target **Linux** (`manylinux_2_39_x86_64` and `manylinux_2_39_aarch64`). There are no compiled Windows binaries (`win_amd64`) available.
* **The Solution**: Natively compile the C++ shared libraries on Windows 11 using CMake + MSVC + NVCC, patch the python wrapper to load dependencies correctly on Windows, and install it locally inside S2B2S's virtual environment.

---

## 2. Compilation and Packaging Steps

### Step 1: Clone Repositories and Fetch Submodules
We cloned both the native engine and the python wrapper into directories adjacent to S2B2S:
```powershell
# Clone qwentts.cpp and initialize submodules
git clone https://github.com/ServeurpersoCom/qwentts.cpp c:\Users\Z\Downloads\PROJECTS\STT_BRAIN_TTS\qwentts.cpp
cd c:\Users\Z\Downloads\PROJECTS\STT_BRAIN_TTS\qwentts.cpp
git submodule update --init --recursive

# Clone the ctypes python wrapper
cd ..
git clone https://github.com/andimarafioti/qwentts-cpp-python c:\Users\Z\Downloads\PROJECTS\STT_BRAIN_TTS\qwentts-cpp-python
```

### Step 2: Configure and Compile the C++ Shared Library
Run CMake to generate build files with CUDA support and shared library targets enabled, then compile in Release mode:
```powershell
cd c:\Users\Z\Downloads\PROJECTS\STT_BRAIN_TTS\qwentts.cpp
cmake -S . -B build -DGGML_CUDA=ON -DQWEN_SHARED=ON
cmake --build build --config Release -j
```
* **Output**: The compiled DLLs (`qwen.dll`, `ggml.dll`, `ggml-base.dll`, `ggml-cpu.dll`, `ggml-cuda.dll`) are generated under `build\Release\`.

### Step 3: Patch the Wrapper for Windows DLL Resolution
Windows requires dependencies of a DLL to be in the DLL search path or pre-loaded. We modified `src/qwentts_cpp/_binding.py` in `qwentts-cpp-python`:

1. **Add `ggml-cuda.dll` to dependency loading on Windows**:
   ```python
   def _dependency_names() -> Sequence[str]:
       if sys.platform == "win32":
           return ("ggml-base.dll", "ggml-cpu.dll", "ggml-cuda.dll", "ggml.dll")
   ```
2. **Inject standard CUDA and `venv` DLL directories**:
   Inside `_load_cdll`, discover and add directories containing required CUDA runtimes (such as `cudart64_13.dll`):
   ```python
   if sys.platform == "win32" and hasattr(os, "add_dll_directory"):
       self._dll_dir_handle = os.add_dll_directory(lib_dir)
       # Add system CUDA Toolkit bin folder
       cuda_path = os.environ.get("CUDA_PATH")
       if cuda_path:
           os.add_dll_directory(os.path.join(cuda_path, "bin"))
       # Fallback standard path
       standard_cuda = r"C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v13.3\bin"
       if os.path.isdir(standard_cuda):
           os.add_dll_directory(standard_cuda)
       # Discover venv's internal nvidia packages for runtime dlls
       import glob
       for path_dir in sys.path:
           nvidia_dir = os.path.join(path_dir, "nvidia")
           if os.path.isdir(nvidia_dir):
               bin_dirs = glob.glob(os.path.join(nvidia_dir, "*", "bin"))
               sub_dirs = glob.glob(os.path.join(nvidia_dir, "*", "bin", "*"))
               for d in bin_dirs + sub_dirs:
                   if os.path.isdir(d):
                       os.add_dll_directory(d)
   ```

### Step 4: Package and Local Installation in editable mode
Copy the compiled DLLs into the package structure and install it inside S2B2S's virtual environment:
```powershell
cd c:\Users\Z\Downloads\PROJECTS\STT_BRAIN_TTS\qwentts-cpp-python

# Copy native libraries
..\S2B2S\venv\Scripts\python.exe scripts\build_native.py --skip-build --build-dir ..\qwentts.cpp\build

# Install inside the S2B2S virtual environment in editable mode
uv pip install -e . --python ..\S2B2S\venv
```

### Step 5: Install `faster-qwen3-tts[ggml]`
Now, run the final package installation inside `S2B2S`:
```powershell
cd c:\Users\Z\Downloads\PROJECTS\STT_BRAIN_TTS\S2B2S
uv pip install "faster-qwen3-tts[ggml]" --python venv
```
Since the `qwentts-cpp-python` dependency is satisfied by our locally compiled package, `uv` installs `faster-qwen3-tts[ggml]` cleanly without attempting to download incompatible Linux wheels.

---

## 3. Verification Commands
The installation and native library loading were successfully verified inside the `venv`:
```powershell
# 1. Verify ctypes library load
venv\Scripts\python.exe -c "from qwentts_cpp import QwenLibrary; print(QwenLibrary().version())"
# Output: d17c33d (2026-07-12)

# 2. Verify faster-qwen3-tts imports
venv\Scripts\python.exe -c "from faster_qwen3_tts import FasterQwen3TTS"
# Output: Success (Exit Code 0)
```

---

## 4. Critical Architectural Findings

### Finding A: The ICL Phoneme Bleed Artifact
* **The Issue**: In In-Context Learning (ICL) voice cloning mode, text and codec token embeddings are summed positionally across the length of the reference audio. The final prefill position matches the very last token of the reference wave. Consequently, the first generated word begins conditioning on whatever acoustic consonant or phoneme cluster the reference ends on (e.g. producing an unintended "thumbs" or "comes" sound).
* **The Fix**: S2B2S's wrapper automatically appends **0.5 seconds of PCM silence** to the reference audio before building the generation prompt context. This flushes the acoustic state cleanly, eliminating phoneme bleed.

### Finding B: K-Quants CUDA Constraint
* **The Issue**: The custom row-getting kernels (`getrows.cu`) inside `qwentts.cpp`'s GPU backend do not implement k-quantizations (like `Q4_K_M` or `Q5_K_M` which mixed-quantize layers using `Q6_K`). Loading a `Q4_K_M` model on CUDA results in a GPU kernel crash:
  ```
  unsupported src0 type: q6_K
  ```
* **The Fix**: S2B2S utilizes uniform `Q8_0` (`quant="Q8"`) quantization instead. `Q8_0` uses regular block sizes and is fully compatible with the compiled CUDA row-getting kernels.

---

## 5. Automated Build Setup Script

Advanced users can automate the entire clone, patch, native compile, and local venv packaging workflow by running:
```powershell
.\scripts\compile-qwen3-ggml.ps1
```
This script handles the configuration, Windows binding file injection, MSVC C++ Release compilation, and editable installs automatically.
