// scripts/update-all.ts
//
// Brings the toolchain and every dependency to its newest version, then proves
// the result still builds:
//
//   bun run update              rtk + npm + Cargo, newest that resolves
//   bun run update --dry-run    report what would move, write nothing
//
// Run this before each release (0.9.7 → 0.9.8 → 0.9.9 → 1.0.0) and whenever
// something has been pinned for a while. It is not part of the pre-commit
// hook: it reaches the network, it rewrites three lockfiles, and a dependency
// bump is a change that deserves its own commit with its own message.
//
// **`--prerelease` is always passed to the dependency updater.** Newest-first
// is the point of running this at all: a pre-release that is newer than the
// stable tag is exactly the version this project wants to be testing, because
// a problem found here is a problem found before the stable release rather
// than after. The updater still refuses anything its validation steps reject
// (`tsc -b` → `vite build` → `cargo check`), so "newest" cannot mean "broken".
//
// Order: rtk first, then the dependencies, then the pack. rtk is the tool this
// project's own commands are routed through, so a stale one makes every later
// step's output the wrong shape; the pack is regenerated last so it describes
// the tree the update actually produced.

import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const REPO_ROOT = resolve(import.meta.dirname, "..");
const TAG = "[update]";
const dryRun = process.argv.includes("--dry-run");

interface Step {
  label: string;
  argv: string[];
  /** Skipped entirely in a dry run (it installs a binary). */
  skipsOnDryRun?: boolean;
}

const STEPS: Step[] = [
  {
    label: "rtk → latest release",
    argv: ["run", "update:rtk"],
    skipsOnDryRun: true,
  },
  {
    label: "npm + Cargo → newest (prerelease allowed)",
    argv: [
      "run",
      "update-deps",
      "--",
      "--prerelease",
      ...(dryRun ? ["--dry-run"] : []),
    ],
  },
  { label: "repomix pack", argv: ["run", "repomix"] },
];

console.log(
  dryRun
    ? `${TAG} dry run — nothing is written`
    : `${TAG} updating rtk and every dependency to the newest versions`,
);

for (const step of STEPS) {
  if (dryRun && step.skipsOnDryRun) {
    console.log(`${TAG} ${step.label} — skipped in a dry run`);
    continue;
  }
  const started = Date.now();
  process.stdout.write(`${TAG} ${step.label} …\n`);
  const result = spawnSync("bun", step.argv, {
    cwd: REPO_ROOT,
    stdio: "inherit",
    shell: process.platform === "win32",
  });
  if (result.status !== 0) {
    console.error(
      `${TAG} ${step.label} FAILED after ${Date.now() - started} ms`,
    );
    process.exit(1);
  }
  console.log(`${TAG} ${step.label} ok (${Date.now() - started} ms)`);
}

console.log(
  `${TAG} done. Review the lockfile diff, then run \`bun run precommit:full\`.`,
);
