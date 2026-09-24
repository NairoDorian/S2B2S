// scripts/build-options.ts
//
// Tunable build posture options for zer0 (Handy_V2).
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
   * - "auto": auto-probes the local machine's GPU (e.g. sm_89 for RTX 4070)
   * - "default": builds the full multi-arch distribution matrix (sm_50..sm_90)
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
 * Configured specifically for rapid iteration and maximum performance on this
 * host machine:
 * - Single CUDA arch (auto-detected, sm_89) for fast nvcc compilation.
 * - CUDA Flash Attention enabled across all quants.
 * - Native host CPU acceleration (i9-13900H: AVX2, FMA, AVX-VNNI int8 acceleration).
 */
export const FAST_BUILD_OPTIONS: PostureOptions = {
  cudaArchitectures: "auto",
  cmakeArgs: [
    "-DTRANSCRIBE_VULKAN=ON", // Vulkan backend (Intel iGPU & NVIDIA)
    "-DGGML_CUDA_FA_QUANTS=all", // Flash Attention across all quants on sm_89
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
 * - Conservative host CPU floor (TRANSCRIBE_X86_CONSERVATIVE=ON) so binaries run
 *   on any x86_64 CPU without crashing on missing ISA instructions (SIGILL).
 * - Flash Attention enabled for all supported GPU architectures.
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
    "-DGGML_CUDA_FA_QUANTS=all", // Flash Attention across all quants
    // Example additional options you can enable for full builds:
    // "-DGGML_CUDA_GRAPHS=ON",
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
