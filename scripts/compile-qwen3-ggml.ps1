# compile-qwen3-ggml.ps1
# Automates the native C++ compilation of qwentts.cpp and qwentts-cpp-python on Windows 11 with CUDA support.

param(
    [string]$ParentDir = ".."
)

$ErrorActionPreference = "Stop"

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Qwen3-TTS Native GGML Compiler Automation" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

# 1. Verify Prerequisites
if (-not (Get-Command "cmake" -ErrorAction SilentlyContinue)) {
    Write-Error "CMake is not installed or not in PATH. Please install CMake."
}
if (-not (Get-Command "nvcc" -ErrorAction SilentlyContinue)) {
    Write-Error "NVIDIA CUDA compiler (nvcc) not found in PATH. Please install CUDA Toolkit."
}
if (-not (Get-Command "git" -ErrorAction SilentlyContinue)) {
    Write-Error "Git is not installed or not in PATH."
}

# Resolve paths
$S2B2S_Dir = Get-Location
$VenvPython = Join-Path $S2B2S_Dir "venv\Scripts\python.exe"
$UvExe = Join-Path $S2B2S_Dir "venv\uv.exe"
if (-not (Test-Path $UvExe)) {
    $UvExe = "uv" # fallback
}

$ParentPath = Resolve-Path $ParentDir
$CppRepoDir = Join-Path $ParentPath "qwentts.cpp"
$WrapperRepoDir = Join-Path $ParentPath "qwentts-cpp-python"
$FasterRepoDir = Join-Path $ParentPath "faster-qwen3-tts"

# 2. Clone qwentts.cpp
if (-not (Test-Path $CppRepoDir)) {
    Write-Host "Cloning qwentts.cpp..." -ForegroundColor Yellow
    & git clone --recurse-submodules https://github.com/ServeurpersoCom/qwentts.cpp $CppRepoDir
} else {
    Write-Host "qwentts.cpp folder already exists." -ForegroundColor Green
}

# 3. Clone qwentts-cpp-python
if (-not (Test-Path $WrapperRepoDir)) {
    Write-Host "Cloning qwentts-cpp-python..." -ForegroundColor Yellow
    & git clone https://github.com/andimarafioti/qwentts-cpp-python $WrapperRepoDir
} else {
    Write-Host "qwentts-cpp-python folder already exists." -ForegroundColor Green
}

# 4. Clone faster-qwen3-tts
if (-not (Test-Path $FasterRepoDir)) {
    Write-Host "Cloning faster-qwen3-tts..." -ForegroundColor Yellow
    & git clone https://github.com/andimarafioti/faster-qwen3-tts $FasterRepoDir
} else {
    Write-Host "faster-qwen3-tts folder already exists." -ForegroundColor Green
}

# 5. Compile C++ libraries natively
Write-Host "Configuring and compiling qwentts.cpp with CUDA support..." -ForegroundColor Yellow
cd $CppRepoDir
if (Test-Path "build") {
    Remove-Item -Recurse -Force "build"
}
& cmake -S . -B build -DGGML_CUDA=ON -DQWEN_SHARED=ON
& cmake --build build --config Release -j

# 6. Patch Wrapper files for Windows loader
Write-Host "Applying Windows DLL loading patches to wrapper..." -ForegroundColor Yellow
$BindingPy = Join-Path $WrapperRepoDir "src\qwentts_cpp\_binding.py"

# Read, replace, and write back
$content = [System.IO.File]::ReadAllText($BindingPy)
if (-not $content.Contains("ggml-cuda.dll")) {
    # Replace dependency names
    $content = $content.Replace("def _dependency_names() -> Sequence[str]:`n    return (", "def _dependency_names() -> Sequence[str]:`n    if sys.platform == 'win32':`n        return ('ggml-base.dll', 'ggml-cpu.dll', 'ggml-cuda.dll', 'ggml.dll')`n    return (")
    $content = $content.Replace("def _dependency_names() -> Sequence[str]:`r`n    return (", "def _dependency_names() -> Sequence[str]:`r`n    if sys.platform == 'win32':`r`n        return ('ggml-base.dll', 'ggml-cpu.dll', 'ggml-cuda.dll', 'ggml.dll')`r`n    return (")

    # Insert DLL directory lookup right at the start of _load_cdll
    $loadCdllStart = "def _load_cdll(self) -> None:"
    $loadCdllReplace = "def _load_cdll(self) -> None:`r`n        if sys.platform == 'win32' and hasattr(os, 'add_dll_directory'):`r`n            self._dll_dir_handle = os.add_dll_directory(lib_dir)`r`n            cuda_path = os.environ.get('CUDA_PATH')`r`n            if cuda_path:`r`n                os.add_dll_directory(os.path.join(cuda_path, 'bin'))`r`n            standard_cuda = r'C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v13.3\bin'`r`n            if os.path.isdir(standard_cuda):`r`n                os.add_dll_directory(standard_cuda)`r`n            import glob`r`n            for path_dir in sys.path:`r`n                nvidia_dir = os.path.join(path_dir, 'nvidia')`r`n                if os.path.isdir(nvidia_dir):`r`n                    bin_dirs = glob.glob(os.path.join(nvidia_dir, '*', 'bin'))`r`n                    sub_dirs = glob.glob(os.path.join(nvidia_dir, '*', 'bin', '*'))`r`n                    for d in bin_dirs + sub_dirs:`r`n                        if os.path.isdir(d):`r`n                            os.add_dll_directory(d)"
    $content = $content.Replace($loadCdllStart, $loadCdllReplace)
    [System.IO.File]::WriteAllText($BindingPy, $content)
}

# 7. Build Wrapper & Copy DLLs
Write-Host "Packaging wrapper and copying DLLs..." -ForegroundColor Yellow
cd $WrapperRepoDir
& $VenvPython scripts/build_native.py --skip-build --build-dir $CppRepoDir\build

# 8. Install packages inside S2B2S venv
Write-Host "Installing qwentts-cpp-python in S2B2S venv..." -ForegroundColor Yellow
cd $S2B2S_Dir
& $UvExe pip install -e $WrapperRepoDir --python $VenvPython

Write-Host "Installing faster-qwen3-tts in S2B2S venv..." -ForegroundColor Yellow
& $UvExe pip install -e $FasterRepoDir --python $VenvPython

Write-Host "Native Qwen3-TTS compile and install completed successfully!" -ForegroundColor Green
