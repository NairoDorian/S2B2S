import fs from "node:fs";
import { spawnSync } from "node:child_process";
import path from "node:path";

import { APP } from "./app-meta";

/**
 * Ultimate All-Inclusive Dependency Updater & Sub-Dependency Tracker for this
 * project.
 * Inspired by Minimalistic_App.
 *
 * Capabilities:
 * 1. Dynamic NPM Registry Querying (@latest versions)
 * 2. Dynamic Crates.io Registry Querying (@latest versions)
 * 3. Direct Dependency Upgrading (package.json & src-tauri/Cargo.toml)
 * 4. Transitive Sub-Dependency & Sub-Sub-Dependency Upgrading (bun update --latest & cargo update)
 * 5. Full Inventory Audit & Diff Tracking (Cargo.lock & node_modules)
 * 6. TypeScript Static Type Checking (bun x tsc -b)
 * 7. Vite Production Frontend Build Validation (bun run vite:build)
 * 8. Native Cargo Backend Compilation Verification (cargo check)
 *
 * CLI flags:
 *   (no args)      Stable @latest pipeline (default — only `latest` dist-tags,
 *                  crates.io `max_version`; nothing is ever force-installed
 *                  beyond what bun/cargo resolve as compatible).
 *   --prerelease   Prefer pre-release versions for DIRECT dependencies:
 *                  every NPM prerelease dist-tag (`next`, `beta`, `rc`, `alpha`,
 *                  `canary`, `experimental`, `insiders`, `dev`) is evaluated and
 *                  the best strictly-newer candidate wins (highest SemVer core,
 *                  then newest publish time); crates.io uses `newest_version`
 *                  (which may include prereleases). Transitive deps still
 *                  resolve via `bun update --latest` / `cargo update` (bun/cargo
 *                  pull in prereleases only when a direct dep requires them).
 *   --dry-run      Query registries and print a "would upgrade" report WITHOUT
 *                  writing anything (no bun add, no Cargo.toml edits, no
 *                  lockfile refreshes, no build steps). Safe to run any time.
 *   --help         Show usage summary.
 *
 * Prerelease clobber guard (step 4b): `bun update --latest` resolves the
 * `latest` dist-tag and rewrites package.json specs, silently downgrading
 * exact prerelease pins (e.g. `react-dom 19.3.0-canary-* -> 19.2.8`) and
 * stripping range operators. After the transitive refresh, prerelease mode
 * re-runs `bun add <pkg>@<spec>` to restore EVERY prerelease pin in the
 * snapshot — whether or not this run targeted it.
 *
 * Compatibility guarantee: every proposed upgrade must survive the validation
 * steps (tsc -b -> vite build -> cargo check) — steps 1-4 hard-fail (exit 1)
 * on any resolution error, so an incompatible "latest" can never be force-applied.
 */

/*
 * Conflict handling (step 4): when `cargo update` fails with "failed to
 * select a version", the error is parsed, the direct crate in the conflict
 * chain is held at its current spec with the reason printed once, and the
 * update is retried — instead of re-running the whole update per crate and
 * dumping cargo's error twice. The final report marks such crates "Held",
 * labels spec-only rewrites (lockfile already had the version) as such, and
 * lists transitive crates that are newer on crates.io but pinned by another
 * crate's requirement (`cargo update --dry-run --verbose`), so "up to date"
 * is only printed when it is true.
 */

// --- CLI flag parsing (before anything else so --help/--dry-run are side-effect free) ---
const cliArgs = new Set(process.argv.slice(2));
const PRERELEASE_MODE = cliArgs.has("--prerelease");
const DRY_RUN = cliArgs.has("--dry-run");
const SHOW_HELP = cliArgs.has("--help") || cliArgs.has("-h");

/**
 * Crates that stay on their STABLE line even under --prerelease.
 * `libc`: its README says "depend on 0.2"; the 1.0.0-alpha.* tags are
 * snapshots of the v1.0 `main` branch published on the same day as the
 * matching 0.2.x. Every other crate in the graph depends on 0.2, so taking
 * the alpha only compiles a second libc on Linux (2026-09-11 research).
 */
const CARGO_STABLE_ONLY: ReadonlySet<string> = new Set(["libc"]);

/** NPM dist-tags probed when --prerelease is active — every tag is evaluated, the best strictly-newer candidate wins. */
const PRERELEASE_TAGS: readonly string[] = [
  "next",
  "beta",
  "rc",
  "alpha",
  "canary",
  "experimental",
  "insiders",
  "dev",
];

if (SHOW_HELP) {
  console.log(`Usage:
  bun run update-deps                          Stable @latest pipeline (default)
  bun run update-deps --prerelease             Prefer beta/alpha/RC versions for direct deps
  bun run update-deps --dry-run                Report what WOULD upgrade (no changes made)
  bun run update-deps --prerelease --dry-run   Preview prerelease upgrades, apply nothing`);
  process.exit(0);
}

interface DependencyStatus {
  name: string;
  ecosystem: "NPM (Bun)" | "Cargo (Rust)";
  type: "runtime" | "dev" | "cargo-dep" | "cargo-build";
  /** Cargo.toml section the crate was read from (Cargo rows only). */
  section?: string;
  currentVersion: string;
  latestVersion: string;
  needsUpdate: boolean;
  /** True when the resolved target is a pre-release (--prerelease mode). */
  prerelease: boolean;
}

interface SubDepDiff {
  name: string;
  ecosystem: "NPM (Bun)" | "Cargo (Rust)";
  before: string;
  after: string;
}

/** Semver versions containing a hyphen (e.g. `0.5.0-beta.1`) are pre-releases. */
function isPrereleaseVersion(version: string): boolean {
  return /-\d|-[a-z]/i.test(version);
}

/**
 * Splits a semver string into numeric core parts and a prerelease identifier.
 * `noUncheckedIndexedAccess` makes `coreStr` `string | undefined`; a version
 * string always has a core part before any `-`, so the fallback to "0" is
 * unreachable but satisfies the type checker.
 */
function parseVersion(v: string): { core: number[]; pre: string | null } {
  const [coreStr, pre] = v.split("-", 2);
  const core = (coreStr ?? "0").split(".").map((n) => parseInt(n, 10) || 0);
  while (core.length < 3) core.push(0);
  return { core, pre: pre ?? null };
}

