// The standalone assert checks under `src/`, run as one gate step.
//
// This repo has no JS unit-test runner. The convention is a `.test.ts` beside
// the module it pins, holding plain `assert` calls and run by hand with `bun`
// (`src/components/whats-new/markdown.test.ts` is the precedent). That is a fine
// convention with one hole: nothing ran them. A check that only runs when
// somebody remembers to run it is documentation, not a net — and the moment it
// matters most (a refactor of the file it pins) is exactly the moment nobody
// thinks to look for it.
//
// So this walks `src/` for `*.test.ts` and runs each in its own process. Each
// file is a script rather than a suite: it exits non-zero on its first failed
// assertion, and its own output says which one. Files run in sorted order so the
// log is stable, and a file that fails does not stop the ones after it — one run
// should report everything that is broken, not the first thing.
//
// Cost: one `bun` process per file (~100 ms each, dominated by startup). It is
// paid once per commit, in the gate, next to `typecheck`.
import { spawnSync } from "node:child_process";
import { readdirSync } from "node:fs";
import { join, relative, resolve } from "node:path";

const REPO_ROOT = resolve(import.meta.dirname, "..");
const SRC = join(REPO_ROOT, "src");
const TAG = "[unit]";

/** Every `*.test.ts` under `src/`, as repo-relative paths with `/` separators. */
function testFiles(): string[] {
  return readdirSync(SRC, { recursive: true, encoding: "utf8" })
    .filter((entry) => entry.endsWith(".test.ts"))
    .map((entry) => relative(REPO_ROOT, join(SRC, entry)).replace(/\\/g, "/"))
    .sort();
}

const files = testFiles();
if (files.length === 0) {
  // Not an error: a repo with no assert checks is a legitimate state, and
  // failing here would make deleting the last one look like a broken gate.
  console.log(`${TAG} no *.test.ts under src/ — nothing to run`);
  process.exit(0);
}

const failed: string[] = [];
for (const file of files) {
  const started = Date.now();
  process.stdout.write(`${TAG} ${file} … `);
  // The file is run, not imported: each check may install stubs, mutate a
  // module's exports or set a process-wide flag, and a fresh process is the
  // only isolation that costs nothing to reason about.
  const result = spawnSync("bun", [file], {
    cwd: REPO_ROOT,
    stdio: "inherit",
    shell: process.platform === "win32",
  });
  const ms = Date.now() - started;
  if (result.status !== 0) {
    failed.push(file);
    console.error(`${TAG} ${file} FAILED after ${ms} ms`);
  } else {
    console.log(`${TAG} ${file} ok (${ms} ms)`);
  }
}

if (failed.length > 0) {
  console.error(
    `${TAG} ${failed.length} of ${files.length} check(s) failed:\n` +
      failed.map((f) => `         ${f}`).join("\n"),
  );
  process.exit(1);
}
console.log(`${TAG} ${files.length} check(s) passed`);
