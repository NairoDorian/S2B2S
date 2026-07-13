#!/usr/bin/env bun
/**
 * Conflict resolver for S2B2S merge conflicts.
 * Strategy:
 *  - JSON locale files: UNION of both sides; upstream wins on value conflicts
 *  - Doc files: S2B2S HEAD wins structurally, upstream additions are merged in
 */

import { readFileSync, writeFileSync, readdirSync } from "fs";
import { join } from "path";
import { execSync } from "child_process";

const ROOT = process.cwd();

// ─── Conflict parser ──────────────────────────────────────────────────────────

/**
 * Parses a file with conflict markers and returns an array of segments:
 *   { type: 'common' | 'ours' | 'theirs', content: string }
 */
function parseConflicts(text) {
  const lines = text.split(/\r?\n/);
  const segments = [];
  let current = { type: "common", lines: [] };

  for (const line of lines) {
    if (/^<<<<<<< /.test(line)) {
      if (current.lines.length > 0) {
        segments.push({
          type: current.type,
          content: current.lines.join("\n"),
        });
      }
      current = { type: "ours", lines: [] };
    } else if (/^=======$/.test(line)) {
      segments.push({ type: current.type, content: current.lines.join("\n") });
      current = { type: "theirs", lines: [] };
    } else if (/^>>>>>>> /.test(line)) {
      segments.push({ type: current.type, content: current.lines.join("\n") });
      current = { type: "common", lines: [] };
    } else {
      current.lines.push(line);
    }
  }
  if (current.lines.length > 0) {
    segments.push({ type: current.type, content: current.lines.join("\n") });
  }
  return segments;
}

/**
 * For doc files: prefer S2B2S HEAD (ours), but also include theirs if it adds content
 * not present in ours. Simple strategy: concatenate ours, then append theirs
 * only if theirs contains unique lines/paragraphs not in ours.
 */
function resolveDocConflict(ours, theirs) {
  // If ours is substantially more complete (more lines), keep ours
  // If theirs has extra paragraphs not in ours, append them
  const oursLines = ours.trim().split("\n");
  const theirsLines = theirs.trim().split("\n");

  if (!ours.trim()) return theirs;
  if (!theirs.trim()) return ours;

  // Check if theirs is just a simpler version of ours (ours is superset)
  // → keep ours
  if (oursLines.length >= theirsLines.length) {
    return ours;
  }

  // Theirs has more lines - check if it adds something genuinely new
  // Find theirs lines that don't appear in ours
  const oursSet = new Set(oursLines.map((l) => l.trim()).filter((l) => l));
  const newTheirsLines = theirsLines.filter(
    (l) => l.trim() && !oursSet.has(l.trim()),
  );

  if (newTheirsLines.length > 0) {
    // Return ours + new content from theirs
    return ours + "\n" + newTheirsLines.join("\n");
  }
  return ours;
}

