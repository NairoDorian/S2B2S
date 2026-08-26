// scripts/tauri-runner.ts
//
// Wraps `@tauri-apps/cli` with:
// 1. The `check-transcribe-deps` check before running Tauri (imported from
//    scripts/check-transcribe-deps.ts so there is a single implementation).
// 2. Fast local GPU build flag support (`--fast` / `--local-gpu` / `--local` / `-localgpu`):
//    When passed to `tauri build` (via `bun run build:fast`), sets TRANSCRIBE_CUDA_ARCHITECTURES=auto
//    so release builds auto-detect only the local system GPU (for fast local release builds)
//    instead of compiling the full multi-arch distribution set (via `bun run build:full`).

import { resolve } from "path";
import { checkTranscribeDeps } from "./check-transcribe-deps";

const root = resolve(import.meta.dirname, "..");

// 1. Process arguments
const rawArgs = process.argv.slice(2);
const filteredArgs: string[] = [];
let localGpuRequested = false;

for (const arg of rawArgs) {
  if (
    arg === "--fast" ||
    arg === "-fast" ||
    arg === "--local-gpu" ||
    arg === "--local" ||
    arg === "-localgpu"
  ) {
    localGpuRequested = true;
  } else {
    filteredArgs.push(arg);
  }
}

if (localGpuRequested) {
  process.env.TRANSCRIBE_CUDA_ARCHITECTURES = "auto";
  console.log(
    "[tauri-runner] Fast build mode enabled (TRANSCRIBE_CUDA_ARCHITECTURES=auto): auto-detecting system GPU for this build.",
  );
}

// 2. Dependency check (never throws, never blocks the build)
checkTranscribeDeps();

// 3. Spawn tauri CLI with filtered arguments
const proc = Bun.spawnSync([process.execPath, "x", "tauri", ...filteredArgs], {
  cwd: root,
  stdio: ["inherit", "inherit", "inherit"],
  env: process.env,
});

process.exit(proc.exitCode ?? 0);
