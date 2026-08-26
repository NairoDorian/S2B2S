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
//   - Automatically before every `bun run tauri` / `build:fast` / `build:full`
//     invocation: scripts/tauri-runner.ts imports `checkTranscribeDeps` from
//     this file (one implementation, not two copies that can drift).
//   - Manually: bun scripts/check-transcribe-deps.ts
//
// Safe by design: never throws and always leaves the process exit code at 0,
// so it never blocks a tauri build, even when offline. Network failures just
// print a warning and proceed with the cached lock.

import { readFileSync } from "fs";
import { resolve, join } from "path";

const root = resolve(import.meta.dirname, "..");
const cargoLockPath = join(root, "src-tauri", "Cargo.lock");

const REPO_URL = "https://github.com/NairoDorian/transcribe.cpp";
const BRANCH = "main";
const TAG = "[check-transcribe-deps]";

export type CheckOutcome =
  | "skipped"
  | "up-to-date"
  | "updated"
  | "update-failed";

/**
 * Compare the locked transcribe.cpp commit with the remote branch tip and run
 * `cargo update` when the remote is ahead. Never throws.
 */
export function checkTranscribeDeps(): CheckOutcome {
  // 1. Read the commit pinned in Cargo.lock for transcribe-cpp.
  let lockContent: string;
  try {
    lockContent = readFileSync(cargoLockPath, "utf-8");
  } catch {
    console.warn(`${TAG} Could not read Cargo.lock — skipping check.`);
    return "skipped";
  }

  // Cargo.lock source line format:
  //   source = "git+https://github.com/NairoDorian/transcribe.cpp?branch=main#<40-hex-sha>"
  const escapedUrl = REPO_URL.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const sourceRe = new RegExp(
    `git\\+${escapedUrl}\\?branch=${BRANCH}#([0-9a-f]{40})`,
  );
  const lockMatch = lockContent.match(sourceRe);
  if (!lockMatch) {
    console.log(`${TAG} transcribe-cpp not found in Cargo.lock — skipping.`);
    return "skipped";
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
      `${TAG} git ls-remote failed (${stderr || "offline?"}). ` +
        "Proceeding with cached Cargo.lock.",
    );
    return "skipped";
  }

  const remoteOutput = lsResult.stdout.toString().trim();
  // Expected format: <40-hex-sha>\trefs/heads/main
  const remoteCommit = remoteOutput.split("\t")[0]?.trim();
  if (!remoteCommit) {
    console.warn(`${TAG} Could not parse remote ref — skipping.`);
    return "skipped";
  }

  // 3. Compare. If different, `cargo update` to pull the latest.
  if (localCommit === remoteCommit) {
    console.log(
      `${TAG} Up to date (commit ${localCommit.slice(0, 12)}). No update needed.`,
    );
    return "up-to-date";
  }

  console.log(
    `${TAG} Remote ${BRANCH} (${remoteCommit.slice(0, 12)}) ` +
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
      `${TAG} cargo update failed. Proceeding with existing lock — ` +
        "run 'cargo update -p transcribe-cpp -p transcribe-cpp-sys' manually.",
    );
    return "update-failed";
  }

  console.log(
    `${TAG} Cargo.lock updated. The build will compile the new commit.`,
  );
  return "updated";
}

if (import.meta.main) {
  checkTranscribeDeps();
  process.exit(0);
}
