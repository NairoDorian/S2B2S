// scripts/tauri-runner.ts
//
// Wraps `@tauri-apps/cli` with:
// 1. The `check-transcribe-deps` check before running Tauri (imported from
//    scripts/check-transcribe-deps.ts so there is a single implementation).
// 2. Fast local GPU build flag support (`--fast` / `--local-gpu` / `--local` / `-localgpu`):
//    When passed to `tauri build` (via `bun run build:fast`), sets TRANSCRIBE_CUDA_ARCHITECTURES=auto
//    so release builds auto-detect only the local system GPU (for fast local release builds)
//    instead of compiling the full multi-arch distribution set (via `bun run build:full`).
// 3. Pruning stale build artifacts from src-tauri/target before every run
//    (scripts/prune-target.ts): superseded dependency versions, git revisions,
//    incremental caches and installers. Skipped when HANDY_NO_PRUNE=1.

import { resolve } from "path";
import { checkTranscribeDeps } from "./check-transcribe-deps";
import { printReport, pruneTarget } from "./prune-target";

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

// 3. Prune what previous builds left behind. Runs after the dependency check
//    so a pin bump's old revision is already "gone from Cargo.lock" and goes
//    with it. Only stale units are touched, so the build that follows is no
//    slower than it would have been.
if (process.env.HANDY_NO_PRUNE !== "1") {
  try {
    printReport(pruneTarget({ tauriDir: resolve(root, "src-tauri") }), false);
  } catch (error) {
    console.warn(`[prune-target] skipped: ${String(error)}`);
  }
}

// 4. Spawn tauri CLI with filtered arguments
const proc = Bun.spawnSync([process.execPath, "x", "tauri", ...filteredArgs], {
  cwd: root,
  stdio: ["inherit", "inherit", "inherit"],
  env: process.env,
});

process.exit(proc.exitCode ?? 0);