/**
 * Minimal SemVer comparator (no external dependency): returns 1 when `a` is
 * newer than `b`, -1 when older, 0 when equal.
 *
 * Rules: numeric core parts compare numerically; a release sorts ABOVE any
 * prerelease of the same core (`8.2.0-beta.0 < 8.2.0`); prerelease identifiers
 * compare numerically when both numeric, lexicographically otherwise, with
 * numeric identifiers sorting below alphanumeric ones.
 *
 * Used to reject prerelease dist-tags that point at OLDER lines than the
 * installed version (e.g. `@tauri-apps/api`'s `next` tag -> `2.0.1` while the
 * project already uses `^2.11.1`) — upgrades must never downgrade.
 */
function compareVersions(a: string, b: string): number {
  const A = parseVersion(a);
  const B = parseVersion(b);
  for (let i = 0; i < 3; i++) {
    const aCore = A.core[i] ?? 0;
    const bCore = B.core[i] ?? 0;
    if (aCore !== bCore) return aCore > bCore ? 1 : -1;
  }
  if (A.pre === null && B.pre === null) return 0;
  if (A.pre === null) return 1;
  if (B.pre === null) return -1;
  const aId = A.pre.split(".");
  const bId = B.pre.split(".");
  const len = Math.max(aId.length, bId.length);
  for (let i = 0; i < len; i++) {
    const x = aId[i];
    const y = bId[i];
    if (x === undefined) return -1;
    if (y === undefined) return 1;
    const xn = /^\d+$/.test(x) ? Number(x) : null;
    const yn = /^\d+$/.test(y) ? Number(y) : null;
    if (xn !== null && yn !== null) {
      if (xn !== yn) return xn > yn ? 1 : -1;
    } else if (xn !== null) {
      return -1; // numeric identifiers sort below alphanumeric ones
    } else if (yn !== null) {
      return 1;
    } else if (x !== y) {
      return x > y ? 1 : -1;
    }
  }
  return 0;
}

/**
 * Compares only the numeric X.Y.Z core of two versions (ignores prerelease
 * identifiers): returns 1 when `a`'s core is newer, -1 when older, 0 when equal.
 * Used to rank prerelease candidates by release line before deciding between
 * different builds of the same line — for equal cores, lexical build-hash
 * comparison is meaningless, so publish time decides instead.
 */
function compareCores(a: string, b: string): number {
  const A = (a.split("-")[0] ?? "0")
    .split(".")
    .map((n) => parseInt(n, 10) || 0);
  const B = (b.split("-")[0] ?? "0")
    .split(".")
    .map((n) => parseInt(n, 10) || 0);
  for (let i = 0; i < 3; i++) {
    const aCore = A[i] ?? 0;
    const bCore = B[i] ?? 0;
    if (aCore !== bCore) return aCore > bCore ? 1 : -1;
  }
  return 0;
}

function cleanVersion(v: string): string {
  if (typeof v !== "string") return "";
  return v.replace(/^[\^~=v]/, "").trim();
}

/**
 * Resolves the target NPM version.
 * Stable mode: the `latest` dist-tag (cheap `/latest` endpoint), returned only
 * when it is STRICTLY NEWER than `current` — never a downgrade of a
 * deliberately pinned prerelease.
 * Prerelease mode: full packument; EVERY probed prerelease dist-tag is
 * evaluated and the best candidate that is strictly newer than `current` wins
 * (publisher-agnostic — react ships fresher builds under `canary` than under
 * `next`, typescript under `next`, etc.). Ranking: highest core version first;
 * for equal cores, the chronologically newest publish wins (packument `time`
 * field — lexical build-hash comparison is meaningless for canary builds); a
 * lexical SemVer tiebreak decides otherwise. Stale tags pointing at older
 * lines are rejected by the gate; `latest` is returned only when it is itself
 * an upgrade, and `null` means "nothing newer exists".
 */
async function fetchLatestNpmVersion(
  pkgName: string,
  current: string,
  prerelease = false,
): Promise<string | null> {
  try {
    if (!prerelease) {
      const response = await fetch(
        `https://registry.npmjs.org/${pkgName}/latest`,
        {
          headers: { Accept: "application/json" },
        },
      );
      if (response.ok) {
        const data = (await response.json()) as { version?: string };
        return data.version && compareVersions(data.version, current) > 0
          ? data.version
          : null;
      }
      return null;
    }
    const response = await fetch(`https://registry.npmjs.org/${pkgName}`, {
      headers: { Accept: "application/json" },
    });
    if (!response.ok) return null;
    const data = (await response.json()) as {
      "dist-tags"?: Record<string, string>;
      time?: Record<string, string>;
    };
    const tags = data["dist-tags"] ?? {};
    const times = data.time ?? {};
    const currentTime = times[current] ?? null;

    // Upgrade gate: strictly newer by core version; for equal cores, the
    // publish-time comparison decides (when both timestamps are known and
    // differ) — a canary build published later IS an upgrade even when its
    // build hash sorts lexically lower. Falls back to lexical SemVer.
    const isStrictlyNewer = (
      v: string,
      publishedAt: string | null,
    ): boolean => {
      const coreDiff = compareCores(v, current);
      if (coreDiff !== 0) return coreDiff > 0;
      if (
        publishedAt !== null &&
        currentTime !== null &&
        publishedAt !== currentTime
      ) {
        return (
          new Date(publishedAt).getTime() > new Date(currentTime).getTime()
        );
      }
      return compareVersions(v, current) > 0;
    };

    let best: { version: string; publishedAt: string | null } | null = null;
    for (const tag of PRERELEASE_TAGS) {
      const candidate = tags[tag];
      const publishedAt = candidate ? (times[candidate] ?? null) : null;
      if (!candidate || !isStrictlyNewer(candidate, publishedAt)) continue;
      if (best === null) {
        best = { version: candidate, publishedAt };
        continue;
      }
      const coreDiff = compareCores(candidate, best.version);
      let takeCandidate: boolean;
      if (coreDiff !== 0) {
        takeCandidate = coreDiff > 0;
      } else if (
        publishedAt !== null &&
        best.publishedAt !== null &&
        publishedAt !== best.publishedAt
      ) {
        takeCandidate =
          new Date(publishedAt).getTime() >
          new Date(best.publishedAt).getTime();
      } else {
        takeCandidate = compareVersions(candidate, best.version) > 0;
      }
      if (takeCandidate) best = { version: candidate, publishedAt };
    }

    const latestTag = tags["latest"] ?? null;
    return (
      best?.version ??
      (latestTag && isStrictlyNewer(latestTag, times[latestTag] ?? null)
        ? latestTag
        : null)
    );
  } catch {
    return null;
  }
}

/**
 * Resolves the target crates.io version.
 * Stable mode: `max_version` (highest non-prerelease).
 * Prerelease mode: `newest_version` — when it is at least the installed
 * version (a pinned newest pre-release then reads "already @latest" instead
 * of showing the older stable as its target); otherwise `max_version`.
 */
