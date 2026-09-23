// scripts/check-transcribe-deps.ts
//
// Checks if our git dependencies:
//   1. transcribe-cpp / transcribe-cpp-sys (https://github.com/NairoDorian/transcribe.cpp, branch=main)
//   2. tauri-plugin-* (https://github.com/tauri-apps/plugins-workspace, branch=v3)
// are pinned to the latest remote commit. If the remote branch tip differs from
// the commit locked in Cargo.lock, runs `cargo update` to pull the latest —
// so every `bun run tauri dev` catches upstream changes without manual bumping
// or hardcoding specific commit SHAs.
//
// The Tauri crates themselves (`tauri`, `tauri-runtime-wry`, ...) are no longer
// watched here: they come from crates.io at a pinned alpha, and a version bump
// is `scripts/update-deps.ts`'s job, not a branch-tip refresh. The plugins stay
// on a branch because Tauri publishes them behind their own core releases —
// crates.io stops at `3.0.0-alpha.0`, which still calls the pre-alpha.2 plugin
// API, so the `v3` branch is the only line that compiles against our core.
//
// How it works:
//   1. Reads the commit hashes pinned in src-tauri/Cargo.lock
//   2. Fetches the remote HEAD for the tracking branches via `git ls-remote`
//   3. If any differs, runs `cargo update -p <package>` for that dependency
//   4. If they match, reports up to date
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

const TAG = "[check-transcribe-deps]";

export type CheckOutcome =
  | "skipped"
  | "up-to-date"
  | "updated"
  | "update-failed";

interface GitDep {
  name: string;
  repoUrl: string;
  branch: string;
  packages: string[];
}

const TRACKED_GIT_DEPS: GitDep[] = [
  {
    name: "transcribe-cpp",
    repoUrl: "https://github.com/NairoDorian/transcribe.cpp",
    branch: "main",
    packages: ["transcribe-cpp", "transcribe-cpp-sys"],
  },
  {
    name: "tauri plugins-workspace",
    repoUrl: "https://github.com/tauri-apps/plugins-workspace",
    branch: "v3",
    packages: [
      "tauri-plugin-autostart",
      "tauri-plugin-clipboard-manager",
      "tauri-plugin-dialog",
      "tauri-plugin-fs",
      "tauri-plugin-global-shortcut",
      "tauri-plugin-log",
      "tauri-plugin-opener",
      "tauri-plugin-os",
      "tauri-plugin-process",
      "tauri-plugin-single-instance",
      "tauri-plugin-store",
      "tauri-plugin-updater",
    ],
  },
];

function checkSingleDep(dep: GitDep): CheckOutcome {
  let lockContent: string;
  try {
    lockContent = readFileSync(cargoLockPath, "utf-8");
  } catch {
    console.warn(
      `${TAG} Could not read Cargo.lock — skipping check for ${dep.name}.`,
    );
    return "skipped";
  }

  const escapedUrl = dep.repoUrl.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const sourceRe = new RegExp(
    `git\\+${escapedUrl}\\?branch=${dep.branch}#([0-9a-f]{40})`,
  );
  const lockMatch = lockContent.match(sourceRe);
  if (!lockMatch) {
    console.log(`${TAG} ${dep.name} not found in Cargo.lock — skipping.`);
    return "skipped";
  }
  const localCommit = lockMatch[1];

  const lsResult = Bun.spawnSync(
    ["git", "ls-remote", dep.repoUrl, `refs/heads/${dep.branch}`],
    { stdio: ["pipe", "pipe", "pipe"] },
  );

  if (lsResult.exitCode !== 0) {
    const stderr = lsResult.stderr.toString().trim();
    console.warn(
      `${TAG} git ls-remote failed for ${dep.name} (${stderr || "offline?"}). ` +
        "Proceeding with cached Cargo.lock.",
    );
    return "skipped";
  }

  const remoteOutput = lsResult.stdout.toString().trim();
  const remoteCommit = remoteOutput.split("\t")[0]?.trim();
  if (!remoteCommit) {
    console.warn(
      `${TAG} Could not parse remote ref for ${dep.name} — skipping.`,
    );
    return "skipped";
  }

  if (localCommit === remoteCommit) {
    console.log(
      `${TAG} ${dep.name}: Up to date (commit ${localCommit.slice(0, 12)}). No update needed.`,
    );
    return "up-to-date";
  }

  console.log(
    `${TAG} ${dep.name}: Remote ${dep.branch} (${remoteCommit.slice(0, 12)}) ` +
      `is ahead of local lock (${localCommit.slice(0, 12)}). Updating Cargo.lock...`,
  );

  const updateArgs = [
    "cargo",
    "update",
    ...dep.packages.flatMap((p) => ["-p", p]),
  ];
  const updateResult = Bun.spawnSync(updateArgs, {
    cwd: join(root, "src-tauri"),
    stdio: ["inherit", "inherit", "inherit"],
  });

  if (updateResult.exitCode !== 0) {
    console.warn(
      `${TAG} cargo update failed for ${dep.name}. Proceeding with existing lock — ` +
        `run '${updateArgs.join(" ")}' manually.`,
    );
    return "update-failed";
  }

  console.log(
    `${TAG} ${dep.name}: Cargo.lock updated. The build will compile the new commit.`,
  );
  return "updated";
}

/**
 * Compare locked git dependencies with their remote branch tips and run
 * `cargo update` when any remote branch is ahead. Never throws.
 */
export function checkTranscribeDeps(): CheckOutcome {
  let anyUpdated = false;
  let anyFailed = false;

  for (const dep of TRACKED_GIT_DEPS) {
    const outcome = checkSingleDep(dep);
    if (outcome === "updated") anyUpdated = true;
    if (outcome === "update-failed") anyFailed = true;
  }

  if (anyFailed) return "update-failed";
  if (anyUpdated) return "updated";
  return "up-to-date";
}

if (import.meta.main) {
  checkTranscribeDeps();
  process.exit(0);
}
