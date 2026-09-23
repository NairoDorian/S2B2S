// scripts/prune-target.ts
//
// Removes stale build artifacts from src-tauri/target. Cargo never deletes
// the outputs of units that stopped existing: every `cargo update`, pin bump
// or feature change leaves the previous hashed artifacts, build-script
// outputs and incremental caches behind. After a few months the debug
// profile alone was 65 GB here (33 GB of deps, 25 GB of incremental caches,
// 7 GB of build-script outputs, six copies of the transcribe.cpp native build
// among them). `tauri-runner.ts` calls `pruneTarget` before every
// `tauri dev` / `tauri build`; `bun run prune:target [--dry-run] [--verbose]`
// runs it by hand.
//
// What goes, per profile directory (debug, release):
//   - incremental/<crate>-<key>: every key but the two newest per crate (a
//     crate's lib and test units keep separate keys). A key changes whenever
//     the unit's metadata does and an old key is never read again.
//   - deps/<crate>-<hash>.*: units whose dep-info (.d) points at a registry
//     version or git revision that is no longer in Cargo.lock; for the
//     workspace's own crates (the app, its tests and benches) every hash but
//     the two newest per crate and kind.
//   - build/<crate>-<hash>: build-script outputs whose recorded paths point
//     at a version / revision no longer in Cargo.lock; for the app crate,
//     every run but the two newest.
//   - .fingerprint/<crate>-<hash> of everything removed above.
//   - bundle/**: installers of a version other than tauri.conf.json's
//     (an unversioned artefact is kept: it cannot be proven stale).
//
// Nothing current is touched, so the next build is no slower than it would
// have been. The only guess is "newest two" for the app crate; a wrong guess
// costs one recompilation of that unit, never a broken build. Set the `NO_PRUNE`
// flag to skip the automatic run (its prefix comes from app-meta.ts — see
// scripts/lib/env-flag.ts).

import { existsSync, readdirSync, readFileSync, rmSync, statSync } from "fs";
import { join, resolve } from "path";

const TAG = "[prune-target]";
const PROFILES = ["debug", "release"];
/** How many of the newest hashes to keep for the workspace's own units. */
const KEEP_WORKSPACE_UNITS = 2;

const HASHED_FILE = /^(.+?)-([0-9a-f]{16})(\..+)?$/;
const HASHED_DIR = /^(.+?)-([0-9a-f]{16})$/;
const INCREMENTAL_DIR = /^(.+)-([a-z0-9]+)$/;
const REGISTRY_PATH =
  /[\\/]registry[\\/]src[\\/][^\\/]+[\\/]([^\\/]+?)-(\d+\.\d+\.\d+[^\\/\s]*)[\\/]/;
const GIT_PATH = /[\\/]git[\\/]checkouts[\\/][^\\/]+[\\/]([0-9a-f]{7})[\\/]/;
/**
 * Dep-info of the workspace's own units lists sources relative to the
 * package root ("src\lib.rs", "tests\x.rs", "build.rs"); registry and git
 * units list absolute paths into ~/.cargo.
 */
const WORKSPACE_PATH = /\s(src|tests|benches)[\\/]|\sbuild\.rs(\s|$)/;

export interface PruneOptions {
  /** The `src-tauri` directory. */
  tauriDir: string;
  dryRun?: boolean;
  /** Print every removed path. */
  verbose?: boolean;
}

export interface PruneReport {
  removedBytes: number;
  removedItems: number;
  skipped: number;
  lines: string[];
}

interface Lock {
  registry: Set<string>;
  gitRevs: Set<string>;
  workspace: Set<string>;
}

interface Unit {
  hash: string;
  name: string;
  files: string[];
  bytes: number;
  newest: number;
  exe: boolean;
  rlib: boolean;
  depInfo: string | null;
}

