// scripts/repomix.ts
//
// Regenerates the whole-repository pack used to hand this project to an AI
// assistant:
//
//   bun run repomix            rebuild silently, print the one-line summary
//   bun run repomix --verbose  stream repomix's own progress
//   bun run repomix --check    fail when the pack on disk is stale
//
// The pack is not committed (see `.gitignore`) — it is derived from the
// working tree and would be a second copy of the whole repository in every
// diff. What makes it worth regenerating on a schedule is the opposite: it is
// *always* stale, and a stale pack is worse than none, because the assistant
// reading it answers about code that no longer exists.
//
// Configuration lives in `repomix.config.json` (exclusions, output shape) so
// the command line here stays a command line. The CLI itself is fetched with
// `bunx` at a pinned version rather than added to `devDependencies`: it is a
// maintenance tool run a few times a week, and a repository-packing CLI with
// its language grammars is not weight the app should carry in its lockfile.

import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, statSync } from "node:fs";
import { resolve } from "node:path";

/** Pinned so the pack has the same shape on every machine and every run. */
const REPOMIX_VERSION = "1.18.0";

const REPO_ROOT = resolve(import.meta.dirname, "..");
const CONFIG = resolve(REPO_ROOT, "repomix.config.json");
/** Must match `output.filePath` in the config. */
const OUTPUT = resolve(REPO_ROOT, "repomix-output.xml");
const TAG = "[repomix]";

const args = process.argv.slice(2);
const verbose = args.includes("--verbose") || args.includes("-v");
const check = args.includes("--check");

if (!existsSync(CONFIG)) {
  console.error(`${TAG} ${CONFIG} is missing; the pack has no exclusions`);
  process.exit(1);
}

/** The newest mtime under the repository, ignoring the pack and build output. */
function newestSourceMtime(): number {
  const tracked = spawnSync("git", ["ls-files", "-z"], {
    cwd: REPO_ROOT,
    encoding: "utf8",
  });
  if (tracked.status !== 0) return 0;
  let newest = 0;
  for (const file of tracked.stdout.split("\0")) {
    if (!file) continue;
    try {
      newest = Math.max(newest, statSync(resolve(REPO_ROOT, file)).mtimeMs);
    } catch {
      // A tracked file that is not on disk (a stale index entry) cannot be
      // newer than the pack, so skipping it is the conservative choice.
    }
  }
  return newest;
}

/**
 * `--check` is a timestamp comparison against git's own tracked-file list, not
 * a content hash: the pack embeds absolute paths and a header with timings, so
 * a byte comparison would differ on every run even when nothing changed.
 */
if (check) {
  if (!existsSync(OUTPUT)) {
    console.error(
      `${TAG} no pack at repomix-output.xml — run \`bun run repomix\``,
    );
    process.exit(1);
  }
  const pack = statSync(OUTPUT).mtimeMs;
  const source = newestSourceMtime();
  if (source > pack) {
    console.error(
      `${TAG} the pack is older than the newest tracked file — run \`bun run repomix\``,
    );
    process.exit(1);
  }
  console.log(`${TAG} up to date`);
  process.exit(0);
}

const run = spawnSync(
  "bunx",
  [`repomix@${REPOMIX_VERSION}`, "--config", CONFIG],
  {
    cwd: REPO_ROOT,
    encoding: "utf8",
    stdio: verbose ? "inherit" : ["ignore", "pipe", "inherit"],
    shell: process.platform === "win32",
  },
);

if (run.status !== 0) {
  console.error(`${TAG} repomix exited ${run.status}`);
  process.exit(run.status ?? 1);
}

// The file count is read back off the pack rather than scraped from repomix's
// progress output: the count is what matters when deciding whether a change to
// the exclusions did what it was meant to, and the file itself is the only
// place that cannot disagree with the pack that was actually written.
try {
  const pack = readFileSync(OUTPUT, "utf8");
  const files = pack.match(/<file path="/g)?.length ?? 0;
  const bytes = statSync(OUTPUT).size;
  console.log(
    `${TAG} wrote repomix-output.xml — ${files} files, ` +
      `${(bytes / 1024 / 1024).toFixed(1)} MB`,
  );
} catch {
  console.log(`${TAG} wrote repomix-output.xml`);
}
