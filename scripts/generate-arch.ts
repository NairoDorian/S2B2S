import fs from "node:fs";
import path from "node:path";
import {
  pack,
  searchFiles,
  generateTreeString,
  loadFileConfig,
  mergeConfigs,
  type PackResult,
} from "repomix";

// 1-line description registry for files in S2B2S.
const fileDescriptions: Record<string, string> = {
  ".gitignore":
    "Git ignore configuration excluding build artifacts, model caches, venv, and OS files.",
  ".prettierrc": "Prettier code formatting configuration.",
  ".prettierignore": "Prettier ignore configuration.",
  "package.json":
    "Node/Bun manifest with scripts and dependencies for React 18/19, Tauri v2, and Tailwind v4.",
  "repomix.config.json": "Repomix packing configuration.",
  "tsconfig.json": "TypeScript compiler options and path aliases.",
  "tsconfig.node.json": "TypeScript config for Node/Vite build scripts.",
  "vite.config.ts":
    "Vite dev server and production bundler configuration tailored for Tauri v2 multi-webview.",
  "index.html": "Main application webview HTML entry point.",
  "README.md": "S2B2S product documentation and feature overview.",
  "BUILD.md":
    "Cross-platform compilation instructions and prerequisites for Windows, macOS, and Linux.",
  "CHANGELOG.md": "Full project release and feature changelog.",
  "CONTRIBUTING.md": "Contributor guide and coding conventions.",
  "AGENTS.md": "AI coding agent instructions and cross-platform mandate.",
  "STATUS.md": "Current subsystem readiness and roadmap status.",
  "src/main.tsx": "React application mount entry point.",
  "src/App.tsx":
    "Main application shell, sidebar routing, error boundary, and global shortcuts.",
  "src/App.css":
    "Application stylesheet integrating Tailwind CSS and theme tokens.",
  "src/bindings.ts":
    "Specta-generated typed IPC bridge connecting frontend to Tauri backend.",
  "src/vite-env.d.ts":
    "Vite environment and __APP_VERSION__ global declarations.",
  "src-tauri/Cargo.toml":
    "Cargo manifest defining Rust dependencies (Tauri v2, cpal, rodio, transcribe-rs, etc.).",
  "src-tauri/tauri.conf.json":
    "Tauri v2 configuration defining multi-window capabilities, updater, and permissions.",
  "src-tauri/build.rs":
    "Rust build script managing C/C++ native dependencies and asset staging.",
  "src-tauri/src/lib.rs":
    "Main backend entry point, manager initialization, and specta command builder.",
  "src-tauri/src/main.rs":
    "Binary entry point launching the Tauri application loop.",
  "src-tauri/src/actions.rs":
    "Shortcut action orchestrators (transcribe, converse, speak selection).",
  "src-tauri/src/overlay.rs":
    "Platform-specific floating recording and status overlay window.",
  "src-tauri/src/settings.rs":
    "Application settings schema, persistence, and migrations.",
  "scripts/version.ts":
    "Single source of truth for the application version constant (APP_VERSION).",
  "scripts/before-commit.ts":
    "Pre-commit pipeline and version synchronization across package.json, Cargo.toml, and tauri.conf.json.",
  "scripts/generate-arch.ts":
    "Repomix pack() API-driven architecture document generator.",
  "scripts/create-icons.ts":
    "Cross-platform icon generator producing PNG, ICO, and ICNS assets.",
};

/** Human-readable byte size (e.g. `1.2 KB`). */
function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

/** Normalize OS-native separators to POSIX. */
function toPosix(p: string): string {
  return p.replaceAll("\\", "/");
}

/**
 * Converts Repomix's 2-space-indented tree into box-drawing characters
 * (├── / └── / │) for a more readable architecture map.
 */
function toBoxDrawingTree(treeString: string): string {
  const entries = treeString.split("\n").map((line) => {
    const trimmed = line.trim();
    return {
      depth: Math.round((line.length - trimmed.length) / 2),
      name: trimmed,
    };
  });

  const isLast = new Array<boolean>(entries.length).fill(true);
  for (let i = entries.length - 2; i >= 0; i--) {
    const current = entries[i];
    if (current === undefined) continue;
    const next = entries.slice(i + 1).find((e) => e.depth <= current.depth);
    if (next !== undefined && next.depth === current.depth) isLast[i] = false;
  }

  const findAncestor = (i: number, depth: number): number => {
    for (let j = i - 1; j >= 0; j--) {
      const entry = entries[j];
      if (entry === undefined) continue;
      if (entry.depth === depth) return j;
      if (entry.depth < depth) break;
    }
    return -1;
  };

  const lines: string[] = [];
  for (let i = 0; i < entries.length; i++) {
    const entry = entries[i];
    if (entry === undefined) continue;
    const { depth, name } = entry;
    const prefix: string[] = [];
    for (let d = 1; d <= depth; d++) {
      const anc = findAncestor(i, d - 1);
      prefix.push(anc >= 0 && !isLast[anc] ? "│   " : "    ");
    }
    lines.push(`${prefix.join("")}${isLast[i] ? "└── " : "├── "}${name}`);
  }
  return lines.join("\n");
}