const fmt = (bytes: number): string => {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(0)} MB`;
  return `${Math.ceil(bytes / 1024)} KB`;
};

const sizeOf = (path: string): number => {
  try {
    const st = statSync(path);
    if (!st.isDirectory()) return st.size;
    let total = 0;
    for (const entry of readdirSync(path, { withFileTypes: true })) {
      total += entry.isDirectory()
        ? sizeOf(join(path, entry.name))
        : (() => {
            try {
              return statSync(join(path, entry.name)).size;
            } catch {
              return 0;
            }
          })();
    }
    return total;
  } catch {
    return 0;
  }
};

const mtimeOf = (path: string): number => {
  try {
    return statSync(path).mtimeMs;
  } catch {
    return 0;
  }
};

/** Versions and revisions the lockfile still references. */
function readLock(tauriDir: string): Lock {
  const lock: Lock = {
    registry: new Set(),
    gitRevs: new Set(),
    workspace: new Set(),
  };
  const text = readFileSync(join(tauriDir, "Cargo.lock"), "utf8");
  for (const block of text.split("[[package]]").slice(1)) {
    const name = block.match(/^name = "([^"]+)"/m)?.[1];
    const version = block.match(/^version = "([^"]+)"/m)?.[1];
    const source = block.match(/^source = "([^"]+)"/m)?.[1];
    if (!name || !version) continue;
    if (!source) {
      lock.workspace.add(name);
    } else if (source.startsWith("git+")) {
      const rev = source.split("#")[1];
      if (rev) lock.gitRevs.add(rev.slice(0, 7));
    } else {
      lock.registry.add(`${name}@${version}`);
    }
  }
  return lock;
}

/** "stale" when the text names a version / revision no longer in the lock. */
function classify(
  text: string,
  lock: Lock,
): "stale" | "current" | "workspace" | "unknown" {
  const registry = text.match(REGISTRY_PATH);
  if (registry) {
    return lock.registry.has(`${registry[1]}@${registry[2]}`)
      ? "current"
      : "stale";
  }
  const git = text.match(GIT_PATH);
  if (git) return lock.gitRevs.has(git[1]) ? "current" : "stale";
  if (WORKSPACE_PATH.test(text)) return "workspace";
  return "unknown";
}

class Pruner {
  readonly report: PruneReport = {
    removedBytes: 0,
    removedItems: 0,
    skipped: 0,
    lines: [],
  };
  private readonly removedHashes = new Set<string>();

  constructor(private readonly opts: PruneOptions) {}

  private remove(path: string, why: string): number {
    const bytes = sizeOf(path);
    if (this.opts.verbose) {
      console.log(
        `${TAG} ${this.opts.dryRun ? "would remove" : "remove"} ${fmt(bytes).padStart(7)}  ${path}  (${why})`,
      );
    }
    if (!this.opts.dryRun) {
      try {
        rmSync(path, { recursive: true, force: true });
      } catch (error) {
        this.report.skipped += 1;
        console.warn(`${TAG} could not remove ${path}: ${String(error)}`);
        return 0;
      }
    }
    this.report.removedBytes += bytes;
    this.report.removedItems += 1;
    return bytes;
  }

  /** incremental/<crate>-<key>: keep the two newest keys per crate. */
  pruneIncremental(profileDir: string): number {
    const dir = join(profileDir, "incremental");
    if (!existsSync(dir)) return 0;
    const byCrate = new Map<string, { path: string; mtime: number }[]>();
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      const m = entry.name.match(INCREMENTAL_DIR);
      if (!m) continue;
      const path = join(dir, entry.name);
      const list = byCrate.get(m[1]) ?? [];
      list.push({ path, mtime: mtimeOf(path) });
      byCrate.set(m[1], list);
    }
    let freed = 0;
    for (const list of byCrate.values()) {
      list.sort((a, b) => b.mtime - a.mtime);
      // A crate's lib build and its test build are separate units with
      // separate keys, so two live keys per crate name is the normal case.
      for (const { path } of list.slice(KEEP_WORKSPACE_UNITS)) {
        freed += this.remove(path, "superseded incremental cache");
      }
    }
    return freed;
  }

  /** deps/<crate>-<hash>.*: units of versions gone from the lock. */
  pruneDeps(profileDir: string, lock: Lock): number {
    const dir = join(profileDir, "deps");
    if (!existsSync(dir)) return 0;
    const units = new Map<string, Unit>();
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      if (!entry.isFile()) continue;
      const m = entry.name.match(HASHED_FILE);
      if (!m) continue;
      const [, stem, hash, ext] = m;
      const path = join(dir, entry.name);
      const unit = units.get(hash) ?? {
        hash,
        name: stem,
        files: [],
        bytes: 0,
        newest: 0,
        exe: false,
        rlib: false,
        depInfo: null,
      };
      unit.files.push(path);
      unit.newest = Math.max(unit.newest, mtimeOf(path));
      // A binary is `.exe` on Windows and has no extension on Linux/macOS.
      if (ext === ".exe" || ext === undefined) unit.exe = true;
      if (ext === ".rlib") unit.rlib = true;
      if (ext === ".d") {
        unit.depInfo = path;
        unit.name = stem;
      }
      units.set(hash, unit);
    }

    let freed = 0;
    const workspace = new Map<string, Unit[]>();
    for (const unit of units.values()) {
      if (!unit.depInfo) continue; // no dep-info: cargo's own final copies
      let text: string;
      try {
        text = readFileSync(unit.depInfo, "utf8");
      } catch {
        continue;
      }
      switch (classify(text, lock)) {
        case "stale":
          freed += this.removeUnit(unit, "version no longer in Cargo.lock");
          break;
        case "workspace": {
          // Kinds are kept apart so `cargo check` / clippy runs (metadata
          // only) never push the real rlib or the binaries out.
          const kind = unit.exe ? "bin" : unit.rlib ? "lib" : "check";
          const key = `${unit.name}:${kind}`;
          const list = workspace.get(key) ?? [];
          list.push(unit);
          workspace.set(key, list);
          break;
        }
        default:
          break;
      }
    }
    for (const list of workspace.values()) {
      list.sort((a, b) => b.newest - a.newest);
      for (const unit of list.slice(KEEP_WORKSPACE_UNITS)) {
        freed += this.removeUnit(unit, "superseded workspace build");
      }
    }
    return freed;
  }

  private removeUnit(unit: Unit, why: string): number {
    let freed = 0;
    for (const file of unit.files) freed += this.remove(file, why);
    this.removedHashes.add(unit.hash);
    return freed;
  }

  /** build/<crate>-<hash>: build-script runs of versions gone from the lock. */
  pruneBuildDirs(profileDir: string, lock: Lock): number {
    const dir = join(profileDir, "build");
    if (!existsSync(dir)) return 0;
    let freed = 0;
    const workspace = new Map<
      string,
      { path: string; hash: string; mtime: number }[]
    >();
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      const m = entry.name.match(HASHED_DIR);
      if (!m) continue;
      const [, crate, hash] = m;
      const path = join(dir, entry.name);
      if (lock.workspace.has(crate)) {
        const list = workspace.get(crate) ?? [];
        list.push({
          path,
          hash,
          mtime: mtimeOf(join(path, "invoked.timestamp")),
        });
        workspace.set(crate, list);
        continue;
      }
      const output = join(path, "output");
      if (!existsSync(output)) continue;
      let text: string;
      try {
        text = readFileSync(output, "utf8");
      } catch {
        continue;
      }
      if (classify(text, lock) === "stale") {
        freed += this.remove(
          path,
          "build script of a version no longer in Cargo.lock",
        );
        this.removedHashes.add(hash);
      }
    }
    for (const list of workspace.values()) {
      list.sort((a, b) => b.mtime - a.mtime);
      for (const { path, hash } of list.slice(KEEP_WORKSPACE_UNITS)) {
        freed += this.remove(path, "superseded build-script run");
        this.removedHashes.add(hash);
      }
    }
    return freed;
  }

  /** .fingerprint/<crate>-<hash> of units removed above. */
  pruneFingerprints(profileDir: string): number {
    const dir = join(profileDir, ".fingerprint");
    if (!existsSync(dir) || this.removedHashes.size === 0) return 0;
    let freed = 0;
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      const m = entry.name.match(HASHED_DIR);
      if (m && this.removedHashes.has(m[2])) {
        freed += this.remove(
          join(dir, entry.name),
          "fingerprint of a removed unit",
        );
      }
    }
    return freed;
  }

  /** bundle/<kind>/*: installers of another version. */
  pruneBundles(profileDir: string, version: string | null): number {
    const dir = join(profileDir, "bundle");
    if (!existsSync(dir) || !version) return 0;
    let freed = 0;
    for (const kind of readdirSync(dir, { withFileTypes: true })) {
      if (!kind.isDirectory()) continue;
      const kindDir = join(dir, kind.name);
      for (const entry of readdirSync(kindDir, { withFileTypes: true })) {
        if (!entry.isFile()) continue;
        // `_<version>` (msi, nsis, deb, AppImage, dmg) or `-<version>-` (rpm)
        // is this version; a name with no version at all (the macOS updater
        // `.app.tar.gz` / `.sig`) cannot be proven stale and is kept.
        if (
          entry.name.includes(`_${version}`) ||
          entry.name.includes(`-${version}-`) ||
          !/\d+\.\d+\.\d+/.test(entry.name)
        )
          continue;
        freed += this.remove(
          join(kindDir, entry.name),
          "installer of another version",
        );
      }
    }
    return freed;
  }
}

function appVersion(tauriDir: string): string | null {
  try {
    const conf = JSON.parse(
      readFileSync(join(tauriDir, "tauri.conf.json"), "utf8"),
    );
    return typeof conf.version === "string" ? conf.version : null;
  } catch {
    return null;
  }
}

export function pruneTarget(opts: PruneOptions): PruneReport {
  const tauriDir = resolve(opts.tauriDir);
  const target = join(tauriDir, "target");
  const pruner = new Pruner(opts);
  if (!existsSync(target) || !existsSync(join(tauriDir, "Cargo.lock"))) {
    return pruner.report;
  }
  const lock = readLock(tauriDir);
  const version = appVersion(tauriDir);
  for (const profile of PROFILES) {
    const profileDir = join(target, profile);
    if (!existsSync(profileDir)) continue;
    const before = pruner.report.removedBytes;
    const parts: string[] = [];
    const incremental = pruner.pruneIncremental(profileDir);
    if (incremental) parts.push(`incremental ${fmt(incremental)}`);
    const deps = pruner.pruneDeps(profileDir, lock);
    if (deps) parts.push(`deps ${fmt(deps)}`);
    const build = pruner.pruneBuildDirs(profileDir, lock);
    if (build) parts.push(`build scripts ${fmt(build)}`);
    pruner.pruneFingerprints(profileDir);
    const bundles = pruner.pruneBundles(profileDir, version);
    if (bundles) parts.push(`old installers ${fmt(bundles)}`);
    const freed = pruner.report.removedBytes - before;
    if (freed > 0) {
      pruner.report.lines.push(
        `${profile}: ${fmt(freed)} (${parts.join(", ")})`,
      );
    }
  }
  return pruner.report;
}

/** Print the report the way the build runner does; returns whether anything went. */
export function printReport(report: PruneReport, dryRun: boolean): boolean {
  if (report.removedItems === 0) {
    if (dryRun) console.log(`${TAG} Nothing stale in src-tauri/target.`);
    return false;
  }
  const verb = dryRun ? "Would remove" : "Removed";
  console.log(
    `${TAG} ${verb} ${fmt(report.removedBytes)} of stale build artifacts — ${report.lines.join("; ")}` +
      (report.skipped ? ` (${report.skipped} in use, skipped)` : ""),
  );
  return true;
}

if (import.meta.main) {
  const args = process.argv.slice(2);
  const dryRun = args.includes("--dry-run") || args.includes("-n");
  const verbose = args.includes("--verbose") || args.includes("-v");
  const tauriDir = resolve(import.meta.dirname, "..", "src-tauri");
  const report = pruneTarget({ tauriDir, dryRun, verbose });
  printReport(report, dryRun);
}