function resolveDocFile(text) {
  const segments = parseConflicts(text);
  let result = "";
  for (const seg of segments) {
    if (seg.type === "common") {
      result += seg.content;
    } else if (seg.type === "ours") {
      // Store ours, wait for theirs
      result += "###OURS###" + seg.content + "###END_OURS###";
    } else if (seg.type === "theirs") {
      // Find last OURS marker and replace with resolved
      const oursMatch = result.match(
        /###OURS###([\s\S]*?)###END_OURS###(?![\s\S]*###OURS###)/,
      );
      if (oursMatch) {
        const oursContent = oursMatch[1];
        const resolved = resolveDocConflict(oursContent, seg.content);
        result = result.replace(
          /###OURS###[\s\S]*?###END_OURS###(?![\s\S]*###OURS###)/,
          resolved,
        );
      } else {
        result += seg.content;
      }
    }
  }
  // Clean up any remaining markers
  result = result.replace(/###OURS###([\s\S]*?)###END_OURS###/g, "$1");
  return result;
}

// ─── JSON conflict resolver ───────────────────────────────────────────────────

/**
 * Deep merge two plain objects: upstream wins on leaf value conflicts,
 * both sides' keys are included (union).
 */
function deepMerge(ours, theirs) {
  if (
    typeof ours !== "object" ||
    typeof theirs !== "object" ||
    ours === null ||
    theirs === null
  ) {
    // Upstream (theirs) wins on scalar conflicts
    return theirs !== undefined ? theirs : ours;
  }
  if (Array.isArray(ours) || Array.isArray(theirs)) {
    return theirs !== undefined ? theirs : ours;
  }

  const result = { ...ours };
  for (const key of Object.keys(theirs)) {
    if (key in result) {
      result[key] = deepMerge(result[key], theirs[key]);
    } else {
      result[key] = theirs[key];
    }
  }
  return result;
}

/**
 * Resolve a JSON file with conflict markers:
 * 1. Strip conflict markers, collecting ours and theirs versions
 * 2. Parse both as JSON
 * 3. Deep merge (union of keys, upstream wins on conflicts)
 */
function resolveJsonFile(text) {
  // Collect all ours and theirs segments
  const segments = parseConflicts(text);

  let oursText = "";
  let theirsText = "";

  for (const seg of segments) {
    if (seg.type === "common") {
      oursText += seg.content;
      theirsText += seg.content;
    } else if (seg.type === "ours") {
      oursText += seg.content;
    } else if (seg.type === "theirs") {
      theirsText += seg.content;
    }
  }

  // Try parsing both sides
  let oursObj, theirsObj;
  try {
    oursObj = JSON.parse(oursText);
  } catch (e) {
    console.warn(`  Warning: Could not parse 'ours' JSON: ${e.message}`);
    // Fall back to theirs
    try {
      return JSON.stringify(JSON.parse(theirsText), null, 2);
    } catch {
      return theirsText;
    }
  }

  try {
    theirsObj = JSON.parse(theirsText);
  } catch (e) {
    console.warn(`  Warning: Could not parse 'theirs' JSON: ${e.message}`);
    return JSON.stringify(oursObj, null, 2);
  }

  const merged = deepMerge(oursObj, theirsObj);
  return JSON.stringify(merged, null, 2);
}

// ─── File processors ──────────────────────────────────────────────────────────

function processFile(filePath, type) {
  const text = readFileSync(filePath, "utf8");
  if (!text.includes("<<<<<<<")) {
    console.log(`  [SKIP] No conflicts: ${filePath}`);
    return false;
  }

  let resolved;
  if (type === "json") {
    resolved = resolveJsonFile(text);
  } else {
    resolved = resolveDocFile(text);
  }

  // Write with CRLF on Windows to match existing files
  const withCRLF = resolved.replace(/\r\n/g, "\n").replace(/\n/g, "\r\n");
  writeFileSync(filePath, withCRLF, "utf8");
  return true;
}

// ─── Main ─────────────────────────────────────────────────────────────────────

const docFiles = ["AGENTS.md", "BUILD.md", "README.md"];
const localeRoot = join(ROOT, "src", "i18n", "locales");
const localeDirs = [
  "ar",
  "bg",
  "cs",
  "de",
  "en",
  "es",
  "fr",
  "he",
  "it",
  "ja",
  "ko",
  "pl",
  "pt",
  "ru",
  "sv",
  "tr",
  "uk",
  "vi",
  "zh",
  "zh-TW",
];

console.log("=== Resolving doc files ===");
const stagedFiles = [];

for (const doc of docFiles) {
  const fp = join(ROOT, doc);
  console.log(`Processing ${doc}...`);
  const changed = processFile(fp, "doc");
  if (changed) {
    stagedFiles.push(fp);
    console.log(`  [OK] Resolved: ${doc}`);
  }
}

console.log("\n=== Resolving i18n locale files ===");
for (const locale of localeDirs) {
  const fp = join(localeRoot, locale, "translation.json");
  console.log(`Processing ${locale}/translation.json...`);
  const changed = processFile(fp, "json");
  if (changed) {
    stagedFiles.push(fp);
    console.log(`  [OK] Resolved: ${locale}/translation.json`);
  }
}

console.log("\n=== Staging resolved files ===");
if (stagedFiles.length > 0) {
  const fileList = stagedFiles.map((f) => `"${f}"`).join(" ");
  try {
    execSync(`git add ${fileList}`, { cwd: ROOT, stdio: "pipe" });
    console.log(`Staged ${stagedFiles.length} files.`);
  } catch (e) {
    console.error("git add failed:", e.message);
    // Try staging one by one
    for (const f of stagedFiles) {
      try {
        execSync(`git add "${f}"`, { cwd: ROOT, stdio: "pipe" });
        console.log(`  Staged: ${f}`);
      } catch (e2) {
        console.error(`  Failed to stage ${f}: ${e2.message}`);
      }
    }
  }
}

// Verify no conflict markers remain
console.log("\n=== Verifying no conflict markers remain ===");
let allClean = true;
for (const fp of stagedFiles) {
  const content = readFileSync(fp, "utf8");
  if (content.includes("<<<<<<<")) {
    console.error(`  [ERROR] Conflict markers still present in: ${fp}`);
    allClean = false;
  } else {
    console.log(`  [OK] Clean: ${fp}`);
  }
}

if (allClean) {
  console.log("\n✅ All files resolved and staged successfully!");
} else {
  console.error(
    "\n❌ Some files still have conflict markers — manual review needed.",
  );
  process.exit(1);
}
