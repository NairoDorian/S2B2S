// scripts/build-options.ts
//
// Tunable build posture options for the native transcribe.cpp build.
//
// Separates the configuration for `fast` (local machine dev/release) and `full`
// (distribution release / multi-arch matrix), allowing each to be customized
// independently.
//
// Used by `scripts/tauri-runner.ts` when running:
//   - bun run tauri dev:fast   / bun run build:fast   (--fast)
//   - bun run tauri dev:full   / bun run build:full   (--full)
//   - bun run tauri dev:cpu    / bun run build:cpu    (--cpu)

import { FULL_MODEL_SET } from "./lib/cpu-lane";

export interface PostureOptions {
  /**
   * Target CUDA architecture policy:
   * - "auto": auto-probes the local machine's GPU (e.g. sm_89 for an RTX 4070)
   *   for a fast single-arch nvcc compile.
   * - "default": hands the architecture list to ggml-cuda's own CMake default,
   *   the full distribution matrix (sm_50 upward, extended to Blackwell on
   *   toolkits new enough to add it — so the exact list tracks the CUDA
   *   toolkit installed, not this file).
   * - Explicit semicolon-delimited list: e.g. "75;80;86;89"
   */
  cudaArchitectures: string;

  /**
   * CMake `-D` definition arguments passed to transcribe.cpp via TRANSCRIBE_CMAKE_ARGS.
   */
  cmakeArgs: string[];

  /**
   * Model set to compile into libtranscribe.
   * Defaults to FULL_MODEL_SET ("full") — all 19 architecture families.
   * Can be set to "minimal-multilingual" or a custom set if needed.
   */
  modelSet?: string;

  /**
   * Optional custom environment variables injected for this build posture.
   */
  env?: Record<string, string>;

  /**
   * Human-readable description printed by tauri-runner in the startup banner.
   */
  description: string;
}

/**
 * Options for the `fast` build posture (`--fast`, `dev:fast`, `build:fast`).
 *
 * Configured specifically for rapid iteration and maximum performance on the
 * host machine:
 * - Single CUDA arch, auto-probed from the local GPU, for fast nvcc compilation.
 * - The conservative x86 ISA floor lifted (`TRANSCRIBE_X86_CONSERVATIVE=OFF`,
 *   `GGML_NATIVE=ON`) so the host's AVX2 / FMA / AVX-VNNI are used.
 * - Flash Attention left at ggml's default: the kernels for the F16 KV cache
 *   transcribe.cpp actually runs are compiled, the extra quantized-KV ones are
 *   not (see the note in `cmakeArgs` below).
 */
export const FAST_BUILD_OPTIONS: PostureOptions = {
  cudaArchitectures: "auto",
  cmakeArgs: [
    "-DTRANSCRIBE_VULKAN=ON", // Vulkan backend (Intel iGPU & NVIDIA)
    // GGML_CUDA_FA_QUANTS is deliberately unset: it only adds kernels for
    // quantized KV caches, and transcribe.cpp runs an F16 KV cache.
    "-DTRANSCRIBE_X86_CONSERVATIVE=OFF", // Lift conservative ISA floor for host CPU
    "-DGGML_NATIVE=ON", // Compiler native tuning (-march=native / /arch:AVX2)
    "-DGGML_AVX2=ON", // AVX2 SIMD instructions
    "-DGGML_FMA=ON", // Fused multiply-add
    "-DGGML_AVX_VNNI=ON", // AVX-VNNI (INT8 matrix acceleration on 13th Gen+)
  ],
  modelSet: FULL_MODEL_SET,
  env: {},
  description:
    "this machine's GPU only, Flash Attention & Vulkan enabled, native AVX2/VNNI",
};

/**
 * Options for the `full` build posture (`--full`, `dev:full`, `build:full`).
 *
 * Configured for multi-architecture release and distribution packages:
 * - Multi-architecture CUDA matrix (`default` covers all supported NVIDIA GPUs).
 * - Conservative host CPU floor (TRANSCRIBE_X86_CONSERVATIVE=ON, which
 *   `dynamic-backends` forces on x86 anyway) so binaries run on any x86_64 CPU
 *   without crashing on missing ISA instructions (SIGILL).
 * - Flash Attention compiled in for the F16 KV cache on every architecture in
 *   the matrix; the extra quantized-KV kernels stay off (see `cmakeArgs`).
 *
 * Edit this object to change or add any options specific to `build:full`.
 */
export const FULL_BUILD_OPTIONS: PostureOptions = {
  // Target CUDA architecture: "default" targets the full distribution matrix.
  // Change to a specific list (e.g. "75;80;86;89") if you wish to restrict the release matrix.
  cudaArchitectures: "default",

  // CMake arguments for full/distribution builds:
  cmakeArgs: [
    "-DTRANSCRIBE_VULKAN=ON", // Vulkan backend (Intel iGPU & NVIDIA)
    // GGML_CUDA_FA_QUANTS is deliberately unset: it only adds kernels for
    // quantized KV caches, and transcribe.cpp runs an F16 KV cache.
    // Example additional options you can enable for full builds:
    // (GGML_CUDA_GRAPHS is ON by default: transcribe-cpp-sys passes it for
    // every CUDA build. Never add -DGGML_CUDA_GRAPHS=OFF.)
    // "-DTRANSCRIBE_BUILD_TESTS=OFF",
  ],

  modelSet: FULL_MODEL_SET,

  // Custom environment variables for full builds:
  env: {
    // Example:
    // TRANSCRIBE_FORCE_REBUILD: "1",
  },

  description: "full architecture matrix, Flash Attention enabled",
};
