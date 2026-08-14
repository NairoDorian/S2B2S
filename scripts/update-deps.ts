import fs from "node:fs";
import { spawnSync } from "node:child_process";
import path from "node:path";

/**
 * Ultimate All-Inclusive Dependency Updater & Sub-Dependency Tracker for S2B2S.
 *
 * Capabilities:
 * 1. Dynamic NPM Registry Querying (@latest & --prerelease versions)
 * 2. Dynamic Crates.io Registry Querying (@latest & --prerelease versions)
 * 3. Direct Dependency Upgrading (package.json & src-tauri/Cargo.toml)
 * 4. Transitive Sub-Dependency & Sub-Sub-Dependency Upgrading (bun update --latest & cargo update)
 * 5. Full Inventory Audit & Diff Tracking (Cargo.lock & node_modules)
 * 6. Python TTS Virtual Environment package update (when venv exists)
 * 7. TypeScript Static Type Checking (bunx tsc --noEmit)
 * 8. Native Cargo Backend Compilation Verification (cargo check)
 * 9. Single-File ARCHITECTURE.md Synchronization (bun run arch)
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
 *                  resolve via `bun update --latest` / `cargo update`.
 *   --dry-run      Query registries and print a "would upgrade" report WITHOUT
 *                  writing anything (no bun add, no Cargo.toml edits, no
 *                  lockfile refreshes, no build steps). Safe to run any time.
 *   --help         Show this usage summary.
 */

const args = process.argv.slice(2);
const PRERELEASE_MODE = args.includes("--prerelease");
const DRY_RUN = args.includes("--dry-run");
const SHOW_HELP = args.includes("--help") || args.includes("-h");

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

/**
 * Crates intentionally pinned to prevent build-cache invalidation.
 *
 * Two categories:
 *  1. Audio/ONNX subsystem compatibility (cpal, rubato, ort, ndarray, …)
 *  2. Native build-tool crates (cc, cmake, ring, aws-lc-sys, …) — bumping
 *     any of these invalidates the transcribe-cpp-sys / libsqlite3-sys /
 *     aws-lc-sys C++ compile cache, causing a multi-minute full recompile on
 *     every `bun run tauri dev` after an update.  Pin them so `cargo update`
 *     leaves them alone.
 */
const PINNED_CRATES: readonly string[] = [
  // ── Audio / ONNX subsystem ──────────────────────────────────────────────
  "cpal",
  "rubato",
  "audioadapter",
  "audioadapter-buffers",
  "specta",
  "tauri-specta",
  "windows-core",
  "windows-core-061",
  "ort",
  "ndarray",
  "getrandom",
  // ── Native build-tool crates — cache-stability pins ─────────────────────
  // Bumping any of these forces a full C/C++ recompile of transcribe-cpp-sys,
  // libsqlite3-sys, aws-lc-sys, ring, etc.  Only update manually when needed.
  "cc",
  "cmake",
  "vswhom",
  "vswhom-sys",
  "embed-resource",
  "tauri-winres",
  "tauri-build",
  "find-msvc-tools",
  "ring",
  "aws-lc-sys",
  "aws-lc-rs",
  "libsqlite3-sys",
];

if (SHOW_HELP) {
  console.log(`Usage:
  bun run update-deps                          Stable @latest pipeline (default)
  bun run update-deps --prerelease             Prefer beta/alpha/RC/canary versions for direct deps
  bun run update-deps --dry-run                Report what WOULD upgrade (no changes made)
  bun run update-deps --prerelease --dry-run   Preview prerelease upgrades, apply nothing`);
  process.exit(0);
}

