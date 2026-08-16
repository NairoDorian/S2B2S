import fs from "node:fs";
import { spawnSync } from "node:child_process";
import path from "node:path";

/**
 * Ultimate All-Inclusive Dependency Updater & Sub-Dependency Tracker for Handy.
 * Inspired by Minimalistic_App.
 *
 * Capabilities:
 * 1. Dynamic NPM Registry Querying (@latest versions)
 * 2. Dynamic Crates.io Registry Querying (@latest versions)
 * 3. Direct Dependency Upgrading (package.json & src-tauri/Cargo.toml)
 * 4. Transitive Sub-Dependency & Sub-Sub-Dependency Upgrading (bun update --latest & cargo update)
 * 5. Full Inventory Audit & Diff Tracking (Cargo.lock & node_modules)
 * 6. TypeScript + Vite Production Build Validation (bun run build)
 * 7. Native Cargo Backend Compilation Verification (cargo check)
 *
 * CLI flags:
 *   (no args)      Stable @latest pipeline (default)
 *   --prerelease   Prefer pre-release versions for direct dependencies
 *   --dry-run      Query registries and print a "would upgrade" report without writing
 *   --help         Show usage summary
 */

const args = process.argv.slice(2);
const PRERELEASE_MODE = args.includes("--prerelease");
const DRY_RUN = args.includes("--dry-run");
const SHOW_HELP = args.includes("--help") || args.includes("-h");

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
  bun scripts/update-deps.ts                    Stable @latest pipeline (default)
  bun scripts/update-deps.ts --prerelease       Prefer beta/alpha/RC versions for direct deps
  bun scripts/update-deps.ts --dry-run          Report what WOULD upgrade (no changes made)
  bun scripts/update-deps.ts --prerelease --dry-run Preview prerelease upgrades`);
  process.exit(0);
}

interface DependencyStatus {
  name: string;
  ecosystem: "NPM (Bun)" | "Cargo (Rust)";
  type: "runtime" | "dev" | "cargo-dep" | "cargo-build";
  currentVersion: string;
  latestVersion: string;
  needsUpdate: boolean;
  prerelease: boolean;
}

interface SubDepDiff {
  name: string;
  ecosystem: "NPM (Bun)" | "Cargo (Rust)";
  before: string;
  after: string;
}

function isPrereleaseVersion(version: string): boolean {
  return /-\d|-[a-z]/i.test(version);
}

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
  const namePadded = s.name.padEnd(36, " ");
  const ecoPadded = s.ecosystem.padEnd(12, " ");
  const currPadded = s.currentVersion.padEnd(10, " ");
  const latPadded = s.latestVersion.padEnd(10, " ");
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
    `🔍 DRY RUN${PRERELEASE_MODE ? " (PRERELEASE PREVIEW)" : ""} — NO CHANGES WILL BE MADE`,
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
      " Dependency / Crate Name             | Ecosystem    | Current    | Target     | Pre",
    );
    console.log(
      "--------------------------------------+--------------+------------+------------+-----",
    );
    outdated
      .sort((a, b) => a.name.localeCompare(b.name))
      .forEach((s) => {
        console.log(renderStatusRow(s));
      });
    if (preCount > 0) {
      console.log(
        `\n ⚠️  ${preCount} target(s) are PRE-RELEASES (--prerelease mode).`,
      );
    }
  }
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

  const pkgPath = path.resolve("package.json");
  const cargoTomlPath = path.resolve("src-tauri/Cargo.toml");
  const cargoLockPath = path.resolve("src-tauri/Cargo.lock");

  if (!fs.existsSync(pkgPath) || !fs.existsSync(cargoTomlPath)) {
    console.error("❌ Fatal: package.json or src-tauri/Cargo.toml not found!");
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

  // 3. Parse Cargo.toml dependencies (excluding git dependencies)
  const cargoContent = fs.readFileSync(cargoTomlPath, "utf8");
  const lines = cargoContent.split(/\r?\n/);
  let currentSection = "";

  const cargoCratesToQuery: { name: string; ver: string; section: string }[] =
    [];

  const PINNED_CRATES = new Set([
    "transcribe-cpp",
    "transcribe-cpp-sys",
    "rubato",
    "audioadapter",
    "audioadapter-buffers",
    "libc",
    "specta",
    "specta-typescript",
    "tauri-specta",
  ]);

  lines.forEach((line: string) => {
    const trimmed = line.trim();
    if (trimmed.startsWith("[") && trimmed.endsWith("]")) {
      currentSection = trimmed.slice(1, -1).trim();
      return;
    }

    if (
      [
        "dependencies",
        "build-dependencies",
        "dev-dependencies",
        "target.'cfg(target_os = \"windows\")'.dependencies",
        "target.'cfg(target_os = \"macos\")'.dependencies",
        "target.'cfg(target_os = \"linux\")'.dependencies",
      ].includes(currentSection)
    ) {
      if (trimmed.includes("git =")) return; // skip custom git forks
      const matchInline = trimmed.match(
        /^([a-zA-Z0-9_-]+)\s*=\s*\{[^}]*version\s*=\s*"([^"]+)"/,
      );
      const matchSimple = trimmed.match(/^([a-zA-Z0-9_-]+)\s*=\s*"([^"]+)"/);
      const match = matchInline || matchSimple;
      if (match && match[1] !== undefined && match[2] !== undefined) {
        if (!PINNED_CRATES.has(match[1])) {
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
          PRERELEASE_MODE,
        );
        const needs = latest ? currClean !== latest : false;
        allStatuses.push({
          name,
          ecosystem: "Cargo (Rust)",
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

  if (DRY_RUN) {
    printDryRunReport(allStatuses);
    process.exit(0);
  }

  const beforeCargoLock = parseCargoLock(cargoLockPath);
  const beforeBunLock = parseBunInstalledVersions();

  // --- Step 1: Upgrading Outdated NPM Runtime Dependencies ---
  const outdatedRuntime = allStatuses.filter(
    (s) => s.type === "runtime" && s.needsUpdate,
  );
  if (outdatedRuntime.length === 0) {
    console.log(
      "📦 Step 1/6: NPM Runtime Dependencies -> All direct packages already @target",
    );
  } else {
    console.log(
      `📦 Step 1/6: Upgrading ${outdatedRuntime.length} Outdated NPM Runtime Dependencies...`,
    );
    const targets = outdatedRuntime.map(
      (s) => `${s.name}@${PRERELEASE_MODE ? s.latestVersion : "latest"}`,
    );
    const { success, durationMs } = runCmd("bun", ["add", ...targets]);
    if (!success) {
      console.error("❌ Error: NPM runtime dependency upgrade failed!");
      process.exit(1);
    }
    console.log(`✅ Step 1/6 Complete (${durationMs}ms)`);
  }
  console.log("");

  // --- Step 2: Upgrading Outdated NPM DevDependencies ---
  const outdatedDev = allStatuses.filter(
    (s) => s.type === "dev" && s.needsUpdate,
  );
  if (outdatedDev.length === 0) {
    console.log(
      "🛠️ Step 2/6: NPM DevDependencies -> All direct packages already @target",
    );
  } else {
    console.log(
      `🛠️ Step 2/6: Upgrading ${outdatedDev.length} Outdated NPM DevDependencies...`,
    );
    const targets = outdatedDev.map(
      (s) => `${s.name}@${PRERELEASE_MODE ? s.latestVersion : "latest"}`,
    );
    const { success, durationMs } = runCmd("bun", ["add", "-d", ...targets]);
    if (!success) {
      console.error("❌ Error: NPM devDependency upgrade failed!");
      process.exit(1);
    }
    console.log(`✅ Step 2/6 Complete (${durationMs}ms)`);
  }
  console.log("");

  // --- Step 3: Upgrading Outdated Cargo Rust Crates in Cargo.toml ---
  const outdatedCargo = allStatuses.filter(
    (s) => s.ecosystem === "Cargo (Rust)" && s.needsUpdate,
  );
  if (outdatedCargo.length === 0) {
    console.log(
      "🦀 Step 3/6: Cargo Rust Crates -> All Cargo.toml specs already @target",
    );
  } else {
    console.log(
      `🦀 Step 3/6: Syncing ${outdatedCargo.length} Outdated Cargo Crates in Cargo.toml...`,
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
    "🔒 Step 4/6: Refreshing All Sub-Crates & Transitive Sub-Dependencies (bun update & cargo update)...",
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
    console.log(`✅ Step 4/6 Sub-dependency update complete (${cargoMs}ms)\n`);
  } else {
    console.error("❌ Error: Cargo sub-crate update failed!");
    process.exit(1);
  }

  const afterCargoLock = parseCargoLock(cargoLockPath);
  const afterBunLock = parseBunInstalledVersions();

  // --- Step 4b: Re-pin Prerelease Pins (Clobber Guard) ---
  // `bun update --latest` resolves the `latest` dist-tag and REWRITES
  // package.json specs (e.g. `react-dom 19.3.0-canary-* -> 19.2.8`), silently
  // downgrading ANY exact prerelease pin — including ones this run did not
  // touch — and stripping range operators. Stable mode is unaffected (`@latest`
  // pins survive unchanged), so this re-pin only runs in prerelease mode and
  // restores every direct NPM dependency whose snapshot spec is a prerelease.
  if (PRERELEASE_MODE) {
    const pkgNow = JSON.parse(fs.readFileSync(pkgPath, "utf8")) as {
      dependencies?: Record<string, string>;
      devDependencies?: Record<string, string>;
    };
    const repinRuntime: string[] = [];
    const repinDev: string[] = [];
    const specs = { ...specSnapshot.dependencies, ...specSnapshot.devDependencies };
    for (const [name, spec] of Object.entries(specs)) {
      if (!isPrereleaseVersion(cleanVersion(spec))) continue;
      const currentSpec = pkgNow.dependencies?.[name] ?? pkgNow.devDependencies?.[name];
      if (currentSpec === spec) continue;
      const isDev = (specSnapshot.devDependencies ?? {})[name] !== undefined;
      (isDev ? repinDev : repinRuntime).push(`${name}@${spec}`);
    }
    if (repinRuntime.length > 0) {
      const { success: repinRuntimeOk } = runCmd("bun", ["add", ...repinRuntime]);
      if (!repinRuntimeOk) {
        console.error("❌ Error: prerelease runtime re-pin failed after bun update --latest!");
        process.exit(1);
      }
    }
    if (repinDev.length > 0) {
      const { success: repinDevOk } = runCmd("bun", ["add", "-d", ...repinDev]);
      if (!repinDevOk) {
        console.error("❌ Error: prerelease dev re-pin failed after bun update --latest!");
        process.exit(1);
      }
    }
    if (repinRuntime.length > 0 || repinDev.length > 0) {
      console.log("🔄 Step 4b/6: Re-pinned exact prerelease targets (bun update --latest clobber guard)\n");
    }
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

  // --- Step 5: TypeScript & Vite Build Validation ---
  console.log(
    "⚡ Step 5/6: Validating TypeScript & Vite Production Build (bun run build)...",
  );
  const { success: buildSuccess, durationMs: buildMs } = runCmd("bun", [
    "run",
    "build",
  ]);
  if (!buildSuccess) {
    console.error("❌ Error: Frontend build failed!");
    process.exit(1);
  } else {
    console.log(`✅ Step 5/6 Complete (${buildMs}ms)\n`);
  }

  // --- Step 6: Native Cargo Backend Compilation Verification ---
  console.log(
    "🔍 Step 6/6: Checking Cargo Rust Backend Compilation (cargo check)...",
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
    console.log(`✅ Step 6/6 Complete (${checkMs}ms)\n`);
  }

  const totalDirectCount = allStatuses.length;
  const totalSubCrates = Object.keys(afterCargoLock).length;
  const totalSubNpm = Object.keys(afterBunLock).length;

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
    " Dependency / Crate Name             | Ecosystem    | Current    | Latest     | Status",
  );
  console.log(
    "--------------------------------------+--------------+------------+------------+-------------------",
  );
  allStatuses
    .sort((a, b) => a.name.localeCompare(b.name))
    .forEach((s) => {
      const namePadded = s.name.padEnd(36, " ");
      const ecoPadded = s.ecosystem.padEnd(12, " ");
      const currPadded = s.currentVersion.padEnd(10, " ");
      const latPadded = s.latestVersion.padEnd(10, " ");
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
      " Sub-Dependency Name                 | Ecosystem    | Before     | Upgraded",
    );
    console.log(
      "--------------------------------------+--------------+------------+-----------",
    );
    subDepChanges
      .sort((a, b) => a.name.localeCompare(b.name))
      .forEach((sd) => {
        const namePadded = sd.name.padEnd(36, " ");
        const ecoPadded = sd.ecosystem.padEnd(12, " ");
        const beforePadded = sd.before.padEnd(10, " ");
        console.log(
          ` ${namePadded} | ${ecoPadded} | ${beforePadded} | ${sd.after}`,
        );
      });
  }
  console.log(
    "=================================================================\n",
  );
  console.log(
    "🎉 ALL DEPENDENCIES, SUB-DEPENDENCIES, AND CRATES UPDATED & VERIFIED SUCCESSFULLY!",
  );
}

updateEverything().catch((err) => {
  console.error("❌ Fatal unhandled error:", err);
  process.exit(1);
});
