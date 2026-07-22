#!/usr/bin/env bun
/**
 * S2B2S One-Shot Dependency Upgrader
 *
 * Automatically updates ALL dependencies across all languages in one command:
 *   1. Bun / Frontend npm dependencies (`bun update`)
 *   2. Cargo / Rust crates (`cargo update` in src-tauri)
 *   3. Python TTS Virtual Environment (`venv` pip / uv package upgrade)
 *   4. Runs automated type checks (`tsc --noEmit`), binding exports (`cargo test`), and i18n checks
 *
 * Usage:
 *   bun run update:all
 *   OR
 *   bun scripts/update-all-deps.ts
 */

import { execSync } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const projectRoot = resolve(__dirname, "..");
const srcTauriDir = resolve(projectRoot, "src-tauri");

const GREEN = "\x1b[32m";
const YELLOW = "\x1b[33m";
const RED = "\x1b[31m";
const CYAN = "\x1b[36m";
const RESET = "\x1b[0m";
const BOLD = "\x1b[1m";

function logStep(stepNum: number, title: string) {
  console.log(`\n${BOLD}${CYAN}[Step ${stepNum}] ${title}${RESET}`);
  console.log("─".repeat(60));
}

function runCmd(
  cmd: string,
  cwd: string = projectRoot,
  label?: string,
  allowFail = false,
) {
  if (label) {
    console.log(`${YELLOW}➜ Executing:${RESET} ${label} (${cmd})`);
  } else {
    console.log(`${YELLOW}➜ Executing:${RESET} ${cmd}`);
  }
  try {
    execSync(cmd, { cwd, stdio: "inherit", encoding: "utf8" });
    console.log(`${GREEN}✔ Succeeded!${RESET}`);
    return true;
  } catch (error: any) {
    if (allowFail) {
      console.log(
        `${YELLOW}⚠️ Optional command failed (non-critical):${RESET} ${cmd}`,
      );
      return true;
    }
    console.error(`${RED}✘ Command failed:${RESET} ${cmd}`);
    return false;
  }
}

console.log(`${BOLD}${GREEN}
════════════════════════════════════════════════════════════
  🚀 S2B2S Full Stack Dependency Upgrade Engine
════════════════════════════════════════════════════════════${RESET}`);

let hasErrors = false;

// ─────────────────────────────────────────────────────────────────────────
// Step 1: Update Frontend Dependencies (Bun / Node / React / Vite)
// ─────────────────────────────────────────────────────────────────────────
logStep(1, "Updating Frontend JS/TS Dependencies (Bun)");
const bunOk = runCmd("bun update", projectRoot, "Bun Package Update");
if (!bunOk) hasErrors = true;

// ─────────────────────────────────────────────────────────────────────────
// Step 2: Update Backend Dependencies (Rust / Cargo)
// ─────────────────────────────────────────────────────────────────────────
logStep(2, "Updating Backend Rust Dependencies (Cargo)");
const cargoOk = runCmd("cargo update", srcTauriDir, "Cargo Crates Update");
if (!cargoOk) hasErrors = true;

// ─────────────────────────────────────────────────────────────────────────
// Step 3: Update Python Virtual Environment Dependencies (TTS Engines)
// ─────────────────────────────────────────────────────────────────────────
logStep(3, "Updating Python TTS Virtual Environment Dependencies");
const isWin = process.platform === "win32";
const venvPy = isWin
  ? resolve(projectRoot, "venv", "Scripts", "python.exe")
  : resolve(projectRoot, "venv", "bin", "python");

if (existsSync(venvPy)) {
  const pyCmd = `"${venvPy}" -m pip install --upgrade piper-tts kokoro-tts pocket-tts kittentts torch numpy soundfile`;
  const pyOk = runCmd(pyCmd, projectRoot, "Python venv Upgrade", true);
  if (!pyOk) hasErrors = true;
} else {
  console.log(
    `${YELLOW}ℹ Python venv not found at ${venvPy}. Skipping Python package updates.${RESET}`,
  );
  console.log(
    `${YELLOW}  (Run .\\scripts\\setup_tts_venv.ps1 or bash scripts/setup_tts_venv.sh to set up venv)${RESET}`,
  );
}

// ─────────────────────────────────────────────────────────────────────────
// Step 4: Verification (TypeScript + Rust Specta + i18n)
// ─────────────────────────────────────────────────────────────────────────
logStep(4, "Running Full Stack Verification Checks");

console.log(`\n${CYAN}1/3 Verifying TypeScript Types...${RESET}`);
const tscOk = runCmd("bunx tsc --noEmit", projectRoot, "TypeScript Check");
if (!tscOk) hasErrors = true;

console.log(`\n${CYAN}2/3 Verifying i18n Translations...${RESET}`);
const i18nOk = runCmd(
  "bun scripts/check-translations.ts",
  projectRoot,
  "Translation Check",
);
if (!i18nOk) hasErrors = true;

console.log(
  `\n${CYAN}3/3 Regenerating Specta Bindings & Rust Compilation Check...${RESET}`,
);
const testOk = runCmd(
  "cargo test export_bindings",
  srcTauriDir,
  "Rust Specta Export & Compilation",
);
if (!testOk) hasErrors = true;

// ─────────────────────────────────────────────────────────────────────────
// Summary
// ─────────────────────────────────────────────────────────────────────────
console.log(
  `\n${BOLD}${CYAN}════════════════════════════════════════════════════════════${RESET}`,
);
if (!hasErrors) {
  console.log(
    `${BOLD}${GREEN}🎉 ALL DEPENDENCIES UPDATED & VERIFIED SUCCESSFULLY! 🎉${RESET}`,
  );
  console.log(
    `${GREEN}Your project is fully up-to-date across Bun, Cargo, and Python.${RESET}`,
  );
} else {
  console.log(`${BOLD}${RED}⚠️ UPDATE COMPLETED WITH WARNINGS/ERRORS.${RESET}`);
  console.log(
    `${YELLOW}Please inspect the log output above for details.${RESET}`,
  );
  process.exit(1);
}
console.log(
  `${BOLD}${CYAN}════════════════════════════════════════════════════════════${RESET}\n`,
);
