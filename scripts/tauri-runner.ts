// scripts/tauri-runner.ts
//
// Wraps `@tauri-apps/cli` with:
// 1. The `check-transcribe-deps` check before running Tauri (imported from
//    scripts/check-transcribe-deps.ts so there is a single implementation).
// 2. The transcribe.cpp **build posture**: which model architectures the native
//    library compiles, and whether CUDA targets only this machine's GPU.
// 3. Pruning stale build artifacts from src-tauri/target before every run
//    (scripts/prune-target.ts): superseded dependency versions, git revisions,
//    incremental caches and installers. Skipped when the `NO_PRUNE` flag is
//    set — the prefix comes from app-meta.ts, so see scripts/lib/env-flag.ts
//    for the spelling rather than typing one here.
//
// # Build postures
//
// transcribe.cpp builds each model family as a loadable
// `transcribe-arch-<family>.dll` (the `arch-dl` cargo feature, always on for
// Windows x86_64 and Linux — see src-tauri/Cargo.toml) and `TRANSCRIBE_MODEL_SET`
// picks WHICH families:
//
//   minimal-multilingual   parakeet + granite + qwen3_asr   (3 plugins)
//   full                   all 18 families                  (18 plugins)
//
// The preset is part of the CMake cache key, so the two postures cache
// separately and switching costs one native rebuild without invalidating the
// other. `.cargo/config.toml` defaults a bare `cargo build` to minimal;
// everything below is about overriding that for the full builds.
//
//   bun run tauri dev          minimal, default CUDA targets   fast iteration
//   bun run tauri dev:fast     minimal, CUDA arch = local GPU  fastest iteration
//   bun run tauri dev:full     full set, default CUDA targets  all models
//   bun run build:fast         minimal, CUDA arch = local GPU  local release
//   bun run build:full         full set, default CUDA targets  distribution
//
// `--full` and `--fast` are the primitives; `dev:full` / `build:fast` and the
// bare `:full` / `:fast` spellings are accepted as sugar so `bun run tauri
// build:full` behaves like `bun run build:full`.
//
// A model whose family is not in the minimal set fails to load in that posture
// — that is the trade, and it is why the resolved posture is printed on every
// run rather than left implicit.

import { existsSync, readFileSync } from "fs";
import { resolve, join } from "path";
import { homedir } from "os";
import { checkTranscribeDeps } from "./check-transcribe-deps";
import { appEnvFlag } from "./lib/env-flag";
import { printReport, pruneTarget } from "./prune-target";

const root = resolve(import.meta.dirname, "..");

/** The two model-architecture sets transcribe.cpp can be built with. */
const MINIMAL_MODEL_SET = "minimal-multilingual";
const FULL_MODEL_SET = "full";

type Posture = {
  /** `TRANSCRIBE_MODEL_SET`: which `transcribe-arch-*.dll` get built. */
  modelSet: string;
  /** `TRANSCRIBE_CUDA_ARCHITECTURES=auto`: local GPU only, not the full matrix. */
  localGpu: boolean;
};

/** Resolve a `dev:full` / `build:fast` style subcommand token into argv + posture. */
function expandModeToken(
  arg: string,
): { args: string[]; posture?: Partial<Posture> } | null {
  const [command, mode] = arg.split(":");
  if (!mode || (command !== "dev" && command !== "build")) {
    return null;
  }
  if (mode === "full") {
    return { args: [command, "--full"] };
  }
  if (mode === "fast") {
    return { args: [command, "--fast"] };
  }
  return null;
}

// 1. Process arguments — posture flags are consumed here, everything else is
//    forwarded to the Tauri CLI untouched.
// `--local-gpu` and friends predate `--fast` and remain the documented spelling
// of the fast local-release build, so each maps onto the same posture.
const POSTURE_FLAGS: Record<string, "full" | "fast"> = {
  "--full": "full",
  "--fast": "fast",
  "-fast": "fast",
  "--local-gpu": "fast",
  "--local": "fast",
  "-localgpu": "fast",
};

const passthroughArgs: string[] = [];
const postureFlags = new Set<"full" | "fast">();

