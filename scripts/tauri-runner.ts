// scripts/tauri-runner.ts
//
// Wraps `@tauri-apps/cli` with:
// 1. The `check-transcribe-deps` check before running Tauri (imported from
//    scripts/check-transcribe-deps.ts so there is a single implementation).
// 2. The transcribe.cpp **build posture**: whether CUDA targets only this
//    machine's GPU, the full distribution matrix, or is left out entirely.
// 3. Pruning stale build artifacts from src-tauri/target before every run
//    (scripts/prune-target.ts): superseded dependency versions, git revisions,
//    incremental caches and installers. Skipped when the `NO_PRUNE` flag is
//    set — the prefix comes from app-meta.ts, so see scripts/lib/env-flag.ts
//    for the spelling rather than typing one here.
//
// # Build postures
//
// Every model architecture is compiled into libtranscribe, and every lane packs
// the **full** family set: which families a build carries is no longer a lane
// axis (the `arch-dl` module split and the per-lane model set are both switched
// off for now). The lanes differ on exactly one axis, the CUDA architecture
// policy:
//
//   --fast   TRANSCRIBE_CUDA_ARCHITECTURES=auto      this machine's GPU only
//   --full   TRANSCRIBE_CUDA_ARCHITECTURES=default   the full distribution matrix
//   --cpu    TRANSCRIBE_CMAKE_ARGS=-DTRANSCRIBE_CUDA=OFF   no CUDA at all
//
// which gives the lanes:
//
//   bun run dev:cpu        no CUDA, every family     the usual working loop
//   bun run dev:fast       local CUDA arch           when the GPU path changed
//   bun run dev:full       full CUDA matrix          release-shaped dev build
//   bun run build:cpu      no CUDA, every family     smoke build
//   bun run build:fast     local CUDA arch           local release
//   bun run build:full     full CUDA matrix          distribution
//
// `--cpu` is the cheap lane and the one to reach for by default: it drops the
// ~189 `.cu` translation units and the `ggml-cuda` backend module from the
// build, and the app then runs on the CPU backend, which is compiled in
// regardless. Reach for `dev:fast` when the change touches the GPU path
// (kernels, device selection, VRAM, a timing), and `build:full` for anything
// release-shaped.
//
// `--full` sets the architecture variable explicitly (`default`) rather than
// leaving it unset, so it means "every architecture" in a dev build too —
// unset falls back to the profile, and a dev profile probes the local GPU.
//
// `--full` / `--fast` / `--cpu` are the primitives; `dev:full` / `build:cpu`
// style tokens are accepted as sugar so `bun run tauri build:full` behaves
// like `bun run build:full`.

import { existsSync, readFileSync } from "fs";
import { resolve, join } from "path";
import { homedir } from "os";
import { checkTranscribeDeps } from "./check-transcribe-deps";
import { applyCpuLane, FULL_MODEL_SET, withCmakeArg } from "./lib/cpu-lane";
import { appEnvFlag } from "./lib/env-flag";
import { printReport, pruneTarget } from "./prune-target";
import { applyCompilerCache } from "./lib/compiler-cache";
import { FAST_BUILD_OPTIONS, FULL_BUILD_OPTIONS } from "./build-options";

const root = resolve(import.meta.dirname, "..");

/**
 * How many architecture families `FULL_MODEL_SET` (src/CMakeLists.txt
 * `_all_families`) compiles — reported in the posture line. The set itself is
 * `lib/cpu-lane.ts`'s, shared with every script that compiles the backend.
 */
const FULL_FAMILY_COUNT = 19;

/** What the native build does about CUDA. */
type CudaPolicy = "local" | "matrix" | "off";

type Posture = {
  /** What the native build does about CUDA. */
  cuda: CudaPolicy;
};

/** Resolve a `dev:full` / `build:cpu` style subcommand token into argv. */
function expandModeToken(arg: string): { args: string[] } | null {
  const [command, mode] = arg.split(":");
  if (!mode || (command !== "dev" && command !== "build")) {
    return null;
  }
  if (mode === "full" || mode === "fast" || mode === "cpu") {
    return { args: [command, `--${mode}`] };
  }
  return null;
}

// 1. Process arguments — posture flags are consumed here, everything else is
//    forwarded to the Tauri CLI untouched.
// `--local-gpu` and friends predate `--fast` and remain the documented spelling
// of the fast local-release build, so each maps onto the same posture.
const POSTURE_FLAGS: Record<string, CudaPolicy> = {
  "--full": "matrix",
  "--fast": "local",
  "-fast": "local",
  "--local-gpu": "local",
  "--local": "local",
  "-localgpu": "local",
  "--cpu": "off",
  "-cpu": "off",
};

const passthroughArgs: string[] = [];
const cudaFlags = new Set<CudaPolicy>();

for (const arg of process.argv.slice(2)) {
  const expanded = expandModeToken(arg);
  for (const token of expanded ? expanded.args : [arg]) {
    const cudaFlag = POSTURE_FLAGS[token];
    if (cudaFlag) {
      cudaFlags.add(cudaFlag);
      continue;
    }
    passthroughArgs.push(token);
  }
}

// The three policies describe mutually exclusive postures; silently picking one
// would hide a typo, so refuse instead. (`build --fast --cpu` is not a build
// anyone means to run.)
if (cudaFlags.size > 1) {
  console.error(
    "[tauri-runner] --fast, --full and --cpu are mutually exclusive: --fast targets " +
      "this machine's CUDA architecture, --full the whole distribution matrix, " +
      "--cpu builds with no CUDA at all.",
  );
  process.exit(2);
}

