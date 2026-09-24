// scripts/cargo-cpu.ts
//
// Runs a cargo command in `src-tauri/` on the CPU-only build posture:
//
//   bun scripts/cargo-cpu.ts clippy --all-targets     (`bun run lint:backend`)
//   bun scripts/cargo-cpu.ts test --all-targets       (`bun run test:backend`)
//   bun scripts/cargo-cpu.ts check                    (`bun run check:backend`)
//
// Verification builds prove that the Rust compiles and behaves, and neither
// depends on transcribe.cpp's CUDA kernels. Running them on the CUDA posture
// cost minutes of nvcc per configure, and flipped the native configure away
// from the `dev:cpu` loop's, which then paid the rebuild back. See
// `lib/cpu-lane.ts` for the environment, shared with `tauri-runner.ts --cpu`
// and the dependency updater's `cargo check`.
//
// CI calls cargo directly with its own environment and is unaffected. To lint
// or test the CUDA posture on purpose, run cargo by hand from `src-tauri/`.

import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

import { cpuLaneEnv } from "./lib/cpu-lane";
import { applyCompilerCache } from "./lib/compiler-cache";

const args = process.argv.slice(2);
if (args.length === 0) {
  console.error("usage: bun scripts/cargo-cpu.ts <cargo subcommand> [args…]");
  process.exit(2);
}

const env = cpuLaneEnv();
applyCompilerCache(env);

const run = spawnSync("cargo", args, {
  cwd: resolve(import.meta.dirname, "..", "src-tauri"),
  stdio: "inherit",
  env,
});
process.exit(run.status ?? 1);
