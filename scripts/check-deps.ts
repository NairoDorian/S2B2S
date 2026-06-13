#!/usr/bin/env bun
/**
 * S2B2S Dependency Version Checker
 *
 * Checks all dependencies used by S2B2S for available updates:
 *   - Bun / Node.js
 *   - Rust (Cargo.toml)
 *   - npm/bun frontend dependencies
 *   - Python TTS engine packages
 *   - Tauri CLI
 *
 * Usage: bun scripts/check-deps.ts
 */

import { readFileSync } from "node:fs";
import { execSync } from "node:child_process";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const projectRoot = resolve(__dirname, "..");
const GREEN = "\x1b[32m";
const YELLOW = "\x1b[33m";
const RED = "\x1b[31m";
const CYAN = "\x1b[36m";
const RESET = "\x1b[0m";
const BOLD = "\x1b[1m";

interface DepResult {
  name: string;
  current: string;
  latest?: string;
  status: "ok" | "outdated" | "error" | "info";
  source: string;
}

const results: DepResult[] = [];

function addResult(r: DepResult) {
  results.push(r);
}

// ─────────────────────────────────────────────────────────────────────────
// 1. Bun / Runtime
// ─────────────────────────────────────────────────────────────────────────
try {
  const bunV = execSync("bun --version", { encoding: "utf8" }).trim();
  const nodeV = execSync("node --version", { encoding: "utf8" }).trim().replace("v", "");
  addResult({ name: "Bun", current: bunV, status: "info", source: "runtime" });
  addResult({ name: "Node.js", current: nodeV, status: "info", source: "runtime" });
} catch {
  addResult({ name: "Bun", current: "not found", status: "error", source: "runtime" });
}

// ─────────────────────────────────────────────────────────────────────────
// 2. Rust / Cargo
// ─────────────────────────────────────────────────────────────────────────
try {
  const rustcV = execSync("rustc --version", { encoding: "utf8" }).trim().split(" ")[1];
  addResult({ name: "Rust (rustc)", current: rustcV, status: "info", source: "toolchain" });

  const cargoV = execSync("cargo --version", { encoding: "utf8" }).trim().split(" ")[1];
  addResult({ name: "Cargo", current: cargoV, status: "info", source: "toolchain" });

  // Check for outdated Rust dependencies (ALL depths, including transitive)
  try {
    const cargoUpdate = execSync("cargo update --dry-run --verbose 2>&1", {
      encoding: "utf8",
      cwd: resolve(projectRoot, "src-tauri"),
      timeout: 120_000,
    });
    const lines = cargoUpdate.split("\n");
    for (const line of lines) {
      // "Updating crate v1.0 -> v2.0" — can be updated via cargo update
      let m = line.match(/^\s*Updating (\S+) v(\S+) -> v(\S+)/);
      if (m) {
        addResult({
          name: `[Rust] ${m[1]}`,
          current: m[2],
          latest: m[3],
          status: "outdated",
          source: "cargo (auto)",
        });
        continue;
      }
      // "Unchanged crate v1.0 (available: v2.0)" — semver-constrained, needs manual Cargo.toml bump
      m = line.match(/^\s*Unchanged (\S+) v(\S+) \(available: v(\S+)\)/);
      if (m) {
        addResult({
          name: `[Rust*] ${m[1]}`,
          current: m[2],
          latest: m[3],
          status: "outdated",
          source: "cargo (constrained)",
        });
      }
    }
  } catch {
    addResult({ name: "Rust deps", current: "check failed", status: "error", source: "cargo" });
  }
} catch {
  addResult({ name: "Rust", current: "not found", status: "error", source: "toolchain" });
}

// ─────────────────────────────────────────────────────────────────────────
// 3. Frontend (package.json) — check for outdated deps
// ─────────────────────────────────────────────────────────────────────────
try {
  // Use bun to check for outdated packages (parse table output, --format json is broken)
  try {
    const tableOut = execSync("bun outdated 2>&1", {
      encoding: "utf8",
      cwd: projectRoot,
      timeout: 60_000,
    });
    const lines = tableOut.split("\n");
    for (const line of lines) {
      // Table format: "│ package-name     │ current │ update │ latest │"
      const m = line.match(/^\│\s*(\S.*?)\s*\│\s*(\S+)\s*\│\s*(\S+)\s*\│\s*(\S+)\s*\│/);
      if (m && m[1] !== "Package") {
        addResult({
          name: `[JS] ${m[1].trim()}`,
          current: m[2],
          latest: m[4],
          status: "outdated",
          source: "package.json",
        });
      }
    }
    // Also show key framework deps as info
    const pkgJson = JSON.parse(
      readFileSync(resolve(projectRoot, "package.json"), "utf8")
    );
    const keyDeps = [
      "@tauri-apps/cli", "vite", "typescript", "react", "react-dom",
      "tailwindcss", "zustand", "i18next", "three", "zod",
    ];
    const allDeps = { ...pkgJson.dependencies, ...pkgJson.devDependencies };
    for (const dep of keyDeps) {
      if (allDeps[dep] && !results.some((r) => r.name === `[JS] ${dep}`)) {
        addResult({
          name: `[JS] ${dep}`,
          current: allDeps[dep].replace("^", "").replace("~", ""),
          status: "info",
          source: "package.json",
        });
      }
    }
  } catch {
    addResult({ name: "[JS] deps", current: "check failed", status: "error", source: "package.json" });
  }
} catch (e) {
  addResult({ name: "[JS] deps", current: "check failed", status: "error", source: "package.json" });
}