const cuda: CudaPolicy = cudaFlags.values().next().value ?? "local";

const posture: Posture = { cuda };

// Auto-detect and configure compiler caching (sccache / ccache) for both
// transcribe.cpp (CMake/CUDA/C++) and Rust crates in src-tauri.
const compilerCache = applyCompilerCache(process.env);

// Written before the spawn so cargo (and its build scripts) inherit it. Always
// the full set: the family-set selection is off for now, so a build carries
// every model regardless of the lane. (build.rs also honours a
// `minimal-multilingual` cargo feature, which nothing here enables.)
process.env.TRANSCRIBE_MODEL_SET = FULL_MODEL_SET;

/**
 * Compute an isolated cache directory for a given posture and its CMake arguments.
 * Ensures that changes to options (e.g. in scripts/build-options.ts) or switching
 * between postures invalidates the transcribe.cpp native cache, prompting a rebuild,
 * while sccache reuses identical compilation units under the hood.
 */
function getPostureCacheDir(lane: string, cmakeArgs: string[]): string {
  const base =
    process.platform === "win32"
      ? (process.env.LOCALAPPDATA ?? process.env.APPDATA ?? homedir())
      : (process.env.XDG_CACHE_HOME ?? join(homedir(), ".cache"));
  const hash = Bun.hash(cmakeArgs.join(" ")).toString(16).slice(0, 8);
  return join(base, "handy", "transcribe_cpp_cache", `${lane}-${hash}`);
}

if (posture.cuda === "local") {
  process.env.TRANSCRIBE_CUDA_ARCHITECTURES =
    FAST_BUILD_OPTIONS.cudaArchitectures;
  for (const arg of FAST_BUILD_OPTIONS.cmakeArgs) {
    process.env.TRANSCRIBE_CMAKE_ARGS = withCmakeArg(
      process.env.TRANSCRIBE_CMAKE_ARGS,
      arg,
    );
  }
  if (FAST_BUILD_OPTIONS.modelSet) {
    process.env.TRANSCRIBE_MODEL_SET = FAST_BUILD_OPTIONS.modelSet;
  }
  if (FAST_BUILD_OPTIONS.env) {
    Object.assign(process.env, FAST_BUILD_OPTIONS.env);
  }
  if (
    !process.env.TRANSCRIBE_PREBUILT_DIR &&
    !process.env.TRANSCRIBE_CACHE_DIR
  ) {
    process.env.TRANSCRIBE_CACHE_DIR = getPostureCacheDir(
      "local",
      FAST_BUILD_OPTIONS.cmakeArgs,
    );
  }
} else if (posture.cuda === "matrix") {
  process.env.TRANSCRIBE_CUDA_ARCHITECTURES =
    FULL_BUILD_OPTIONS.cudaArchitectures;
  for (const arg of FULL_BUILD_OPTIONS.cmakeArgs) {
    process.env.TRANSCRIBE_CMAKE_ARGS = withCmakeArg(
      process.env.TRANSCRIBE_CMAKE_ARGS,
      arg,
    );
  }
  if (FULL_BUILD_OPTIONS.modelSet) {
    process.env.TRANSCRIBE_MODEL_SET = FULL_BUILD_OPTIONS.modelSet;
  }
  if (FULL_BUILD_OPTIONS.env) {
    Object.assign(process.env, FULL_BUILD_OPTIONS.env);
  }
  if (
    !process.env.TRANSCRIBE_PREBUILT_DIR &&
    !process.env.TRANSCRIBE_CACHE_DIR
  ) {
    process.env.TRANSCRIBE_CACHE_DIR = getPostureCacheDir(
      "matrix",
      FULL_BUILD_OPTIONS.cmakeArgs,
    );
  }
} else {
  // No CUDA at all: the configure switch, a private native cache root and the
  // full family set — `lib/cpu-lane.ts` explains each, and the dependency
  // updater and the gate's cargo steps apply the very same environment.
  applyCpuLane(process.env);
}

const cudaLine = {
  local: `TRANSCRIBE_CUDA_ARCHITECTURES=${FAST_BUILD_OPTIONS.cudaArchitectures} (${FAST_BUILD_OPTIONS.description}).`,
  matrix: `TRANSCRIBE_CUDA_ARCHITECTURES=${FULL_BUILD_OPTIONS.cudaArchitectures} (${FULL_BUILD_OPTIONS.description}).`,
  off: "TRANSCRIBE_CUDA=OFF (no CUDA in this build; the app runs on CPU).",
}[posture.cuda];

console.log(
  `[tauri-runner] transcribe.cpp posture: TRANSCRIBE_MODEL_SET=${FULL_MODEL_SET} ` +
    `(${FULL_FAMILY_COUNT} architecture families: every model, compiled into ` +
    `libtranscribe) and ${cudaLine}`,
);

if (compilerCache) {
  console.log(
    `[tauri-runner] compiler cache: active (${compilerCache}) for Rust & C/C++`,
  );
}

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
    // `tauri signer generate` always writes an encrypted key, so with no
    // password in the environment the bundler stops on "Decrypting updater
    // signing key, expect a prompt for password" -- and that read blocks a
    // non-interactive run (`CI=`, a script, an agent) forever, after the
    // bundles are already built. These keys carry no password, and minisign
    // reads an empty one exactly as it reads no password at all, so supplying
    // it explicitly just skips the prompt. A key regenerated *with* a password
    // needs TAURI_SIGNING_PRIVATE_KEY_PASSWORD set to it, in which case
    // `??=` keeps that value instead.
    process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ??= "";
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

process.exit(proc.exitCode ?? 1);
