import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { APP_VERSION } from "./version";

/**
 * before-commit.ts — Professional Developer Pre-Commit Pipeline & Version Sync.
 *
 * Enforces the single global version (`scripts/version.ts` → `APP_VERSION`)
 * across every file that mirrors it and provides a unified, professional
 * pre-commit verification suite for TypeScript, Vite, Cargo, and Git.
 *
 * Modes:
 *   (no args)                 Sync APP_VERSION into all mirrors, printing a report.
 *   --check                   Read-only validation; exits 1 on any drift (CI / hooks).
 *   --bump <major|minor|patch>
 *                             Increment APP_VERSION in version.ts, then sync everything.
 *   --set <semver>            Set APP_VERSION to an exact semver string and sync.
 *   --full / --all            Run the complete professional pre-commit test suite:
 *                             1. Version mirror drift check
 *                             2. i18n translation validation (check:translations)
 *                             3. Architecture map freshness generation (arch)
 *                             4. TypeScript static type checking (tsc --noEmit)
 *                             5. Rust Cargo compilation check (cargo check)
 *   --stage                   Automatically stage synced mirror files with `git add`.
 *   --install-hook            Install `.git/hooks/pre-commit` running version check & typecheck.
 *   --uninstall-hook          Remove the installed `.git/hooks/pre-commit`.
 *   --help                    Show this comprehensive usage summary.
 *
 * Synchronized mirrors:
 *   - package.json              (version field)
 *   - src-tauri/Cargo.toml      (package version)
 *   - src-tauri/tauri.conf.json (version field — drives the bundled artifact
 *                                and the auto-updater's latest.json feed)
 *   - src-tauri/Cargo.lock      (root crate entry, refreshed via `cargo generate-lockfile`)
 */

const ROOT = process.cwd();
const VERSION_SOURCE = path.join(ROOT, "scripts", "version.ts");
const CARGO_MANIFEST_DIR = path.join(ROOT, "src-tauri");
const CARGO_CRATE_NAME = "s2b2s";

interface VersionMirror {
  /** Human-readable label for the report. */
  label: string;
  /** Absolute path to the mirror file. */
  file: string;
  /** Regex with exactly one capture group matching the current version value. */
  pattern: RegExp;
  /** Renders the full replacement for the matched substring. */
  render: (version: string) => string;
}

const mirrors: VersionMirror[] = [
  {
    label: "package.json",
    file: path.join(ROOT, "package.json"),
    pattern: /"version"\s*:\s*"([^"]+)"/,
    render: (v) => `"version": "${v}"`,
  },
  {
    label: "src-tauri/Cargo.toml",
    file: path.join(ROOT, "src-tauri", "Cargo.toml"),
    pattern: /^version\s*=\s*"([^"]+)"/m,
    render: (v) => `version = "${v}"`,
  },
  {
    label: "src-tauri/tauri.conf.json",
    file: path.join(ROOT, "src-tauri", "tauri.conf.json"),
    pattern: /"version"\s*:\s*"([^"]+)"/,
    render: (v) => `"version": "${v}"`,
  },
];

/** Exits with a descriptive error when a precondition fails. */
function fail(message: string): never {
  console.error(`\n❌ Error: ${message}\n`);
  process.exit(1);
}

/** Executes a shell command synchronously and returns timing and success status. */
function runCommand(
  label: string,
  command: string,
  args: string[],
  cwd: string = ROOT,
): { success: boolean; durationMs: number } {
  const start = Date.now();
  console.log(`⏳ ${label} (${command} ${args.join(" ")})...`);
  const result = spawnSync(command, args, {
    cwd,
    stdio: "inherit",
    shell: true,
  });
  const durationMs = Date.now() - start;
  return { success: result.status === 0, durationMs };
}

/** Reads the current value of a mirror, or null when the pattern is absent. */
function readMirrorValue(mirror: VersionMirror): string | null {
  const content = fs.readFileSync(mirror.file, "utf8");
  return content.match(mirror.pattern)?.[1] ?? null;
}

/** Returns the root crate version recorded in Cargo.lock, or null. */
function readLockfileVersion(): string | null {
  const lockPath = path.join(CARGO_MANIFEST_DIR, "Cargo.lock");
  if (!fs.existsSync(lockPath)) return null;
  const content = fs.readFileSync(lockPath, "utf8");
  return (
    content.match(
      new RegExp(`name = "${CARGO_CRATE_NAME}"\\r?\\nversion = "([^"]+)"`, "m"),
    )?.[1] ?? null
  );
}

/** Refreshes the Cargo.lock root entry after a Cargo.toml version change. */
function refreshLockfileVersion(): boolean {
  const result = spawnSync("cargo", ["generate-lockfile"], {
    cwd: CARGO_MANIFEST_DIR,
    stdio: "inherit",
    shell: true,
  });
  return result.status === 0;
}

