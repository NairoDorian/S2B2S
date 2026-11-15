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
//   - Automatically before every `bun run tauri` invocation, which is every
//     `dev:*` and `build:*` lane alike: scripts/tauri-runner.ts imports
//     `checkTranscribeDeps` from this file (one implementation, not two copies
//     that can drift). Skipped when `TRANSCRIBE_DIR` or
//     `TRANSCRIBE_PREBUILT_DIR` points the native build at an explicit local
//     install — there is nothing to pull in that case.
//   - Manually: bun scripts/check-transcribe-deps.ts
//
// Safe by design: never throws and always leaves the process exit code at 0,
// so it never blocks a tauri build, even when offline. Network failures just
// print a warning and proceed with the cached lock.
//
// The update is verified, not trusted. `cargo update -p <package>` keeps every
// other crate locked, so when the new branch tip raises a requirement (the
// fork's 0.4.2 needs `log >= 0.4.35` while the lock held 0.4.34) the `[patch]`
// entry cannot apply. Cargo does not fail then: it records the git commit
// under `[[patch.unused]]` and silently locks the crates.io release of the same
// name instead, so the build stops compiling the fork. The lock is therefore
// read back after the update, and only a lock in which every tracked package
// comes from the tracked git source counts. Otherwise the update is retried
// with `--recursive` (which unlocks the package's own dependencies), and when
// that also fails the original lock is restored and the failure is reported.

import { readFileSync, writeFileSync } from "fs";
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

/** One `[[package]]` entry of Cargo.lock: name, version and `source`, if any. */
interface LockedPackage {
  name: string;
  version: string;
  source: string | null;
}

/**
 * `-p` arguments for `cargo update`. A bare name is ambiguous when the lock
 * holds two copies (the crates.io release beside the git one), so those get
 * `name@version` each.
 */
export function updateSpecs(tracked: LockedPackage[]): string[] {
  return tracked.flatMap((p) => {
    const twins = tracked.filter((t) => t.name === p.name).length;
    if (twins > 1) return ["-p", `${p.name}@${p.version}`];
    // Emit a bare name once, however often it was listed.
    return tracked.findIndex((t) => t.name === p.name) === tracked.indexOf(p)
      ? ["-p", p.name]
      : [];
  });
}

/**
 * The `[[package]]` entries of a Cargo.lock. `[[patch.unused]]` entries are
 * deliberately not included: a git commit recorded there is NOT what the build
 * compiles.
 */
export function lockedPackages(lock: string): LockedPackage[] {
  const packages: LockedPackage[] = [];
  for (const block of lock.split(/\r?\n(?=\[\[)/)) {
    if (!block.startsWith("[[package]]")) continue;
    const name = block.match(/^name = "([^"]+)"/m)?.[1];
    if (!name) continue;
    packages.push({
      name,
      version: block.match(/^version = "([^"]+)"/m)?.[1] ?? "",
      source: block.match(/^source = "([^"]+)"/m)?.[1] ?? null,
    });
  }
  return packages;
}

function gitSourcePrefix(dep: GitDep): string {
  return `git+${dep.repoUrl}?branch=${dep.branch}#`;
}

/**
 * The commit every tracked package of `dep` is locked to, or `null` when one
 * of them is missing or comes from anywhere else (crates.io, another commit).
 */
export function lockedGitCommit(lock: string, dep: GitDep): string | null {
  const prefix = gitSourcePrefix(dep);
  const packages = lockedPackages(lock);
  let commit: string | null = null;
  for (const name of dep.packages) {
    const entries = packages.filter((p) => p.name === name);
    // A crate dropped from the graph (a platform-only plugin, say) is fine;
    // what matters is that nothing tracked comes from a different source.
    if (entries.length === 0) continue;
    for (const entry of entries) {
      if (!entry.source?.startsWith(prefix)) return null;
      const hash = entry.source.slice(prefix.length);
      if (commit !== null && hash !== commit) return null;
      commit = hash;
    }
  }
  return commit;
}

function runCargoUpdate(args: string[]): boolean {
  const result = Bun.spawnSync(args, {
    cwd: join(root, "src-tauri"),
    stdio: ["inherit", "inherit", "inherit"],
  });
  return result.exitCode === 0;
}

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

  const tracked = lockedPackages(lockContent).filter((p) =>
    dep.packages.includes(p.name),
  );
  if (tracked.length === 0) {
    console.log(`${TAG} ${dep.name} not found in Cargo.lock — skipping.`);
    return "skipped";
  }
  const lockedCommit = lockedGitCommit(lockContent, dep);
  if (lockedCommit === null) {
    // Already off the tracked source (a previous silent fallback, or a hand
    // edit). Say so loudly, then let the update below try to repair it.
    console.warn(
      `${TAG} ${dep.name}: Cargo.lock does not take ${dep.packages.join(", ")} ` +
        `from ${dep.repoUrl} (${dep.branch}) — ` +
        tracked
          .map((p) => `${p.name} from ${p.source ?? "a path"}`)
          .join(", ") +
        ". Trying to re-pin it.",
    );
  }
  const localCommit = lockedCommit ?? "";

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
      `is ahead of local lock (${localCommit.slice(0, 12) || "none"}). Updating Cargo.lock...`,
  );

  const specs = updateSpecs(tracked);
  const attempts: string[][] = [
    ["cargo", "update", ...specs],
    ["cargo", "update", ...specs, "--recursive"],
  ];

  for (const args of attempts) {
    if (!runCargoUpdate(args)) {
      console.warn(`${TAG} '${args.join(" ")}' failed.`);
      continue;
    }
    let updated = "";
    try {
      updated = readFileSync(cargoLockPath, "utf-8");
    } catch {
      break;
    }
    const pinned = lockedGitCommit(updated, dep);
    if (pinned === remoteCommit) {
      console.log(
        `${TAG} ${dep.name}: Cargo.lock now pins ${remoteCommit.slice(0, 12)} ` +
          `(${args.slice(2).join(" ")}). The build will compile the new commit.`,
      );
      return "updated";
    }
    console.warn(
      `${TAG} '${args.join(" ")}' did not pin ${dep.name} to ${remoteCommit.slice(0, 12)} ` +
        "(cargo fell back to another source; see [[patch.unused]] in Cargo.lock).",
    );
    writeFileSync(cargoLockPath, lockContent);
  }

  writeFileSync(cargoLockPath, lockContent);
  console.warn(
    `${TAG} ${dep.name}: could not move the pin to ${remoteCommit.slice(0, 12)}; ` +
      "Cargo.lock was restored. Either cargo could not resolve the new commit " +
      "(see its error above: it may need a newer crate this lock pins, e.g. the " +
      "Tauri core for the plugins) or it fell back to another source because a " +
      "dependency the commit requires is held back — then run " +
      "`cargo update -p <pkg> -p <held dep>` in src-tauri/ and check that " +
      "Cargo.lock has no [[patch.unused]] entry.",
  );
  return "update-failed";
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
