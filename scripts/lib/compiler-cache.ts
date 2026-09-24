// scripts/lib/compiler-cache.ts
//
// Dynamically locates sccache or ccache on the system without relying on
// custom configuration environment variables.
//
// Automatically applies:
// 1. RUSTC_WRAPPER for Rust crates in src-tauri.
// 2. CMAKE_C_COMPILER_LAUNCHER and CMAKE_CXX_COMPILER_LAUNCHER in TRANSCRIBE_CMAKE_ARGS
//    for transcribe.cpp C/C++ files.
//
// Note: Does NOT pass sccache to CMAKE_CUDA_COMPILER_LAUNCHER on Windows because
// sccache fails to parse NVCC's MSVC flags (-Xcompiler=-Fd...,-FS) with "Could not parse shell line".
// CUDA nvcc runs directly with Ninja parallel jobs, while C, C++, and Rust enjoy full caching.

import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { withCmakeArg } from "./cpu-lane";

/**
 * Dynamically locate sccache or ccache on the system without relying on
 * custom configuration environment variables.
 */
export function findCompilerCacheDynamically(): string | null {
  // 1. Probe PATH dynamically using Bun.which
  for (const bin of ["sccache", "ccache"]) {
    const found = Bun.which(bin);
    if (found) {
      return found;
    }
  }

  // 2. Check standard user tool locations if not in active PATH
  const candidates = [
    join(homedir(), ".cargo", "bin", "sccache.exe"),
    join(homedir(), ".cargo", "bin", "ccache.exe"),
    join(homedir(), "AppData", "Local", "bin", "sccache.exe"),
    join(homedir(), "AppData", "Local", "bin", "ccache.exe"),
  ];

  for (const candidate of candidates) {
    if (existsSync(candidate)) {
      return candidate;
    }
  }

  return null;
}

/**
 * Configure compiler caching dynamically:
 * - Enables RUSTC_WRAPPER for Rust crate builds.
 * - Passes CMAKE_C_COMPILER_LAUNCHER and CMAKE_CXX_COMPILER_LAUNCHER to TRANSCRIBE_CMAKE_ARGS.
 * - Leaves CMAKE_CUDA_COMPILER_LAUNCHER unset on Windows for sccache to avoid nvcc shell-parsing crashes.
 */
export function applyCompilerCache(
  env: NodeJS.ProcessEnv = process.env,
): string | null {
  // Make sure TRANSCRIBE_CCACHE_PATH is unset so upstream build.rs doesn't
  // blindly bind sccache to CMAKE_CUDA_COMPILER_LAUNCHER on Windows.
  delete env.TRANSCRIBE_CCACHE_PATH;

  const cachePath = findCompilerCacheDynamically();
  if (cachePath) {
    // Enable for Rust crates
    env.RUSTC_WRAPPER ??= cachePath;

    // Normalize forward slashes for CMake arguments
    const cmakeLauncher = cachePath.replace(/\\/g, "/");

    // Pass C and CXX compiler launchers to CMake
    env.TRANSCRIBE_CMAKE_ARGS = withCmakeArg(
      env.TRANSCRIBE_CMAKE_ARGS,
      `-DCMAKE_C_COMPILER_LAUNCHER="${cmakeLauncher}"`,
    );
    env.TRANSCRIBE_CMAKE_ARGS = withCmakeArg(
      env.TRANSCRIBE_CMAKE_ARGS,
      `-DCMAKE_CXX_COMPILER_LAUNCHER="${cmakeLauncher}"`,
    );

    // If it's real ccache (not sccache), ccache can also handle CUDA on Windows
    const isSccache = cachePath.toLowerCase().includes("sccache");
    if (!isSccache) {
      env.TRANSCRIBE_CMAKE_ARGS = withCmakeArg(
        env.TRANSCRIBE_CMAKE_ARGS,
        `-DCMAKE_CUDA_COMPILER_LAUNCHER="${cmakeLauncher}"`,
      );
    } else {
      // Explicitly clear CMAKE_CUDA_COMPILER_LAUNCHER so CMake overrides any
      // cached launcher in existing build directories and lets nvcc compile directly.
      env.TRANSCRIBE_CMAKE_ARGS = withCmakeArg(
        env.TRANSCRIBE_CMAKE_ARGS,
        '-DCMAKE_CUDA_COMPILER_LAUNCHER=""',
      );
    }
  }
  return cachePath;
}