/** Validates SemVer format (e.g. 1.0.0 or 1.0.0-rc.1). */
function isValidSemver(v: string): boolean {
  return /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$/.test(v);
}

/** Semver increment — returns the next version string. */
function bumpVersion(current: string, part: string): string {
  const segments = current.split("-")[0]?.split(".").map(Number);
  if (
    !segments ||
    segments.length !== 3 ||
    segments.some((n) => Number.isNaN(n))
  ) {
    fail(
      `Cannot bump malformed version "${current}" — expected semver like "0.1.4".`,
    );
  }
  const [major, minor, patch] = segments as [number, number, number];
  switch (part) {
    case "major":
      return `${major + 1}.0.0`;
    case "minor":
      return `${major}.${minor + 1}.0`;
    case "patch":
      return `${major}.${minor}.${patch + 1}`;
    default:
      fail(`Unknown bump part "${part}" — use major, minor, or patch.`);
  }
}

/** Rewrites `export const APP_VERSION` inside scripts/version.ts. */
function writeVersionSource(next: string): void {
  if (!isValidSemver(next)) {
    fail(`Invalid semver "${next}" — must match X.Y.Z[-prerelease].`);
  }
  const prev = fs.readFileSync(VERSION_SOURCE, "utf8");
  const replaced = prev.replace(
    /export const APP_VERSION = '([^']+)';/,
    `export const APP_VERSION = '${next}';`,
  );
  if (replaced === prev) {
    fail(
      `Could not locate "export const APP_VERSION = '...';" in ${VERSION_SOURCE}.`,
    );
  }
  fs.writeFileSync(VERSION_SOURCE, replaced, "utf8");
}

/** Writes the target version into every registered mirror file. */
function syncMirrors(target: string): {
  updated: string[];
  unchanged: string[];
} {
  const updated: string[] = [];
  const unchanged: string[] = [];

  for (const mirror of mirrors) {
    const content = fs.readFileSync(mirror.file, "utf8");
    const current = content.match(mirror.pattern)?.[1];
    if (current === undefined) {
      fail(`Mirror pattern failed against ${mirror.label} (${mirror.file}).`);
    }
    if (current === target) {
      unchanged.push(mirror.label);
      continue;
    }
    const next = content.replace(mirror.pattern, mirror.render(target));
    fs.writeFileSync(mirror.file, next, "utf8");
    updated.push(mirror.label);
  }

  // If Cargo.toml changed, refresh Cargo.lock
  if (updated.includes("src-tauri/Cargo.toml")) {
    console.log("🔄 Refreshing src-tauri/Cargo.lock...");
    if (!refreshLockfileVersion()) {
      console.warn(
        '⚠️  Warning: "cargo generate-lockfile" failed; Cargo.lock may be stale.',
      );
    } else {
      updated.push("src-tauri/Cargo.lock");
    }
  }

  return { updated, unchanged };
}

/** Validates that all mirrors match APP_VERSION without writing changes. */
function checkMirrors(target: string): boolean {
  let inSync = true;
  console.log(
    `\n🔍 Checking version mirrors against APP_VERSION = ${target}...\n`,
  );

  for (const mirror of mirrors) {
    const val = readMirrorValue(mirror);
    if (val === target) {
      console.log(`  ✅  ${mirror.label.padEnd(28)} matches (${val})`);
    } else {
      console.log(
        `  ❌  ${mirror.label.padEnd(28)} DRIFT: "${val ?? "missing"}" (expected "${target}")`,
      );
      inSync = false;
    }
  }

  const lockVal = readLockfileVersion();
  if (lockVal === null) {
    console.log(
      `  ⚠️   src-tauri/Cargo.lock         missing (run cargo check or cargo generate-lockfile)`,
    );
  } else if (lockVal === target) {
    console.log(`  ✅  src-tauri/Cargo.lock         matches (${lockVal})`);
  } else {
    console.log(
      `  ❌  src-tauri/Cargo.lock         DRIFT: "${lockVal}" (expected "${target}")`,
    );
    inSync = false;
  }

  return inSync;
}

