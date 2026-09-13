// scripts/pre-commit.ts
//
// The routine. Two things live here, and the difference between them is the
// point:
//
//   bun run precommit          THE GATE. The fast checks, nothing else. This is
//                              what the git hook runs, and it must stay fast
//                              enough that nobody ever wants `--no-verify`.
//   bun run precommit:full     The gate plus clippy and the Rust test suite.
//   bun run precommit:routine  THE FULL ROUTINE, run by hand: every dependency
//                              to its newest version first, then `:full`.
//
// Why a script rather than a shell one-liner in `.githooks/pre-commit`: the
// same sequence has to run on a Windows checkout, in CI and by hand before a
// release, and a `&&`-chain of nine commands is a different thing in `sh`,
// `pwsh` and `cmd`. One implementation, three callers.
//
// ## The routine, in order
//
// The gate is not the routine. The routine is what a person runs before a
// commit, and the gate is what git then enforces:
//
//   1. `bun run update` — rtk, then every dependency (npm + Cargo) to its
//      newest PUBLISHED version, **prereleases included**: `--prerelease` is
//      always on in this project, never a flag someone has to remember. The
//      point is to test the next release against what is actually newest, and
//      for part of the stack the newest is only ever a prerelease — the Solid 2
//      toolchain publishes on `next` tags and has no stable 2.0 at all yet.
//      **Manual, and deliberately not in the hook**: it reaches two registries,
//      rewrites four lockfiles and runs a full tsc + vite build + cargo check
//      as its own validation. That validation is the reason it stays out — the
//      tempting shortcut is to have the hook run the update *without* it and
//      let the gate's `typecheck` stand in, but that is a smaller proof (no
//      production build, clippy instead of cargo check) applied to the change
//      that most needs the larger one. A dependency bump gets the whole check,
//      and gets its own commit message.
//      `bun run precommit:routine` is this step and the next one, in order.
//   2. `bun run precommit:full` — the gate below, plus clippy and the tests.
//   3. Commit. `bun run precommit` runs again from the hook, on the staged tree.
//
// ## The gate, in order
//
//   1. **Regenerate, then verify.** `app-meta.ts` is the single source of the
//      product's identity — every other copy of the name, the slug, the
//      version, the identifier or a repository URL is a mirror of it, written
//      by `meta:sync`. Running it first means the commit carries the mirrors
//      the current values imply, and `meta:check` then proves no mirror drifted
//      from its anchor (a changed anchor matches nothing, and `sync` would
//      rather skip than write a wrong value).
//   2. **The cheap correctness gates**, cheapest first, so a broken commit
//      fails in seconds rather than after a full typecheck. The standalone
//      assert checks (`*.test.ts` under `src/`, run by `test-unit.ts`) sit
//      here: they are the only automated check that reads the app's own modules
//      instead of the build's output, and each costs one `bun` process start.
//   3. **Repomix last.** It packs the whole working tree; regenerating it
//      before the checks would pack a tree that is about to change.
//
// What is *not* in the gate, deliberately: `cargo clippy`, `cargo test`, the
// Vite build and `update-deps`. A hook that takes ten minutes is a hook
// everyone bypasses with `--no-verify`, and a bypassed gate is worse than no
// gate. Step 1 of the routine above is why the dependency update is manual
// rather than merely absent: it is not skipped, it is scheduled where its cost
// belongs.
//
// Every step reports its own cost. `docs/PERFORMANCE.md` rule 10 asks for the
// cost of a change; the same standard applies to the gate that measures them.

import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const REPO_ROOT = resolve(import.meta.dirname, "..");
const TAG = "[precommit]";
/**
 * `--routine`: the whole routine rather than the gate. Manual by design — the
 * dependency update runs first, at full strength, and then `precommit:full`
 * runs on the tree it left behind. `precommit` itself and the git hook are
 * untouched by this flag; see the header for the sequence it belongs to.
 */
const routine = process.argv.includes("--routine");
const full =
  routine || process.argv.includes("--full") || process.argv.includes("--all");

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

/**
 * Step 1 of the routine, and the reason `--routine` exists: every dependency to
 * its newest published version, **prereleases included** — `--prerelease` is
 * part of `bun run update`, not an argument the caller supplies, so a routine
 * run cannot accidentally do a stable-only update. That matters for the Solid 2
 * toolchain in particular, which exists only on prerelease lines (`solid-js`'s
 * `latest` is still the 1.x major; the 2.x packages publish on `next`).
 *
 * This is the ordinary `update` entry point — rtk, then `update-deps
 * --prerelease`, then a repomix pack — so the routine and a maintainer running
 * the pieces by hand are the same sequence. `update-deps` here keeps **its own
 * steps 5-7** (`tsc -b` → vite build → cargo check): those three are the
 * production proof that a new resolution compiles, and the gate two steps later
 * only repeats a smaller version of it (`typecheck`, and clippy rather than a
 * build). A dependency bump is exactly the change that deserves the full one.
 */
const ROUTINE_DEPS: Step = {
  label: "deps → newest (prerelease), validated",
  argv: ["run", "update"],
  writes: true,
};

const STEPS: Step[] = [
  { label: "identity mirrors", argv: ["run", "meta:sync"], writes: true },
  { label: "identity in sync", argv: ["run", "meta:check"] },
  { label: "no stale product name", argv: ["run", "check:identity"] },
  { label: "translations complete", argv: ["run", "check:translations"] },
  { label: "lint", argv: ["run", "lint"] },
  { label: "typecheck", argv: ["run", "typecheck"] },
  { label: "unit checks", argv: ["run", "test:unit"] },
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
if (routine) {
  console.log(
    `${TAG} routine: dependencies first (rtk → update-deps --prerelease, at its ` +
      `own tsc + vite build + cargo check), then the full gate on the result`,
  );
} else if (!full) {
  console.log(
    `${TAG} fast gate (${selected.length} steps); \`bun run precommit:full\` adds clippy and the tests`,
  );
}

const startedAll = Date.now();
if (routine) {
  if (!runScript(ROUTINE_DEPS)) process.exit(1);
  // The manifests and lockfiles just changed; `stageGenerated` is what makes
  // the routine's own output reviewable in `git diff --cached` before the
  // commit it is preparing.
  stageGenerated();
}
for (const step of selected.filter((s) => !s.last)) {
  if (!runScript(step)) process.exit(1);
}
if (selected.some((s) => s.writes)) stageGenerated();
for (const step of selected.filter((s) => s.last)) {
  if (!runScript(step)) process.exit(1);
}

const seconds = ((Date.now() - startedAll) / 1000).toFixed(1);
const ran = selected.length + (routine ? 1 : 0);
console.log(`${TAG} all ${ran} steps passed in ${seconds}s`);