async function fetchLatestCrateVersion(
  crateName: string,
  current: string,
  prerelease = false,
): Promise<string | null> {
  try {
    const response = await fetch(
      `https://crates.io/api/v1/crates/${crateName}`,
      {
        headers: { "User-Agent": "HandyAppUpdater/1.0" },
      },
    );
    if (response.ok) {
      const data = (await response.json()) as {
        crate?: {
          max_version?: string;
          max_stable_version?: string;
          newest_version?: string;
        };
      };
      const crate = data.crate;
      if (!crate) return null;
      if (
        prerelease &&
        crate.newest_version &&
        compareVersions(crate.newest_version, current) >= 0
      ) {
        return crate.newest_version;
      }
      return crate.max_stable_version || crate.max_version || null;
    }
  } catch {}
  return null;
}

function parseCargoLock(filePath: string): Record<string, string> {
  const map: Record<string, string> = {};
  if (!fs.existsSync(filePath)) return map;
  const content = fs.readFileSync(filePath, "utf8");
  const blocks = content.split("[[package]]");
  for (const block of blocks) {
    const nameMatch = block.match(/^\s*name\s*=\s*"([^"]+)"/m);
    const verMatch = block.match(/^\s*version\s*=\s*"([^"]+)"/m);
    if (
      nameMatch &&
      verMatch &&
      nameMatch[1] !== undefined &&
      verMatch[1] !== undefined
    ) {
      map[nameMatch[1]] = verMatch[1];
    }
  }
  return map;
}

/** Every locked version per crate name, ascending (Cargo.lock is sorted). */
function parseCargoLockAll(filePath: string): Record<string, string[]> {
  const map: Record<string, string[]> = {};
  if (!fs.existsSync(filePath)) return map;
  for (const block of fs.readFileSync(filePath, "utf8").split("[[package]]")) {
    const name = block.match(/^\s*name\s*=\s*"([^"]+)"/m)?.[1];
    const ver = block.match(/^\s*version\s*=\s*"([^"]+)"/m)?.[1];
    if (name && ver) (map[name] ??= []).push(ver);
  }
  return map;
}

/**
 * The locked version a direct spec resolves to: the highest one on the same
 * release line (major, or minor while major is 0). `cargo update sysinfo`
 * is "ambiguous" when two versions are locked; `sysinfo@0.39.6` is not.
 */
function pickLockedVersion(
  versions: string[],
  spec: string,
): string | undefined {
  const { core } = parseVersion(cleanVersion(spec));
  const sameLine = versions.filter((v) => {
    const c = parseVersion(v).core;
    if (c[0] !== core[0]) return false;
    return core[0] !== 0 || c[1] === core[1];
  });
  const pool = sameLine.length > 0 ? sameLine : versions;
  return pool.toSorted(compareVersions).at(-1);
}

/**
 * Installed NPM packages: read from `bun.lock` (every resolved package,
 * whatever the linker), falling back to a node_modules scan that follows
 * the symlinks Bun's isolated linker leaves at the top level.
 */
function parseBunInstalledVersions(): Record<string, string> {
  const map: Record<string, string> = {};
  const lockPath = path.resolve("bun.lock");
  if (fs.existsSync(lockPath)) {
    const lock = fs.readFileSync(lockPath, "utf8");
    // `"name": ["name@version", …]` — the key is the install path (nested
    // duplicates appear as "parent/name"), the first element the resolution.
    for (const m of lock.matchAll(
      /^\s*"([^"]+)":\s*\["(?:@[^"/]+\/)?[^"@]+@([^"]+)"/gm,
    )) {
      if (m[1] !== undefined && m[2] !== undefined) map[m[1]] = m[2];
    }
    if (Object.keys(map).length > 0) return map;
  }
  const nmPath = path.resolve("node_modules");
  if (!fs.existsSync(nmPath)) return map;

  function scan(dir: string) {
    try {
      const entries = fs.readdirSync(dir, { withFileTypes: true });
      entries.forEach((e) => {
        if (e.isDirectory() || e.isSymbolicLink()) {
          if (e.name.startsWith("@")) {
            scan(path.join(dir, e.name));
          } else {
            const pkgJsonPath = path.join(dir, e.name, "package.json");
            if (fs.existsSync(pkgJsonPath)) {
              try {
                const pj = JSON.parse(fs.readFileSync(pkgJsonPath, "utf8"));
                if (pj.name && pj.version) {
                  map[pj.name] = pj.version;
                }
              } catch {}
            }
          }
        }
      });
    } catch {}
  }
  scan(nmPath);
  return map;
}

function runCmd(
  cmd: string,
  args: string[],
  cwd?: string,
): { success: boolean; durationMs: number } {
  const start = Date.now();
  const res = spawnSync(cmd, args, { stdio: "inherit", shell: true, cwd });
  const durationMs = Date.now() - start;
  return { success: res.status === 0, durationMs };
}

/** Output of a captured child process. */
interface CapturedRun {
  success: boolean;
  durationMs: number;
  stdout: string;
  stderr: string;
}

/** Like runCmd, but captures output so callers can filter or diagnose it. */
function runCaptured(cmd: string, args: string[], cwd?: string): CapturedRun {
  const start = Date.now();
  const res = spawnSync(cmd, args, { encoding: "utf8", shell: true, cwd });
  return {
    success: res.status === 0,
    durationMs: Date.now() - start,
    stdout: res.stdout ?? "",
    stderr: res.stderr ?? "",
  };
}

/**
 * Prints the useful part of a `cargo update` run — what was locked, added,
 * updated or removed. The "Updating git repository …" / "Updating crates.io
 * index" chatter (eight lines per invocation here) collapses into one line.
 */
function printCargoUpdateOutput(run: CapturedRun, indent = "   "): void {
  let refreshed = 0;
  for (const raw of `${run.stdout}\n${run.stderr}`.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line) continue;
    if (/^Updating (git repository|crates\.io index)/.test(line)) {
      refreshed++;
      continue;
    }
    if (
      /^(note: pass `--verbose`|note: to see how you depend|warning: not updating lockfile)/.test(
        line,
      )
    ) {
      continue;
    }
    console.log(`${indent}${line}`);
  }
  if (refreshed > 0) {
    console.log(`${indent}(refreshed ${refreshed} package source(s))`);
  }
}

/** First `error:` line of a cargo run, without the prefix. */
function firstCargoError(stderr: string): string {
  const line = stderr
    .split(/\r?\n/)
    .map((l) => l.trim())
    .find((l) => l.startsWith("error"));
  return (line ?? "cargo failed").replace(/^error(\[[^\]]*\])?:\s*/, "");
}

