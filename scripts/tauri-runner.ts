// scripts/tauri-runner.ts
//
// Wraps `@tauri-apps/cli` with:
// 1. Automatic `check-transcribe-deps` check before running Tauri.
// 2. Fast local GPU build flag support (`--fast` / `--local-gpu` / `--local` / `-localgpu`):
//    When passed to `tauri build` (via `bun run build:fast`), sets TRANSCRIBE_CUDA_ARCHITECTURES=auto
//    so release builds auto-detect only the local system GPU (for fast local release builds)
//    instead of compiling the full multi-arch distribution set (via `bun run build:full`).

import { resolve, join } from "path";
import { readFileSync } from "fs";

const root = resolve(import.meta.dirname, "..");
const cargoLockPath = join(root, "src-tauri", "Cargo.lock");
const REPO_URL = "https://github.com/NairoDorian/transcribe.cpp";
const BRANCH = "main";

// 1. Dependency check (check-transcribe-deps)
function checkTranscribeDeps() {
  let lockContent: string;
  try {
    lockContent = readFileSync(cargoLockPath, "utf-8");
  } catch {
    return;
  }

  const escapedUrl = REPO_URL.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const sourceRe = new RegExp(
    `git\\+${escapedUrl}\\?branch=${BRANCH}#([0-9a-f]{40})`,
  );
  const lockMatch = lockContent.match(sourceRe);
  if (!lockMatch) return;

  const localCommit = lockMatch[1];

  const lsResult = Bun.spawnSync(
    ["git", "ls-remote", REPO_URL, `refs/heads/${BRANCH}`],
    { stdio: ["pipe", "pipe", "pipe"] },
  );

  if (lsResult.exitCode !== 0) return;

  const remoteOutput = lsResult.stdout.toString().trim();
  const remoteCommit = remoteOutput.split("\t")[0]?.trim();
  if (!remoteCommit || localCommit === remoteCommit) return;

  console.log(
    `[check-transcribe-deps] Remote ${BRANCH} (${remoteCommit.slice(0, 12)}) ` +
      `is ahead of local lock (${localCommit.slice(0, 12)}). Updating Cargo.lock...`,
  );

  const updateResult = Bun.spawnSync(
    ["cargo", "update", "-p", "transcribe-cpp", "-p", "transcribe-cpp-sys"],
    {
      cwd: join(root, "src-tauri"),
      stdio: ["inherit", "inherit", "inherit"],
    },
  );

  if (updateResult.exitCode === 0) {
    console.log(
      "[check-transcribe-deps] Cargo.lock updated. The build will compile the new commit.",
    );
  }
}

// 2. Process arguments
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

// 3. Run dependency check
checkTranscribeDeps();

// 4. Spawn tauri CLI with filtered arguments
const proc = Bun.spawnSync([process.execPath, "x", "tauri", ...filteredArgs], {
  cwd: root,
  stdio: ["inherit", "inherit", "inherit"],
  env: process.env,
});

process.exit(proc.exitCode ?? 0);