async function generateArchitectureMarkdown() {
  const rootDir = process.cwd();
  const rootName = path.basename(rootDir);

  const fileConfig = await loadFileConfig(rootDir, null);
  const config = mergeConfigs(rootDir, fileConfig, {});

  config.output.files = false;
  config.output.git.sortByChanges = false;
  config.security.enableSecurityCheck = false;

  const search = await searchFiles(rootDir, config);

  const result: PackResult = await pack([rootDir], config, () => {}, {
    produceOutput: async () => ({ outputForMetrics: "" }),
  });

  const filePaths = [
    ...new Set([
      ...search.filePaths,
      ...result.safeFilePaths,
      ...result.skippedFiles.map((f) => f.path),
    ]),
  ].sort((a, b) => toPosix(a).localeCompare(toPosix(b)));

  const emptyDirs = config.output.includeEmptyDirectories
    ? search.emptyDirPaths
    : [];
  const tree = toBoxDrawingTree(generateTreeString(filePaths, emptyDirs));

  const contentByPath = new Map(
    result.processedFiles.map((f) => [toPosix(f.path), f.content]),
  );
  const tokenCounts = new Map(
    Object.entries(result.fileTokenCounts).map(([k, v]) => [toPosix(k), v]),
  );
  const charCounts = new Map(
    Object.entries(result.fileCharCounts).map(([k, v]) => [toPosix(k), v]),
  );

  const totalChars = [...charCounts.values()].reduce((sum, v) => sum + v, 0);
  const totalTokens = [...tokenCounts.values()].reduce((sum, v) => sum + v, 0);

  const fileRows: string[] = [];
  for (const filePath of filePaths) {
    const posix = toPosix(filePath);
    const absPath = path.join(rootDir, filePath);
    const size = fs.statSync(absPath, { throwIfNoEntry: false })?.size ?? 0;
    const content = contentByPath.get(posix);
    const lines = content !== undefined ? content.split("\n").length : "—";
    const tokens = tokenCounts.get(posix) ?? "—";
    const chars = charCounts.get(posix) ?? "—";
    const desc =
      fileDescriptions[posix] ??
      "Source or resource file for the S2B2S application.";
    fileRows.push(
      `| \`${posix}\` | ${formatBytes(size)} | ${lines} | ${tokens} | ${chars} | ${desc} |`,
    );
  }

  const content = `# S2B2S Architecture Overview

This document provides a single-file summary of the **S2B2S (Speech-to-Brain-to-Speech)** repository architecture. The directory tree and per-file metadata are generated by the [Repomix](https://repomix.com) \`pack()\` API (\`scripts/generate-arch.ts\`).

> [!NOTE]
> This file contains the directory tree and inventory metrics (size, lines, tokens, characters) across the project. Full code contents are omitted to keep the architecture map concise.

---

## 1. Directory Structure

\`\`\`
${rootName}/
${tree}
\`\`\`

---

## 2. File Inventory & Descriptions

Repomix metrics: **${result.totalFiles} files · ${formatBytes(totalChars)} · ${totalTokens.toLocaleString()} tokens** (text files; binary assets are listed without content metrics).

| File Path | Size | Lines | Tokens | Chars | Description |
| :--- | :--- | :--- | :--- | :--- | :--- |
${fileRows.join("\n")}

---

## 3. Technology Stack & Data Flow

- **Frontend Layer**: Built with **React**, **TypeScript**, and **Tailwind CSS v4**, communicating with the backend through Specta-generated typed IPC bindings.
- **Desktop Container**: Powered by **Tauri v2**, providing high-performance native windows, audio streaming, and global shortcuts.
- **Audio & STT Pipeline**: Captures audio with **cpal**, filters speech with **TripleVAD (Silero + RNNoise)**, transcribes with **transcribe-rs** (Parakeet V3, Whisper, Moonshine, SenseVoice, Canary), and normalizes text with **text-processing-rs**.
- **Brain (LLM)**: Streams tokens from local or cloud providers (Ollama, LM Studio, llama.cpp, OpenAI, Anthropic) and splits sentences in real time.
- **TTS Engine**: Synthesizes and plays speech using Piper, Kokoro, Kitten, Pocket, Qwen3, SAPI, OpenAI, ElevenLabs, or Cartesia via **rodio** gapless streaming.
`;

  fs.writeFileSync(path.join(rootDir, "ARCHITECTURE.md"), content, "utf8");
  console.log(
    `✅ ARCHITECTURE.md generated via Repomix pack() API (${result.totalFiles} files, ${totalTokens.toLocaleString()} tokens).`,
  );
}

generateArchitectureMarkdown();