interface CargoResolveConflict {
  /** Direct dependencies of this package that the conflict chain runs through. */
  directCrates: string[];
  /** One sentence a human can act on. */
  reason: string;
}

/**
 * Reads cargo's "failed to select a version" error and names the direct
 * dependency to hold back plus why. Handles the `links` case (two crates
 * would link the same native library, e.g. gtk 0.19 vs rfd's gtk-sys 0.18
 * both linking gtk-3) and plain version-requirement conflicts.
 */
function parseCargoResolveError(
  stderr: string,
  ownPackage: string,
): CargoResolveConflict | null {
  const text = stderr.replace(/\r/g, "");
  const failed = text.match(
    /failed to select a version for (?:the requirement )?`([^`]+)`/,
  );
  if (!failed || failed[1] === undefined) return null;
  const subject = failed[1].replace(/\s*=\s*".*$/, "");
  const own = ownPackage.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const direct = new Set<string>();
  const directRe = new RegExp(
    "which satisfies dependency `([A-Za-z0-9_-]+)[^`]*` of package `" +
      own +
      "\\b",
    "g",
  );
  for (const m of text.matchAll(directRe)) {
    if (m[1] !== undefined) direct.add(m[1]);
  }
  // "required by package `rfd v0.16.0`" followed by the chain up to us.
  const chain: string[] = [];
  const lines = text.split("\n");
  const reqIdx = lines.findIndex((l) => /required by package `/.test(l));
  if (reqIdx >= 0) {
    const first = lines[reqIdx]?.match(/required by package `([^`]+)`/);
    if (first?.[1]) chain.push(first[1]);
    for (const l of lines.slice(reqIdx + 1)) {
      const m = l.match(
        /which satisfies dependency `[^`]+` of package `([^`(]+)/,
      );
      if (!m || m[1] === undefined) break;
      const pkg = m[1].trim();
      if (pkg.startsWith(ownPackage)) break;
      chain.push(pkg);
    }
  }
  const requirement = text.match(
    /versions that meet the requirements `([^`]+)`/,
  );
  const links = text.match(/links to the native library `([^`]+)`/);
  const conflicting = text.match(
    /conflicts with a previous package[^\n]*\n\s*package `([^`]+)`/,
  );
  const needs = `${chain[0] ?? "another crate"}${chain.length > 1 ? ` (via ${chain.slice(1).join(" ← ")})` : ""} needs ${subject}${requirement?.[1] ? ` ${requirement[1]}` : ""}`;
  const reason = links
    ? `${needs}, ${conflicting?.[1] ?? "the new version"} would be linked as well, and cargo allows one crate to link the native library ${links[1]}. Retry when ${chain[0] ?? "that crate"} moves to the new line.`
    : `${needs}, which the new spec cannot satisfy.`;
  return { directCrates: [...direct], reason };
}

/** Rewrites the version of each listed crate in a Cargo.toml body to `^target`. */
function rewriteCargoSpecs(
  content: string,
  crates: readonly DependencyStatus[],
): string {
  let out = content;
  for (const crate of crates) {
    // Inline and simple spec forms are mutually exclusive per line. Anchor
    // to line boundaries so `dialog` never matches inside `log`.
    const escaped = crate.name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const regInline = new RegExp(
      `(^|[\\r\\n])(\\s*${escaped}\\s*=\\s*\\{[^}]*version\\s*=\\s*")([^"]+)(")`,
      "g",
    );
    const regSimple = new RegExp(
      `(^|[\\r\\n])(\\s*${escaped}\\s*=\\s*")([^"]+)(")`,
      "g",
    );
    out = out
      .replace(regInline, `$1$2^${crate.latestVersion}$4`)
      .replace(regSimple, `$1$2^${crate.latestVersion}$4`);
  }
  return out;
}

/** Aligned text table; column widths follow the content. */
function printTable(headers: string[], rows: string[][]): void {
  const widths = headers.map((h, i) =>
    Math.max(h.length, ...rows.map((r) => (r[i] ?? "").length)),
  );
  const line = (cells: string[]) =>
    " " + cells.map((c, i) => c.padEnd(widths[i] ?? 0, " ")).join(" | ");
  console.log(line(headers));
  console.log(
    widths
      .map((w) => "-".repeat(w + 2))
      .join("+")
      .slice(1),
  );
  for (const r of rows) console.log(line(r));
}

/**
 * Ecosystem label for reports. A crate declared in more than one Cargo.toml
 * section (serde in build-dependencies, tokio in dev-dependencies, a
 * target-specific block) gets the section in its label, so the second row
 * is not a mystery duplicate: "Cargo (build)", "Cargo (dev)",
 * "Cargo (linux)", "Cargo (all(windows, x86_64))".
 */
function ecosystemLabel(s: DependencyStatus): string {
  if (s.ecosystem !== "Cargo (Rust)" || !s.section) return s.ecosystem;
  const section = s.section;
  if (section === "dependencies") return "Cargo (Rust)";
  if (section.startsWith("build-")) return "Cargo (build)";
  if (section.startsWith("dev-")) return "Cargo (dev)";
  const cfg = section.match(/cfg\((.*)\)'\.dependencies$/)?.[1];
  if (!cfg) return "Cargo (Rust)";
  const short = cfg
    .replace(/target_(?:os|arch|env|family)\s*=\s*"([^"]+)"/g, "$1")
    .replace(/\s+/g, " ");
  return `Cargo (${short})`;
}

interface BehindLatest {
  name: string;
  current: string;
  available: string;
  requiredBy: string;
}

/**
 * Crates cargo will not move because another crate's requirement pins them:
 * the "Unchanged X vA (available: vB)" lines of a verbose dry run, each with
 * its first dependents from `cargo tree --invert`.
 */