for (const arg of process.argv.slice(2)) {
  const expanded = expandModeToken(arg);
  for (const token of expanded ? expanded.args : [arg]) {
    const postureFlag = POSTURE_FLAGS[token];
    if (postureFlag) {
      postureFlags.add(postureFlag);
      continue;
    }
    passthroughArgs.push(token);
  }
}

const fullRequested = postureFlags.has("full");
const fastRequested = postureFlags.has("fast");

// `--full` and `--fast` describe opposite postures; silently picking one would
// hide a typo, so refuse instead. (`build --fast --full` is not a build anyone
// means to run.)
if (fullRequested && fastRequested) {
  console.error(
    "[tauri-runner] --full and --fast are mutually exclusive: --full builds every " +
      `model architecture (${FULL_MODEL_SET}), --fast narrows CUDA to the local GPU ` +
      `and the minimal set (${MINIMAL_MODEL_SET}).`,
  );
  process.exit(2);
}

const posture: Posture = {
  modelSet: fullRequested ? FULL_MODEL_SET : MINIMAL_MODEL_SET,
  localGpu: fastRequested,
};

// Written before the spawn so cargo (and its build scripts) inherit them. An
// explicit assignment also beats `.cargo/config.toml`'s `force = false` default,
// which is exactly how `dev:full` overrides the minimal preset.
process.env.TRANSCRIBE_MODEL_SET = posture.modelSet;
if (posture.localGpu) {
  process.env.TRANSCRIBE_CUDA_ARCHITECTURES = "auto";
}

// Bundle builds sign the updater artifacts (`createUpdaterArtifacts` is on),
// and the Tauri bundler refuses to produce them without the private key. CI
// gets it from the TAURI_SIGNING_PRIVATE_KEY secret; locally, fall back to the
// key the maintainer generated at `~/.tauri/zer0.key` so `build:fast` /
// `build:full` sign without extra setup. Never logged, never copied anywhere.
if (
  process.env.TAURI_SIGNING_PRIVATE_KEY === undefined &&
  process.env.TAURI_SIGNING_PRIVATE_KEY_PATH === undefined
) {
  const localKey = join(homedir(), ".tauri", "zer0.key");
  if (existsSync(localKey)) {
    process.env.TAURI_SIGNING_PRIVATE_KEY = readFileSync(localKey, "utf-8");
  }
}

const archCount = posture.modelSet === FULL_MODEL_SET ? 18 : 3;
console.log(
  `[tauri-runner] transcribe.cpp posture: TRANSCRIBE_MODEL_SET=${posture.modelSet} ` +
    `(${archCount} architecture module${archCount === 1 ? "" : "s"}` +
    `${posture.modelSet === FULL_MODEL_SET ? ": every model" : ": parakeet, granite, qwen3_asr"}, ` +
    `built as transcribe-arch-*.dll beside libtranscribe)` +
    (posture.localGpu
      ? " and TRANSCRIBE_CUDA_ARCHITECTURES=auto (this machine's GPU only)."
      : " and the full CUDA architecture matrix."),
);

// 2. Dependency check (never throws, never blocks the build)
if (process.env.TRANSCRIBE_DIR || process.env.TRANSCRIBE_PREBUILT_DIR) {
  console.log(
    "[transcribe] using the explicit local install; skipping remote pin updates",
  );
} else {
  checkTranscribeDeps();
}

// 3. Prune what previous builds left behind. Runs after the dependency check
//    so a pin bump's old revision is already "gone from Cargo.lock" and goes
//    with it. Only stale units are touched, so the build that follows is no
//    slower than it would have been.
if (!appEnvFlag("NO_PRUNE")) {
  try {
    printReport(pruneTarget({ tauriDir: resolve(root, "src-tauri") }), false);
  } catch (error) {
    console.warn(`[prune-target] skipped: ${String(error)}`);
  }
}

// 4. Spawn tauri CLI with the posture flags removed
const proc = Bun.spawnSync(
  [process.execPath, "x", "tauri", ...passthroughArgs],
  {
    cwd: root,
    stdio: ["inherit", "inherit", "inherit"],
    env: process.env,
  },
);

process.exit(proc.exitCode ?? 0);
