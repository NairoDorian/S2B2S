// scripts/lib/cpu-lane.ts
//
// The CPU-only build posture, shared by every script that compiles the backend:
// `tauri-runner.ts --cpu` (the `dev:cpu` / `build:cpu` lanes), the dependency
// updater's `cargo check`, and the gate's `cargo clippy` / `cargo test`.
//
// Why one module: a check or a lint only has to prove the Rust compiles and
// behaves, and none of that depends on the ~189 CUDA translation units
// transcribe.cpp would otherwise rebuild — minutes of nvcc per configure. Worse,
// a script that ran with the CUDA posture after a `dev:cpu` session flipped the
// native build's configure every time it ran, so the next `dev:cpu` paid the
// rebuild back. Every script that compiles for verification therefore uses the
// same posture as the usual working loop, and they share this function so the
// environment cannot drift between them.
//
// The `cuda` cargo feature stays on (it lives in the target table and tauri dev
// has no --no-default-features), so the switch is the configure argument: the
// native build script applies TRANSCRIBE_CMAKE_ARGS *after* the feature-derived
// defines, so `-DTRANSCRIBE_CUDA=OFF` wins, and the link line is rebuilt from
// the regenerated manifest, so nothing CUDA is linked.

import { homedir } from "node:os";
import { join } from "node:path";

/**
 * Every architecture family transcribe.cpp can compile. Set on every lane: the
 * family-set selection is off for now, so one build carries every model.
 */
export const FULL_MODEL_SET = "full";

/** Append a `-D` to a CMake args string without clobbering what is there. */
export function withCmakeArg(
  existing: string | undefined,
  arg: string,
): string {
  const trimmed = existing?.trim();
  if (!trimmed) return arg;
  // Idempotent: applying the posture twice must not stack the define.
  return trimmed.split(/\s+/).includes(arg) ? trimmed : `${trimmed} ${arg}`;
}

/**
 * The CPU lane's private cache root. Mirrors `get_cache_root()` in the
 * transcribe.cpp build script — where the sibling GPU caches live — with a
 * `cpu-only` segment so the two never share a directory: the native cache key
 * hashes the feature set and the CUDA architecture but NOT
 * TRANSCRIBE_CMAKE_ARGS, so a shared root could hand a CPU configure a library
 * a CUDA configure wrote.
 */
export function cpuCacheRoot(env: NodeJS.ProcessEnv = process.env): string {
  const base =
    process.platform === "win32"
      ? (env.LOCALAPPDATA ?? env.APPDATA ?? homedir())
      : (env.XDG_CACHE_HOME ?? join(homedir(), ".cache"));
  return join(base, "handy", "transcribe_cpp_cache", "cpu-only");
}

/**
 * Apply the CPU-only posture to `env` in place and return it.
 *
 * `TRANSCRIBE_CACHE_DIR` is only set when the caller has pointed the native
 * build nowhere itself (no prebuilt directory, no cache directory), so an
 * explicit local install always wins.
 */
export function applyCpuLane(
  env: NodeJS.ProcessEnv = process.env,
): NodeJS.ProcessEnv {
  env.TRANSCRIBE_MODEL_SET = FULL_MODEL_SET;
  env.TRANSCRIBE_CMAKE_ARGS = withCmakeArg(
    env.TRANSCRIBE_CMAKE_ARGS,
    "-DTRANSCRIBE_CUDA=OFF",
  );
  delete env.TRANSCRIBE_CUDA_ARCHITECTURES;
  if (!env.TRANSCRIBE_PREBUILT_DIR && !env.TRANSCRIBE_CACHE_DIR) {
    env.TRANSCRIBE_CACHE_DIR = cpuCacheRoot(env);
  }
  return env;
}

/** A copy of the current environment with the CPU-only posture applied. */
export function cpuLaneEnv(): NodeJS.ProcessEnv {
  return applyCpuLane({ ...process.env });
}