function listCratesBehindLatest(cargoCwd: string): BehindLatest[] {
  const run = runCaptured(
    "cargo",
    ["update", "--dry-run", "--verbose"],
    cargoCwd,
  );
  const out: BehindLatest[] = [];
  for (const m of `${run.stdout}\n${run.stderr}`.matchAll(
    /Unchanged (\S+) v(\S+) \(available: v(\S+)\)/g,
  )) {
    const [, name, current, available] = m;
    if (!name || !current || !available) continue;
    const tree = runCaptured(
      "cargo",
      [
        "tree",
        "-i",
        `${name}@${current}`,
        "--depth",
        "1",
        "--prefix",
        "none",
        // Build/dev edges and every target: a crate pinned only by a
        // Linux build-dependency would otherwise print "nothing".
        "--edges",
        "normal,build,dev",
        "--target",
        "all",
      ],
      cargoCwd,
    );
    const dependents = [
      ...new Set(
        tree.stdout
          .split(/\r?\n/)
          .map((l) => l.trim())
          .filter((l) => l && !l.startsWith(`${name} v`))
          .map((l) => l.replace(/ \(.*\)$/, "")),
      ),
    ];
    out.push({
      name,
      current,
      available,
      requiredBy:
        dependents.slice(0, 3).join(", ") +
        (dependents.length > 3 ? `, +${dependents.length - 3}` : ""),
    });
  }
  return out;
}

/**
 * Dry-run report: prints every direct dependency that WOULD be upgraded,
 * the pipeline steps that would run, and exits 0 without touching anything.
 */
function printDryRunReport(allStatuses: DependencyStatus[]): void {
  const outdated = allStatuses.filter((s) => s.needsUpdate);
  const preCount = outdated.filter((s) => s.prerelease).length;

  console.log(
    "=================================================================",
  );
  console.log(
    `🔍 DRY RUN${PRERELEASE_MODE ? " (PRERELEASE PREVIEW)" : ""} — NO CHANGES WILL BE MADE`,
  );
  console.log(
    "=================================================================",
  );

  if (outdated.length === 0) {
    console.log(
      " ✅ All direct dependencies are already at their target versions.",
    );
    if (PRERELEASE_MODE) {
      console.log(
        "    (No pre-release tags found beyond current `latest`/`max_version` targets.)",
      );
    }
  } else {
    console.log(
      ` 📦 ${outdated.length} direct dependency(-ies) WOULD be upgraded:`,
    );
    printTable(
      ["Dependency / Crate Name", "Ecosystem", "Current", "Target", "Pre"],
      outdated
        .toSorted((a, b) => a.name.localeCompare(b.name))
        .map((s) => [
          s.name,
          ecosystemLabel(s),
          s.currentVersion,
          s.latestVersion,
          s.prerelease ? "⚠️" : "",
        ]),
    );
    if (preCount > 0) {
      console.log(
        `\n ⚠️  ${preCount} target(s) are PRE-RELEASES (--prerelease mode). Pre-release builds are`,
      );
      console.log(
        "    unstable by nature — review them before applying with a real run.",
      );
    }
  }

  console.log("\n Steps that WOULD run in a real invocation:");
  console.log("   1-2. bun add <pkg>@<target>        (runtime + dev deps)");
  console.log("   3.   Cargo.toml spec rewrite to ^<target>");
  console.log(
    "   4.   bun update --latest + cargo update   (transitive sub-deps)",
  );
  if (PRERELEASE_MODE) {
    console.log(
      "   4b.  bun add <pkg>@<target> again   (re-pin after --latest clobbers exact pins)",
    );
  }
  console.log("   5.   bun x tsc -b                   (type validation)");
  console.log("   6.   bun run vite:build             (frontend build)");
  console.log("   7.   cargo check                    (backend compile)");
  console.log(
    "=================================================================",
  );
  console.log("✅ Dry run complete — nothing was modified.");
}