// ─────────────────────────────────────────────────────────────────────────
// 4. Python (venv) TTS packages
// ─────────────────────────────────────────────────────────────────────────
const ttsPythonPkgs = ["piper-tts", "kokoro-tts", "pocket-tts", "kittentts", "torch", "soundfile", "numpy"];

// Try venv Python first, then system
let pythonCmd = "";
for (const candidate of [
  resolve(projectRoot, "venv", process.platform === "win32" ? "Scripts/python.exe" : "bin/python"),
  process.platform === "win32" ? "python" : "python3",
  "python",
]) {
  try {
    execSync(`${candidate} --version`, { encoding: "utf8", timeout: 5000 });
    pythonCmd = candidate;
    break;
  } catch {
    continue;
  }
}

if (pythonCmd) {
  try {
    const pyVersion = execSync(`${pythonCmd} --version`, { encoding: "utf8", timeout: 5000 }).trim();
    addResult({ name: "Python", current: pyVersion.replace("Python ", ""), status: "info", source: "venv/system" });
  } catch { /* ignore */ }

  for (const pkg of ttsPythonPkgs) {
    try {
      const info = execSync(`${pythonCmd} -m pip show ${pkg} 2>&1 || echo ""`, {
        encoding: "utf8",
        timeout: 15_000,
      });
      const versionMatch = info.match(/^Version:\s*(.+)$/m);
      if (versionMatch) {
        const current = versionMatch[1];
        // Try to get latest from pip
        let latest: string | undefined;
        try {
          const pipOutdated = execSync(
            `${pythonCmd} -m pip index versions ${pkg} 2>&1 || echo ""`,
            { encoding: "utf8", timeout: 15_000 }
          );
          const latestMatch = pipOutdated.match(/Available versions:\s*(\S+)/);
          if (latestMatch) latest = latestMatch[1].replace(/[,;]$/, "");
        } catch {
          // pip index versions not available on older pip
        }
        addResult({
          name: `[Python] ${pkg}`,
          current,
          latest,
          status: latest && latest !== current ? "outdated" : "ok",
          source: "pip",
        });
      } else {
        addResult({
          name: `[Python] ${pkg}`,
          current: "not installed",
          status: "error",
          source: "pip",
        });
      }
    } catch {
      addResult({
        name: `[Python] ${pkg}`,
        current: "not installed",
        status: "error",
        source: "pip",
      });
    }
  }
} else {
  addResult({ name: "Python", current: "not found", status: "error", source: "venv/system" });
}

// ─────────────────────────────────────────────────────────────────────────
// 5. Tauri CLI
// ─────────────────────────────────────────────────────────────────────────
try {
  const tauriV = execSync("bun run tauri -- --version 2>&1 || cargo tauri --version 2>&1", {
    encoding: "utf8",
    timeout: 15_000,
    cwd: projectRoot,
  }).trim();
  addResult({ name: "Tauri CLI", current: tauriV, status: "info", source: "toolchain" });
} catch {
  addResult({ name: "Tauri CLI", current: "unknown", status: "info", source: "toolchain" });
}

// ─────────────────────────────────────────────────────────────────────────
// Summary
// ─────────────────────────────────────────────────────────────────────────
console.log(`\n${CYAN}${BOLD}════════════════════════════════════════════════════════════${RESET}`);
console.log(`${CYAN}${BOLD}  S2B2S Dependency Status${RESET}`);
console.log(`${CYAN}${BOLD}════════════════════════════════════════════════════════════${RESET}\n`);

const infos = results.filter((r) => r.status === "info");
const oks = results.filter((r) => r.status === "ok");
const outdated = results.filter((r) => r.status === "outdated");
const errors = results.filter((r) => r.status === "error");

if (infos.length > 0) {
  console.log(`${BOLD}Runtime / Toolchain:${RESET}`);
  for (const r of infos) {
    console.log(`  ${r.name.padEnd(30)} ${GREEN}${r.current}${RESET}`);
  }
  console.log("");
}

if (errors.length > 0) {
  console.log(`${BOLD}${RED}Missing / Not Installed:${RESET}`);
  for (const r of errors) {
    console.log(`  ${RED}✗${RESET} ${r.name.padEnd(28)} ${RED}${r.current}${RESET}`);
  }
  console.log("");
}

if (outdated.length > 0) {
  console.log(`${BOLD}${YELLOW}Outdated Dependencies:${RESET}`);
  for (const r of outdated) {
    console.log(
      `  ${YELLOW}↑${RESET} ${r.name.padEnd(28)} ${YELLOW}${r.current}${RESET} -> ${GREEN}${r.latest || "?"}${RESET}`
    );
  }
  console.log("");
}

if (oks.length > 0) {
  console.log(`${BOLD}Up-to-Date:${RESET}`);
  for (const r of oks) {
    console.log(`  ${GREEN}✓${RESET} ${r.name.padEnd(28)} ${r.current}`);
  }
  console.log("");
}

// One-line summary
const summary = [];
if (infos.length) summary.push(`${infos.length} toolchain`);
if (oks.length) summary.push(`${oks.length} up-to-date`);
if (outdated.length) summary.push(`${YELLOW}${outdated.length} outdated${RESET}`);
if (errors.length) summary.push(`${RED}${errors.length} missing${RESET}`);

console.log(`${CYAN}${BOLD}Summary:${RESET} ${summary.join(", ")}`);