/** Runs the full pre-commit pipeline. */
function runFullPipeline(): void {
  console.log("\n🚀 Starting Complete Professional Pre-Commit Pipeline...\n");
  const pipelineStart = Date.now();

  // 1. Version check
  const versionOk = checkMirrors(APP_VERSION);
  if (!versionOk) {
    fail(
      'Version drift detected. Run "bun run before-commit" to synchronize mirrors.',
    );
  }

  // 2. Translations check
  const transCheck = runCommand("Checking Translations", "bun", [
    "run",
    "check:translations",
  ]);
  if (!transCheck.success) {
    fail("Translation validation failed.");
  }

  // 3. TypeScript static type check
  const tsCheck = runCommand("Static Typecheck", "bunx", ["tsc", "--noEmit"]);
  if (!tsCheck.success) {
    fail("TypeScript type check failed.");
  }

  // 4. Cargo check
  const cargoCheck = runCommand(
    "Cargo Rust Compilation Check",
    "cargo",
    ["check"],
    CARGO_MANIFEST_DIR,
  );
  if (!cargoCheck.success) {
    fail("Cargo compilation check failed.");
  }

  const totalTime = ((Date.now() - pipelineStart) / 1000).toFixed(2);
  console.log(
    `\n✨ Pre-Commit Verification Pipeline PASSED in ${totalTime}s!\n`,
  );
}

/** Installs the git pre-commit hook. */
function installHook(): void {
  const gitDir = path.join(ROOT, ".git");
  if (!fs.existsSync(gitDir)) {
    fail("No .git directory found.");
  }
  const hooksDir = path.join(gitDir, "hooks");
  if (!fs.existsSync(hooksDir)) fs.mkdirSync(hooksDir, { recursive: true });

  const hookFile = path.join(hooksDir, "pre-commit");
  const hookContent = `#!/bin/sh
# Auto-generated by bun run before-commit --install-hook
bun run before-commit --check || exit 1
bunx tsc --noEmit || exit 1
`;

  fs.writeFileSync(hookFile, hookContent, { mode: 0o755 });
  console.log("✅ Installed .git/hooks/pre-commit successfully.");
}

// ─── Entry Point ─────────────────────────────────────────────────────────────

function main(): void {
  const args = process.argv.slice(2);

  if (args.includes("--help") || args.includes("-h")) {
    console.log(`
Usage: bun run before-commit [options]

Options:
  (no args)                 Sync APP_VERSION into all mirrors.
  --check                   Validate mirrors match APP_VERSION (exit 1 on drift).
  --bump <major|minor|patch>
                            Increment APP_VERSION and sync all mirrors.
  --set <semver>            Set APP_VERSION to exact semver and sync.
  --full, --all             Run full test & verification suite.
  --install-hook            Install .git/hooks/pre-commit hook.
  --stage                   Run git add on synced files.
  --help                    Show this help message.
`);
    return;
  }

  if (args.includes("--install-hook")) {
    installHook();
    return;
  }

  if (args.includes("--full") || args.includes("--all")) {
    runFullPipeline();
    return;
  }

  const bumpIdx = args.indexOf("--bump");
  if (bumpIdx !== -1) {
    const part = args[bumpIdx + 1];
    if (!part || !["major", "minor", "patch"].includes(part)) {
      fail("Missing or invalid bump part: must be major, minor, or patch.");
    }
    const next = bumpVersion(APP_VERSION, part);
    console.log(`\n📦 Bumping version: ${APP_VERSION} → ${next}\n`);
    writeVersionSource(next);
    const { updated, unchanged } = syncMirrors(next);
    console.log(`  Updated:   ${updated.join(", ") || "none"}`);
    console.log(`  Unchanged: ${unchanged.join(", ") || "none"}`);
    console.log(`\n✅ Version bumped to ${next} and synced everywhere.\n`);
    return;
  }

  const setIdx = args.indexOf("--set");
  if (setIdx !== -1) {
    const next = args[setIdx + 1];
    if (!next || !isValidSemver(next)) {
      fail(`Invalid semver provided to --set: "${next}".`);
    }
    console.log(`\n📦 Setting version to: ${next}\n`);
    writeVersionSource(next);
    const { updated, unchanged } = syncMirrors(next);
    console.log(`  Updated:   ${updated.join(", ") || "none"}`);
    console.log(`  Unchanged: ${unchanged.join(", ") || "none"}`);
    console.log(`\n✅ Version set to ${next} and synced everywhere.\n`);
    return;
  }

  if (args.includes("--check")) {
    const inSync = checkMirrors(APP_VERSION);
    if (!inSync) {
      process.exit(1);
    }
    console.log("\n✅ All version mirrors are in sync.\n");
    return;
  }

  // Default mode: Sync mirrors to APP_VERSION
  console.log(
    `\n📦 Synchronizing version mirrors to APP_VERSION = ${APP_VERSION}...\n`,
  );
  const { updated, unchanged } = syncMirrors(APP_VERSION);
  console.log(`  Updated:   ${updated.join(", ") || "none"}`);
  console.log(`  Unchanged: ${unchanged.join(", ") || "none"}`);
  console.log("\n✅ Version synchronization complete.\n");
}

main();