async function updateEverything() {
  console.log(
    "=================================================================",
  );
  console.log(
    `🚀 STARTING ULTIMATE DUAL-ECOSYSTEM & SUB-DEPENDENCY UPDATE${PRERELEASE_MODE ? " (PRERELEASE MODE)" : ""}`,
  );
  console.log(
    "=================================================================\n",
  );

  if (PRERELEASE_MODE) {
    console.log(
      "⚠️  PRERELEASE MODE: beta/alpha/RC versions are unstable by design.",
    );
    console.log(
      "   Direct deps will prefer prerelease dist-tags / newest crates.io",
    );
    console.log(
      "   versions; compatibility is still enforced by steps 5-7 (tsc, build, cargo check).\n",
    );
  }

  const pkgPath = path.resolve("package.json");
  const cargoTomlPath = path.resolve("src-tauri/Cargo.toml");
  const cargoLockPath = path.resolve("src-tauri/Cargo.lock");

  if (!fs.existsSync(pkgPath) || !fs.existsSync(cargoTomlPath)) {
    console.error("❌ Fatal: package.json or Cargo.toml not found!");
    process.exit(1);
  }

  console.log(
    "🔍 Querying Registries (NPM & Crates.io) for Target Versions...",
  );
  const queryStart = Date.now();

  const pkgJson = JSON.parse(fs.readFileSync(pkgPath, "utf8"));
  const runtimeDeps = pkgJson.dependencies || {};
  const devDeps = pkgJson.devDependencies || {};

  const allStatuses: DependencyStatus[] = [];
  const fetchPromises: Promise<void>[] = [];

  // 1. Query NPM runtime dependencies
  Object.entries(runtimeDeps).forEach(([name, ver]) => {
    fetchPromises.push(
      (async () => {
        const currClean = cleanVersion(ver as string);
        const latest = await fetchLatestNpmVersion(
          name,
          currClean,
          PRERELEASE_MODE,
        );
        const needs = latest !== null && compareVersions(latest, currClean) > 0;
        allStatuses.push({
          name,
          ecosystem: "NPM (Bun)",
          type: "runtime",
          currentVersion: ver as string,
          latestVersion: latest || currClean,
          needsUpdate: needs,
          prerelease: needs && latest !== null && isPrereleaseVersion(latest),
        });
      })(),
    );
  });

  // 2. Query NPM devDependencies
  Object.entries(devDeps).forEach(([name, ver]) => {
    fetchPromises.push(
      (async () => {
        const currClean = cleanVersion(ver as string);
        const latest = await fetchLatestNpmVersion(
          name,
          currClean,
          PRERELEASE_MODE,
        );
        const needs = latest !== null && compareVersions(latest, currClean) > 0;
        allStatuses.push({
          name,
          ecosystem: "NPM (Bun)",
          type: "dev",
          currentVersion: ver as string,
          latestVersion: latest || currClean,
          needsUpdate: needs,
          prerelease: needs && latest !== null && isPrereleaseVersion(latest),
        });
      })(),
    );
  });

  // 3. Parse Cargo.toml dependency sections safely
  // Matches every section whose name ends in `dependencies` (covers `dependencies`,
  // `build-dependencies`, `dev-dependencies`, and all `target.'cfg(...)'.dependencies`
  // variants including cfg(unix), cfg(not(any(...))), cfg(all(windows, x86_64)), etc.)
  const cargoContent = fs.readFileSync(cargoTomlPath, "utf8");
  const lines = cargoContent.split(/\r?\n/);
  let currentSection = "";

  // Collect all crate names patched in [patch.crates-io] so git forks / patches are never queried on crates.io
  const patchedCrates = new Set<string>();
  let inPatchSection = false;
  lines.forEach((line: string) => {
    const trimmed = line.trim();
    if (trimmed.startsWith("[") && trimmed.endsWith("]")) {
      inPatchSection = trimmed === "[patch.crates-io]";
      return;
    }
    if (inPatchSection) {
      const match = trimmed.match(/^([a-zA-Z0-9_-]+)\s*=/);
      if (match && match[1]) {
        patchedCrates.add(match[1]);
      }
    }
  });

  const cargoCratesToQuery: { name: string; ver: string; section: string }[] =
    [];

  lines.forEach((line: string) => {
    const trimmed = line.trim();
    if (trimmed.startsWith("[") && trimmed.endsWith("]")) {
      currentSection = trimmed.slice(1, -1).trim();
      return;
    }

    // Match any Cargo.toml section that holds dependencies (top-level, build,
    // dev, or any cfg-target-scoped variant) without hardcoding platform names.
    if (/dependencies$/.test(currentSection)) {
      if (trimmed.includes("git =")) return; // skip custom git forks
      const matchInline = trimmed.match(
        /^([a-zA-Z0-9_-]+)\s*=\s*\{[^}]*version\s*=\s*"([^"]+)"/,
      );
      const matchSimple = trimmed.match(/^([a-zA-Z0-9_-]+)\s*=\s*"([^"]+)"/);
      const match = matchInline || matchSimple;
      if (match && match[1] !== undefined && match[2] !== undefined) {
        if (!patchedCrates.has(match[1])) {
          cargoCratesToQuery.push({
            name: match[1],
            ver: match[2],
            section: currentSection,
          });
        }
      }
    }
  });

  cargoCratesToQuery.forEach(({ name, ver, section }) => {
    fetchPromises.push(
      (async () => {
        const currClean = cleanVersion(ver);
        const latest = await fetchLatestCrateVersion(
          name,
          currClean,
          PRERELEASE_MODE && !CARGO_STABLE_ONLY.has(name),
        );
        const needs = latest !== null && compareVersions(latest, currClean) > 0;
        allStatuses.push({
          name,
          ecosystem: "Cargo (Rust)",
          section,
          type: section.includes("build") ? "cargo-build" : "cargo-dep",
          currentVersion: ver,
          latestVersion: latest || currClean,
          needsUpdate: needs,
          prerelease: needs && latest !== null && isPrereleaseVersion(latest),
        });
      })(),
    );
  });

  await Promise.all(fetchPromises);
  const queryDuration = Date.now() - queryStart;
  console.log(`✅ Registry query complete (${queryDuration}ms)\n`);

  // --- DRY RUN: report what would change, then stop before any write occurs ---
  if (DRY_RUN) {
    printDryRunReport(allStatuses);
    process.exit(0);
  }

  // --- Snapshot Sub-Dependency States BEFORE Lockfile Refresh ---
  const beforeCargoLock = parseCargoLock(cargoLockPath);
  const beforeBunLock = parseBunInstalledVersions();

  // --- Step 1: Upgrading Outdated NPM Runtime Dependencies ---
  const outdatedRuntime = allStatuses.filter(
    (s) => s.type === "runtime" && s.needsUpdate,
  );
  if (outdatedRuntime.length === 0) {
    console.log(
      "📦 Step 1/7: NPM Runtime Dependencies -> All direct packages already @target",
    );
  } else {
    console.log(
      `📦 Step 1/7: Upgrading ${outdatedRuntime.length} Outdated NPM Runtime Dependencies...`,
    );
    // Stable mode pins @latest; prerelease mode pins the exact resolved target
    // version (dist-tags are transient, exact versions are deterministic).
    const targets = outdatedRuntime.map(
      (s) => `${s.name}@${PRERELEASE_MODE ? s.latestVersion : "latest"}`,
    );
    const { success, durationMs } = runCmd("bun", ["add", ...targets]);
    if (!success) {
      console.error("❌ Error: NPM runtime dependency upgrade failed!");
      process.exit(1);
    }
    console.log(`✅ Step 1/7 Complete (${durationMs}ms)`);
  }
  console.log("");

  // --- Step 2: Upgrading Outdated NPM DevDependencies ---
  const outdatedDev = allStatuses.filter(
    (s) => s.type === "dev" && s.needsUpdate,
  );
  if (outdatedDev.length === 0) {
    console.log(
      "🛠️ Step 2/7: NPM DevDependencies -> All direct packages already @target",
    );
  } else {
    console.log(
      `🛠️ Step 2/7: Upgrading ${outdatedDev.length} Outdated NPM DevDependencies...`,
    );
    const targets = outdatedDev.map(
      (s) => `${s.name}@${PRERELEASE_MODE ? s.latestVersion : "latest"}`,
    );
    const { success, durationMs } = runCmd("bun", ["add", "-d", ...targets]);
    if (!success) {
      console.error("❌ Error: NPM devDependency upgrade failed!");
      process.exit(1);
    }
    console.log(`✅ Step 2/7 Complete (${durationMs}ms)`);
  }
  console.log("");

  // --- Step 3: Upgrading Outdated Cargo Rust Crates in Cargo.toml ---
  const outdatedCargo = allStatuses.filter(
    (s) => s.ecosystem === "Cargo (Rust)" && s.needsUpdate,
  );
  if (outdatedCargo.length === 0) {
    console.log(
      "🦀 Step 3/7: Cargo Rust Crates -> All Cargo.toml specs already @target",
    );
  } else {
    console.log(
      `🦀 Step 3/7: Rewriting ${outdatedCargo.length} Cargo.toml spec(s): ${outdatedCargo
        .map((c) => `${c.name} ${c.currentVersion} → ^${c.latestVersion}`)
        .join(", ")}`,
    );
    fs.writeFileSync(
      cargoTomlPath,
      rewriteCargoSpecs(cargoContent, outdatedCargo),
      "utf8",
    );
    console.log(
      "✅ Cargo.toml specs rewritten (step 4 verifies they resolve; a conflict reverts the crate at fault)",
    );
  }
  console.log("");

  // --- Capture Direct Spec Snapshot AFTER steps 1-3 ---
  // bun add (steps 1-2) may have rewritten specs for upgraded deps. This
  // snapshot feeds the step 4b clobber guard, which restores EVERY prerelease
  // pin that `bun update --latest` downgrades — including ones this run never
  // targeted (e.g. an already-pinned `typescript 7.1.0-dev.*`).
  const specSnapshot = JSON.parse(fs.readFileSync(pkgPath, "utf8")) as {
    dependencies?: Record<string, string>;
    devDependencies?: Record<string, string>;
  };

  // --- Step 4: Refreshing All Transitive Sub-Crates & Sub-Packages ---
  console.log(
    "🔒 Step 4/7: Refreshing All Sub-Crates & Transitive Sub-Dependencies (bun update & cargo update)...",
  );
  const bunUpdateResult = runCmd("bun", ["update", "--latest"]);
  if (!bunUpdateResult.success) {
    console.error("❌ Error: Bun sub-dependency update failed!");
    process.exit(1);
  }
  const cargoCwd = path.resolve("src-tauri");
  const OWN_PACKAGE = APP.slug;
  /** Direct crates whose bump was reverted this run, with the reason. */
  const heldCargo = new Map<string, string>();
  let acceptedCargo: DependencyStatus[] = outdatedCargo.slice();
  let cargoRun = runCaptured("cargo", ["update"], cargoCwd);

  // A resolution conflict names the crate at fault: hold that one back and
  // retry, at most once per bumped crate.
  for (
    let attempt = 0;
    !cargoRun.success &&
    acceptedCargo.length > 0 &&
    attempt <= outdatedCargo.length;
    attempt++
  ) {
    const conflict = parseCargoResolveError(cargoRun.stderr, OWN_PACKAGE);
    const culprits = (conflict?.directCrates ?? []).filter((n) =>
      acceptedCargo.some((c) => c.name === n),
    );
    if (!conflict || culprits.length === 0) break;
    for (const name of culprits) {
      const crate = acceptedCargo.find((c) => c.name === name);
      if (!crate) continue;
      heldCargo.set(name, conflict.reason);
      console.log(
        `  ⏸ ${name} stays at ${crate.currentVersion} (wanted ^${crate.latestVersion}): ${conflict.reason}`,
      );
    }
    acceptedCargo = acceptedCargo.filter((c) => !culprits.includes(c.name));
    fs.writeFileSync(
      cargoTomlPath,
      rewriteCargoSpecs(cargoContent, acceptedCargo),
      "utf8",
    );
    cargoRun = runCaptured("cargo", ["update"], cargoCwd);
  }

  // A conflict the parser could not attribute: try the bumps one at a time,
  // each against its precise locked version (a bare name is ambiguous when
  // two versions of the crate are locked).
  if (
    !cargoRun.success &&
    acceptedCargo.length > 0 &&
    /failed to select a version/.test(cargoRun.stderr)
  ) {
    console.log(
      "  ⚠️ cargo could not resolve the new specs together; trying them one at a time…",
    );
    const lockVersions = parseCargoLockAll(cargoLockPath);
    let kept: DependencyStatus[] = [];
    for (const crate of acceptedCargo) {
      const candidate = [...kept, crate];
      fs.writeFileSync(
        cargoTomlPath,
        rewriteCargoSpecs(cargoContent, candidate),
        "utf8",
      );
      const locked = pickLockedVersion(
        lockVersions[crate.name] ?? [],
        crate.currentVersion,
      );
      const one = runCaptured(
        "cargo",
        ["update", locked ? `${crate.name}@${locked}` : crate.name],
        cargoCwd,
      );
      if (one.success) {
        kept = candidate;
        console.log(`  ✅ ${crate.name} → ^${crate.latestVersion}`);
      } else {
        const why = firstCargoError(one.stderr);
        heldCargo.set(crate.name, why);
        console.log(
          `  ⏸ ${crate.name} stays at ${crate.currentVersion} (wanted ^${crate.latestVersion}): ${why}`,
        );
      }
    }
    acceptedCargo = kept;
    fs.writeFileSync(
      cargoTomlPath,
      rewriteCargoSpecs(cargoContent, acceptedCargo),
      "utf8",
    );
    cargoRun = runCaptured("cargo", ["update"], cargoCwd);
  }

  printCargoUpdateOutput(cargoRun);
  if (cargoRun.success) {
    console.log(
      `✅ Step 4/7 Sub-dependency update complete (${cargoRun.durationMs}ms)\n`,
    );
  } else {
    console.error("❌ Error: cargo update failed:");
    console.error(cargoRun.stderr.trim());
    process.exit(1);
  }

  // --- Snapshot Sub-Dependency States AFTER Lockfile Refresh ---
  const afterCargoLock = parseCargoLock(cargoLockPath);
  const afterBunLock = parseBunInstalledVersions();

  // --- Step 4b: Re-pin Prerelease Pins (Clobber Guard) ---
  // `bun update --latest` resolves the `latest` dist-tag and REWRITES package.json
  // specs (e.g. `react-dom 19.3.0-canary-d5736f09-20260507 -> 19.2.8`), silently
  // downgrading ANY exact prerelease pin — including ones this run did not touch
  // — and stripping range operators. Stable mode is unaffected (`@latest` pins
  // survive unchanged), so this re-pin only runs in prerelease mode and restores
  // every direct NPM dependency whose snapshot spec is a prerelease.
  if (PRERELEASE_MODE) {
    const pkgNow = JSON.parse(fs.readFileSync(pkgPath, "utf8")) as {
      dependencies?: Record<string, string>;
      devDependencies?: Record<string, string>;
    };
    const repinRuntime: string[] = [];
    const repinDev: string[] = [];
    const specs = {
      ...specSnapshot.dependencies,
      ...specSnapshot.devDependencies,
    };
    for (const [name, spec] of Object.entries(specs)) {
      if (!isPrereleaseVersion(cleanVersion(spec))) continue;
      const currentSpec =
        pkgNow.dependencies?.[name] ?? pkgNow.devDependencies?.[name];
      if (currentSpec === spec) continue;
      const isDev = (specSnapshot.devDependencies ?? {})[name] !== undefined;
      (isDev ? repinDev : repinRuntime).push(`${name}@${spec}`);
    }
    if (repinRuntime.length > 0) {
      const { success: repinRuntimeOk } = runCmd("bun", [
        "add",
        ...repinRuntime,
      ]);
      if (!repinRuntimeOk) {
        console.error(
          "❌ Error: prerelease runtime re-pin failed after bun update --latest!",
        );
        process.exit(1);
      }
    }
    if (repinDev.length > 0) {
      const { success: repinDevOk } = runCmd("bun", ["add", "-d", ...repinDev]);
      if (!repinDevOk) {
        console.error(
          "❌ Error: prerelease dev re-pin failed after bun update --latest!",
        );
        process.exit(1);
      }
    }
    if (repinRuntime.length > 0 || repinDev.length > 0) {
      console.log(
        "🔄 Step 4b/7: Re-pinned exact prerelease targets (bun update --latest clobber guard)\n",
      );
    }
  }

  const subDepChanges: SubDepDiff[] = [];

  // Track Bun Sub-dependency changes
  Object.keys(afterBunLock).forEach((name) => {
    const beforeVer = beforeBunLock[name];
    const afterVer = afterBunLock[name];
    if (beforeVer && afterVer && beforeVer !== afterVer) {
      subDepChanges.push({
        name,
        ecosystem: "NPM (Bun)",
        before: beforeVer,
        after: afterVer,
      });
    }
  });

  // Track Cargo Sub-dependency changes
  Object.keys(afterCargoLock).forEach((name) => {
    const beforeVer = beforeCargoLock[name];
    const afterVer = afterCargoLock[name];
    if (beforeVer && afterVer && beforeVer !== afterVer) {
      subDepChanges.push({
        name,
        ecosystem: "Cargo (Rust)",
        before: beforeVer,
        after: afterVer,
      });
    }
  });

  // --- Step 5: TypeScript Static Type Checking ---
  console.log(
    "📐 Step 5/7: Validating TypeScript Static Types (bun x tsc -b)...",
  );
  const { success: tscSuccess, durationMs: tscMs } = runCmd("bun", [
    "x",
    "tsc",
    "-b",
  ]);
  if (!tscSuccess) {
    console.error("❌ Error: TypeScript type checking failed!");
    process.exit(1);
  } else {
    console.log(`✅ Step 5/7 Complete (${tscMs}ms)\n`);
  }

  // --- Step 6: Vite Production Frontend Build Validation ---
  console.log(
    "⚡ Step 6/7: Validating Vite Production Frontend Build (bun run vite:build)...",
  );
  const { success: buildSuccess, durationMs: buildMs } = runCmd("bun", [
    "run",
    "vite:build",
  ]);
  if (!buildSuccess) {
    console.error("❌ Error: Vite production build failed!");
    process.exit(1);
  } else {
    console.log(`✅ Step 6/7 Complete (${buildMs}ms)\n`);
  }

  // --- Step 7: Native Cargo Backend Compilation Verification ---
  console.log(
    "🔍 Step 7/7: Checking Cargo Rust Backend Compilation (cargo check)...",
  );
  const { success: checkSuccess, durationMs: checkMs } = runCmd(
    "cargo",
    ["check"],
    cargoCwd,
  );
  if (!checkSuccess) {
    console.error("❌ Error: Cargo compilation check failed!");
    process.exit(1);
  } else {
    console.log(`✅ Step 7/7 Complete (${checkMs}ms)\n`);
  }

  const totalDirectCount = allStatuses.length;
  const totalSubCrates = Object.keys(afterCargoLock).length;
  const totalSubNpm = Object.keys(afterBunLock).length;

  // What actually happened to each direct dependency, judged from the
  // lockfile after the run — not from what was merely requested.
  const statusLabel = (s: DependencyStatus): string => {
    if (heldCargo.has(s.name)) return "⏸ Held (see notes)";
    if (!s.needsUpdate) return "⚡ Already @latest";
    if (s.ecosystem === "Cargo (Rust)") {
      const before = beforeCargoLock[s.name];
      const after = afterCargoLock[s.name];
      if (
        before &&
        after &&
        before === after &&
        compareVersions(after, s.latestVersion) >= 0
      ) {
        return `✏️ Spec → ^${s.latestVersion} (lock already had ${after})`;
      }
    }
    return s.prerelease ? "⚠️ Pre-release" : "✨ Upgraded";
  };

  // --- Print Direct Dependency Summary Report ---
  console.log(
    "=================================================================",
  );
  console.log(
    `📊 DIRECT DEPENDENCY STATUS REPORT (${totalDirectCount} DIRECT PACKAGES)`,
  );
  console.log(
    "=================================================================",
  );
  printTable(
    ["Dependency / Crate Name", "Ecosystem", "Current", "Latest", "Status"],
    allStatuses
      .toSorted((a, b) => a.name.localeCompare(b.name))
      .map((s) => [
        s.name,
        ecosystemLabel(s),
        s.currentVersion,
        s.latestVersion,
        statusLabel(s),
      ]),
  );
  if (heldCargo.size > 0) {
    console.log("\n ⏸ Held back this run (Cargo.toml spec left as it was):");
    for (const [name, reason] of heldCargo) {
      console.log(`    ${name}: ${reason}`);
    }
  }
  console.log(
    "=================================================================\n",
  );

  // --- Transitive audit: what moved, and what cargo cannot move ---
  console.log(
    "=================================================================",
  );
  console.log(
    `🔗 TRANSITIVE SUB-DEPENDENCY AUDIT (${totalSubCrates} Rust crates + ${totalSubNpm} NPM packages in the lockfiles)`,
  );
  console.log(
    "=================================================================",
  );
  if (subDepChanges.length === 0) {
    console.log(" ⚡ No transitive package changed in this run.");
  } else {
    console.log(` ✨ ${subDepChanges.length} transitive package(s) moved:`);
    printTable(
      ["Sub-Dependency Name", "Ecosystem", "Before", "After"],
      subDepChanges
        .toSorted((a, b) => a.name.localeCompare(b.name))
        .map((sd) => [sd.name, sd.ecosystem, sd.before, sd.after]),
    );
  }
  console.log(" 🔎 Asking cargo which crates it could not move…");
  const behind = listCratesBehindLatest(cargoCwd).filter(
    (b) => !heldCargo.has(b.name),
  );
  if (behind.length > 0) {
    console.log(
      ` ⏳ ${behind.length} crate(s) are newer on crates.io but pinned by a requirement in the graph:`,
    );
    printTable(
      ["Crate", "Locked", "Available", "Required by"],
      behind.map((b) => [b.name, b.current, b.available, b.requiredBy]),
    );
  }
  console.log(
    "=================================================================",
  );
  if (heldCargo.size === 0 && behind.length === 0) {
    console.log(
      `🎉 Entire dependency tree (${totalDirectCount + totalSubCrates + totalSubNpm} packages) is up to date.\n`,
    );
  } else {
    console.log(
      `🏁 Done. ${heldCargo.size} direct crate(s) held back, ${behind.length} transitive crate(s) behind latest — see the notes above; everything else is up to date.\n`,
    );
  }
}

updateEverything().catch((err) => {
  console.error("❌ Fatal unhandled error:", err);
  process.exit(1);
});
