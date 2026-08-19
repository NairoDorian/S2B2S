// scripts/check-transcribe-deps.ts
//
// Checks if the transcribe-cpp / transcribe-cpp-sys git dependencies
// (from https://github.com/NairoDorian/transcribe.cpp, branch=main, applied via
// [patch.crates-io] in src-tauri/Cargo.toml) are pinned to the latest remote
// commit. If the remote branch tip differs from the commit locked in
// Cargo.lock, runs `cargo update` to pull the latest — so the next
// `bun run tauri dev` catches upstream changes to the transcribe.cpp fork
// without a manual bump.
//
// How it works:
//   1. Reads the commit hash pinned in src-tauri/Cargo.lock
//   2. Fetches the remote HEAD for refs/heads/main via `git ls-remote`
//   3. If they differ, runs `cargo update -p transcribe-cpp -p transcribe-cpp-sys`
//   4. If they match, nothing to do
//
// When it runs:
//   - Automatically before every `bun run tauri` invocation (wired into the
//     "tauri" script in package.json)
//   - Can also be run manually: bun scripts/check-transcribe-deps.ts
//
// Safe by design: always exits 0 so it never blocks a tauri build, even when
// offline. Network failures just print a warning and proceed with the cached
// lock.

import { readFileSync } from "fs";
import { resolve, join } from "path";

const root = resolve(import.meta.dirname, "..");
const cargoLockPath = join(root, "src-tauri", "Cargo.lock");

const REPO_URL = "https://github.com/NairoDorian/transcribe.cpp";
const BRANCH = "main";

// 1. Read the commit pinned in Cargo.lock for transcribe-cpp.
let lockContent: string;
try {
  lockContent = readFileSync(cargoLockPath, "utf-8");
} catch {
  console.warn("[check-transcribe-deps] Could not read Cargo.lock — skipping check.");
  process.exit(0);
}

// Cargo.lock source line format:
//   source = "git+https://github.com/NairoDorian/transcribe.cpp?branch=main#<40-hex-sha>"
const escapedUrl = REPO_URL.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const sourceRe = new RegExp(
  `git\\+${escapedUrl}\\?branch=${BRANCH}#([0-9a-f]{40})`,
);
const lockMatch = lockContent.match(sourceRe);
if (!lockMatch) {
  console.log(
    "[check-transcribe-deps] transcribe-cpp not found in Cargo.lock — skipping.",
  );
  process.exit(0);
}
const localCommit = lockMatch[1];

// 2. Fetch the remote HEAD for the branch via `git ls-remote`.
const lsResult = Bun.spawnSync(
  ["git", "ls-remote", REPO_URL, `refs/heads/${BRANCH}`],
  { stdio: ["pipe", "pipe", "pipe"] },
);

if (lsResult.exitCode !== 0) {
  const stderr = lsResult.stderr.toString().trim();
  console.warn(
    `[check-transcribe-deps] git ls-remote failed (${stderr || "offline?"}). ` +
      "Proceeding with cached Cargo.lock.",
  );
  process.exit(0);
}

const remoteOutput = lsResult.stdout.toString().trim();
// Expected format: <40-hex-sha>\trefs/heads/main
const remoteCommit = remoteOutput.split("\t")[0]?.trim();
if (!remoteCommit) {
  console.warn("[check-transcribe-deps] Could not parse remote ref — skipping.");
  process.exit(0);
}

// 3. Compare. If different, `cargo update` to pull the latest.
if (localCommit === remoteCommit) {
  console.log(
    `[check-transcribe-deps] Up to date (commit ${localCommit.slice(0, 12)}). No update needed.`,
  );
  process.exit(0);
}

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

if (updateResult.exitCode !== 0) {
  console.warn(
    "[check-transcribe-deps] cargo update failed. Proceeding with existing lock — " +
      "run 'cargo update -p transcribe-cpp -p transcribe-cpp-sys' manually.",
  );
  process.exit(0);
}

console.log(
  "[check-transcribe-deps] Cargo.lock updated. The build will compile the new commit.",
);
process.exit(0);