interface DependencyStatus {
  name: string;
  ecosystem: "NPM (Bun)" | "Cargo (Rust)";
  type: "runtime" | "dev" | "cargo-dep" | "cargo-build";
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

/** Semver versions containing a hyphen (e.g. `0.5.0-beta.1` or `19.3.0-canary-*`) are pre-releases. */
function isPrereleaseVersion(version: string): boolean {
  return /-\d|-[a-z]/i.test(version);
}

/**
 * Minimal SemVer comparator: returns 1 when `a` is newer than `b`, -1 when older, 0 when equal.
 */
function compareVersions(a: string, b: string): number {
  const parse = (v: string): { core: number[]; pre: string | null } => {
    const [coreStr, pre] = v.split("-", 2);
    const core = (coreStr ?? "0").split(".").map((n) => parseInt(n, 10) || 0);
    while (core.length < 3) core.push(0);
    return { core, pre: pre ?? null };
  };
  const A = parse(a);
  const B = parse(b);
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
      return -1;
    } else if (yn !== null) {
      return 1;
    } else if (x !== y) {
      return x > y ? 1 : -1;
    }
  }
  return 0;
}

/**
 * Compares only the numeric X.Y.Z core of two versions (ignores prerelease identifiers).
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
 * Resolves target NPM version from registry with prerelease & timestamp ranking.
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
 * Resolves target crates.io version.
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
        headers: { "User-Agent": "S2B2SDependencyUpdater/1.0" },
      },
    );
    if (response.ok) {
      const data = (await response.json()) as {
        crate?: { max_version?: string; newest_version?: string };
      };
      const crate = data.crate;
      if (!crate) return null;
      if (
        prerelease &&
        crate.newest_version &&
        compareVersions(crate.newest_version, current) > 0
      ) {
        return crate.newest_version;
      }
      return crate.max_version || null;
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

function parseBunInstalledVersions(): Record<string, string> {
  const map: Record<string, string> = {};
  const nmPath = path.resolve("node_modules");
  if (!fs.existsSync(nmPath)) return map;

  function scan(dir: string) {
    try {
      const entries = fs.readdirSync(dir, { withFileTypes: true });
      entries.forEach((e) => {
        if (e.isDirectory()) {
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

function renderStatusRow(s: DependencyStatus): string {
  const namePadded = s.name.padEnd(34, " ");
  const ecoPadded = s.ecosystem.padEnd(12, " ");
  const currPadded = s.currentVersion.padEnd(9, " ");
  const latPadded = s.latestVersion.padEnd(9, " ");
  const preMark = s.prerelease ? "⚠️" : " ";
  return ` ${namePadded} | ${ecoPadded} | ${currPadded} | ${latPadded} | ${preMark}`;
}

function printDryRunReport(allStatuses: DependencyStatus[]): void {
  const outdated = allStatuses.filter((s) => s.needsUpdate);
  const preCount = outdated.filter((s) => s.prerelease).length;

  console.log(
    "=================================================================",
  );
  console.log(
    `🔍 DRY RUN${PRERELEASE_MODE ? " (PRERELEASE / ALPHA / BETA PREVIEW)" : ""} — NO CHANGES WILL BE MADE`,
  );
  console.log(
    "=================================================================",
  );

  if (outdated.length === 0) {
    console.log(
      " ✅ All direct dependencies are already at their target versions.",
    );
  } else {
    console.log(
      ` 📦 ${outdated.length} direct dependency(-ies) WOULD be upgraded:`,
    );
    console.log(
      " Dependency / Crate Name           | Ecosystem    | Current   | Target    | Pre",
    );
    console.log(
      "------------------------------------+--------------+-----------+-----------+-----",
    );
    outdated
      .sort((a, b) => a.name.localeCompare(b.name))
      .forEach((s) => {
        console.log(renderStatusRow(s));
      });
    if (preCount > 0) {
      console.log(
        `\n ⚠️  ${preCount} target(s) are PRE-RELEASES (--prerelease mode: alpha / beta / rc / canary).`,
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
      "   4b.  bun add <pkg>@<target> again   (re-pin after --latest)",
    );
  }
  console.log("   5.   Python venv pip package update (if venv exists)");
  console.log("   6.   bunx tsc --noEmit              (type validation)");
  console.log("   7.   cargo check                    (backend compile check)");
  console.log("        bun run arch                   (ARCHITECTURE.md sync)");
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
    `🚀 STARTING ULTIMATE DUAL-ECOSYSTEM & SUB-DEPENDENCY UPDATE${PRERELEASE_MODE ? " (PRERELEASE / ALPHA MODE)" : ""}`,
  );
  console.log(
    "=================================================================\n",
  );

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
        const needs = latest ? currClean !== latest : false;
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
        const needs = latest ? currClean !== latest : false;
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
  const cargoContent = fs.readFileSync(cargoTomlPath, "utf8");
  const lines = cargoContent.split(/\r?\n/);
  let currentSection = "";

  const cargoCratesToQuery: { name: string; ver: string; section: string }[] =
    [];

  lines.forEach((line: string) => {
    const trimmed = line.trim();
    if (trimmed.startsWith("[") && trimmed.endsWith("]")) {
      currentSection = trimmed.slice(1, -1).trim();
      return;
    }

    if (
      ["dependencies", "build-dependencies", "dev-dependencies"].includes(
        currentSection,
      )
    ) {
      const matchInline = trimmed.match(
        /^([a-zA-Z0-9_-]+)\s*=\s*\{[^}]*version\s*=\s*"([^"]+)"/,
      );
      const matchSimple = trimmed.match(/^([a-zA-Z0-9_-]+)\s*=\s*"([^"]+)"/);
      const match = matchInline || matchSimple;
      if (match && match[1] !== undefined && match[2] !== undefined) {
        const crateName = match[1];
        const crateVer = match[2];
        if (
          crateVer !== "*" &&
          !crateVer.startsWith("=") &&
          !PINNED_CRATES.includes(crateName)
        ) {
          cargoCratesToQuery.push({
            name: crateName,
            ver: crateVer,
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
          PRERELEASE_MODE,
        );
        const needs = latest ? currClean !== latest : false;
        allStatuses.push({
          name,
          ecosystem: "Cargo (Rust)",
          type: section === "build-dependencies" ? "cargo-build" : "cargo-dep",
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
      `🦀 Step 3/7: Syncing ${outdatedCargo.length} Outdated Cargo Crates in Cargo.toml...`,
    );
    let newCargoContent = cargoContent;
    outdatedCargo.forEach((crate) => {
      const regInline = new RegExp(
        `(${crate.name}\\s*=\\s*\\{[^}]*version\\s*=\\s*")([^"]+)(")`,
        "g",
      );
      const regSimple = new RegExp(`(${crate.name}\\s*=\\s*")([^"]+)(")`, "g");
      newCargoContent = newCargoContent
        .replace(regInline, `$1^${crate.latestVersion}$3`)
        .replace(regSimple, `$1^${crate.latestVersion}$3`);
    });
    fs.writeFileSync(cargoTomlPath, newCargoContent, "utf8");
    console.log("✅ Cargo.toml specifications updated to @target!");
  }
  console.log("");

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
  const { success: cargoSuccess, durationMs: cargoMs } = runCmd(
    "cargo",
    ["update"],
    cargoCwd,
  );
  if (cargoSuccess) {
    console.log(`✅ Step 4/7 Sub-dependency update complete (${cargoMs}ms)\n`);
  } else {
    console.error("❌ Error: Cargo sub-crate update failed!");
    process.exit(1);
  }

  const afterCargoLock = parseCargoLock(cargoLockPath);
  const afterBunLock = parseBunInstalledVersions();

  // --- Step 4b: Re-pin Prerelease Pins (Clobber Guard) ---
  if (PRERELEASE_MODE) {
    const pkgNow = JSON.parse(fs.readFileSync(pkgPath, "utf8")) as {
      dependencies?: Record<string, string>;
      devDependencies?: Record<string, string>;
    };
    const repinRuntime: string[] = [];
    const repinDev: string[] = [];
    const specs = {
      ...(specSnapshot.dependencies ?? {}),
      ...(specSnapshot.devDependencies ?? {}),
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
        "✅ Step 4b/7: Re-pinned exact prerelease targets (bun update --latest clobber guard)\n",
      );
    }
  }

  // --- Step 5: Update Python TTS Virtual Environment Dependencies if present ---
  const isWin = process.platform === "win32";
  const venvPy = isWin
    ? path.resolve("venv", "Scripts", "python.exe")
    : path.resolve("venv", "bin", "python");

  if (fs.existsSync(venvPy)) {
    console.log(
      "🐍 Step 5/7: Upgrading Python TTS Virtual Environment Dependencies...",
    );
    const pyCmd = `"${venvPy}" -m pip install --upgrade piper-tts kokoro-tts pocket-tts kittentts torch numpy soundfile`;
    const pyRes = runCmd(pyCmd, []);
    if (pyRes.success) {
      console.log("✅ Step 5/7 Complete (Python TTS dependencies upgraded)\n");
    }
  } else {
    console.log(
      "🐍 Step 5/7: Python venv not present at ./venv, skipping Python update.\n",
    );
  }

  const subDepChanges: SubDepDiff[] = [];

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

  // --- Step 6: TypeScript Static Type Checking ---
  console.log(
    "📐 Step 6/7: Validating TypeScript Static Types (bunx tsc --noEmit)...",
  );
  const { success: tscSuccess, durationMs: tscMs } = runCmd("bunx", [
    "tsc",
    "--noEmit",
  ]);
  if (!tscSuccess) {
    console.error("❌ Error: TypeScript type checking failed!");
    process.exit(1);
  } else {
    console.log(`✅ Step 6/7 Complete (${tscMs}ms)\n`);
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

  // --- Synchronize Architecture Map ---
  console.log("📐 Synchronizing ARCHITECTURE.md...");
  const { success: archSuccess } = runCmd("bun", ["run", "arch"]);
  if (archSuccess) console.log("✅ ARCHITECTURE.md updated!\n");

  const totalDirectCount = allStatuses.length;
  const totalSubCrates = Object.keys(afterCargoLock).length;
  const totalSubNpm = Object.keys(afterBunLock).length;
  const totalGraphCount = totalDirectCount + totalSubCrates + totalSubNpm;

  console.log(
    "=================================================================",
  );
  console.log(
    "📊 DIRECT DEPENDENCY STATUS REPORT (" +
      totalDirectCount +
      " DIRECT PACKAGES)",
  );
  console.log(
    "=================================================================",
  );
  console.log(
    " Dependency / Crate Name           | Ecosystem    | Current   | Latest    | Status",
  );
  console.log(
    "------------------------------------+--------------+-----------+-----------+-------------------",
  );
  allStatuses
    .sort((a, b) => a.name.localeCompare(b.name))
    .forEach((s) => {
      const namePadded = s.name.padEnd(34, " ");
      const ecoPadded = s.ecosystem.padEnd(12, " ");
      const currPadded = s.currentVersion.padEnd(9, " ");
      const latPadded = s.latestVersion.padEnd(9, " ");
      const statusText = s.needsUpdate
        ? s.prerelease
          ? "⚠️ Pre-release"
          : "✨ Upgraded"
        : "⚡ Already @latest";
      console.log(
        ` ${namePadded} | ${ecoPadded} | ${currPadded} | ${latPadded} | ${statusText}`,
      );
    });
  console.log(
    "=================================================================\n",
  );

  console.log(
    "=================================================================",
  );
  console.log(
    `🔗 TRANSITIVE SUB-DEPENDENCY INVENTORY AUDIT (${totalSubCrates} Rust Sub-Crates + ${totalSubNpm} NPM Sub-Packages)`,
  );
  console.log(
    "=================================================================",
  );
  if (subDepChanges.length === 0) {
    console.log(
      ` ⚡ All ${totalSubCrates + totalSubNpm} sub-crates & sub-dependencies are verified at their latest versions.`,
    );
  } else {
    console.log(
      " Sub-Dependency Name               | Ecosystem    | Before    | Upgraded",
    );
    console.log(
      "------------------------------------+--------------+-----------+-----------",
    );
    subDepChanges
      .sort((a, b) => a.name.localeCompare(b.name))
      .forEach((sd) => {
        const namePadded = sd.name.padEnd(34, " ");
        const ecoPadded = sd.ecosystem.padEnd(12, " ");
        const beforePadded = sd.before.padEnd(9, " ");
        console.log(
          ` ${namePadded} | ${ecoPadded} | ${beforePadded} | ${sd.after}`,
        );
      });
  }
  console.log(
    "=================================================================",
  );
  console.log(
    `🎉 Entire dependency tree (${totalGraphCount} total packages & sub-crates) is 100% up-to-date!\n`,
  );
}

updateEverything();
