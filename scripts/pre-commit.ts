// scripts/pre-commit.ts
//
// The routine every commit passes through.
//
//   bun run precommit          the fast gate (this is what the git hook runs)
//   bun run precommit:full     the fast gate plus clippy and the test suite
//   bun run update             rtk + dependencies at their newest versions
//
// Why a script rather than a shell one-liner in `.githooks/pre-commit`: the
// same sequence has to run on a Windows checkout, in CI and by hand before a
// release, and a `&&`-chain of eight commands is a different thing in `sh`,
// `pwsh` and `cmd`. One implementation, three callers.
//
// Order matters and is not alphabetical:
//
//   1. **Regenerate, then verify.** `app-meta.ts` is the single source of the
//      product's identity — every other copy of the name, the slug, the
//      version, the identifier or a repository URL is a mirror of it, written
//      by `meta:sync`. Running it first means the commit carries the mirrors
//      the current values imply, and `meta:check` then proves no mirror drifted
//      from its anchor (a changed anchor matches nothing, and `sync` would
//      rather skip than write a wrong value).
//   2. **The cheap correctness gates**, cheapest first, so a broken commit
//      fails in seconds rather than after a full typecheck.
//   3. **Repomix last.** It packs the whole working tree; regenerating it
//      before the checks would pack a tree that is about to change.
//
// What is *not* here, deliberately: `cargo clippy`, `cargo test`, the Vite
// build, `update-deps` and `update:rtk`. A hook that takes ten minutes is a
// hook everyone bypasses with `--no-verify`, and a bypassed gate is worse than
// no gate. Tests and clippy are `precommit:full` (and CI); the two updaters
// reach the network and rewrite lockfiles, so they are `bun run update` and
// belong to the pre-release routine, not to a commit.
//
// Every step reports its own cost. `docs/PERFORMANCE.md` rule 10 asks for the
// cost of a change; the same standard applies to the gate that measures them.

import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const REPO_ROOT = resolve(import.meta.dirname, "..");
const TAG = "[precommit]";
const full = process.argv.includes("--full") || process.argv.includes("--all");

interface Step {
  /** Shown in the run log. */
  label: string;
  /** `bun run <script>`, or an argv when the script needs arguments. */
  argv: string[];
  /** Skipped by the hook; run by `precommit:full`. */
  heavy?: boolean;
  /** Runs after the other steps, when they have all passed. */
  last?: boolean;
  /** Steps that modify the tree need their output staged afterwards. */
  writes?: boolean;
}

const STEPS: Step[] = [
  { label: "identity mirrors", argv: ["run", "meta:sync"], writes: true },
  { label: "identity in sync", argv: ["run", "meta:check"] },
  { label: "no stale product name", argv: ["run", "check:identity"] },
  { label: "translations complete", argv: ["run", "check:translations"] },
  { label: "lint", argv: ["run", "lint"] },
  { label: "typecheck", argv: ["run", "typecheck"] },
  { label: "format", argv: ["run", "format:check"] },
  { label: "rust lints", argv: ["run", "lint:backend"], heavy: true },
  { label: "rust tests", argv: ["run", "test:backend"], heavy: true },
  { label: "repomix pack", argv: ["run", "repomix"], last: true },
];

/** Run one step, streaming its output. Returns false on a non-zero exit. */
function runScript(step: Step): boolean {
  const started = Date.now();
  process.stdout.write(`${TAG} ${step.label} … `);
  const result = spawnSync("bun", step.argv, {
    cwd: REPO_ROOT,
    stdio: "inherit",
    shell: process.platform === "win32",
  });
  const ms = Date.now() - started;
  // The label is repeated on failure: the step's own output is above this line
  // by then, and "which step failed" should not require scrolling.
  if (result.status !== 0) {
    console.error(`${TAG} ${step.label} FAILED after ${ms} ms`);
    return false;
  }
  console.log(`${TAG} ${step.label} ok (${ms} ms)`);
  return true;
}

/** Stage whatever the generator just rewrote, so the commit carries it. */
function stageGenerated(): void {
  // `git add -u` stages modifications to *tracked* files only. A generator that
  // creates a file it has never written before (`app_identity.rs` on the first
  // run after a rename) would be missed by `-u` and must not be added blindly
  // either — an untracked file at this point is a decision for the committer,
  // so it is reported instead of staged.
  const staged = spawnSync("git", ["add", "-u"], { cwd: REPO_ROOT });
  if (staged.status !== 0) {
    console.error(
      `${TAG} could not stage the regenerated files; stage them by hand`,
    );
    return;
  }
  const untracked = spawnSync(
    "git",
    ["ls-files", "--others", "--exclude-standard"],
    { cwd: REPO_ROOT, encoding: "utf8" },
  );
  if (untracked.status === 0 && untracked.stdout.trim()) {
    console.log(
      `${TAG} note: untracked files present, not staged automatically:\n` +
        untracked.stdout
          .trim()
          .split("\n")
          .map((f) => `         ${f}`)
          .join("\n"),
    );
  }
}

/**
 * Warn when this clone has no hook installed.
 *
 * `hooks:install` is a one-time per-clone step, and a clone that skipped it
 * looks exactly like a clone that ran it — until a broken commit lands. The
 * gate can tell the difference from `core.hooksPath` and should say so, because
 * "the hook is not installed" is the one failure this routine cannot report by
 * failing.
 */
function warnIfHookMissing(): void {
  const configured = spawnSync("git", ["config", "--get", "core.hooksPath"], {
    cwd: REPO_ROOT,
    encoding: "utf8",
  });
  if (configured.status === 0 && configured.stdout.trim() === ".githooks")
    return;
  console.log(
    `${TAG} note: the git hook is not installed in this clone, so commits ` +
      `skip this gate. Run \`bun run hooks:install\` once to enable it.`,
  );
}

const selected = STEPS.filter((s) => (s.heavy ? full : true));
warnIfHookMissing();
if (!full) {
  console.log(
    `${TAG} fast gate (${selected.length} steps); \`bun run precommit:full\` adds clippy and the tests`,
  );
}

const startedAll = Date.now();
for (const step of selected.filter((s) => !s.last)) {
  if (!runScript(step)) process.exit(1);
}
if (selected.some((s) => s.writes)) stageGenerated();
for (const step of selected.filter((s) => s.last)) {
  if (!runScript(step)) process.exit(1);
}

const seconds = ((Date.now() - startedAll) / 1000).toFixed(1);
console.log(`${TAG} all ${selected.length} steps passed in ${seconds}s`);
