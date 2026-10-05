/**
 * scripts/track-aivorelay.ts
 *
 * Scans the AIVORelay cousin repository, extracts all commits ahead of upstream Handy main,
 * categorizes them, and generates docs/COUSIN_AIVORELAY_TRACKER.md.
 *
 * Maintainer tooling, and the reason it is not in any gate: the sibling
 * checkout path below is hardcoded, so the script runs on one machine only and
 * exits 1 anywhere else. Nothing in the app or the build depends on its output.
 *
 *   bun run track:aivorelay
 */

import { execSync } from "child_process";
import * as fs from "fs";
import * as path from "path";

const AIVORELAY_REPO_PATH =
  "C:/Users/Z/Downloads/PROJECTS/STT_BRAIN_TTS/AIVORelay";
const OUTPUT_DOC_PATH = path.resolve(
  __dirname,
  "../docs/COUSIN_AIVORELAY_TRACKER.md",
);
const GITHUB_REPO_URL = "https://github.com/MaxITService/AIVORelay";

if (!fs.existsSync(AIVORELAY_REPO_PATH)) {
  console.error(
    `Error: AIVORelay repository not found at: ${AIVORELAY_REPO_PATH}`,
  );
  process.exit(1);
}

console.log(`Connecting to AIVORelay repository at ${AIVORELAY_REPO_PATH}...`);

// Ensure upstream-main reference exists in AIVORelay
try {
  execSync("git rev-parse --verify upstream-main", {
    cwd: AIVORELAY_REPO_PATH,
    stdio: "ignore",
  });
} catch {
  console.log("Setting up upstream reference in AIVORelay...");
  try {
    execSync(
      "git fetch https://github.com/cjpais/Handy.git main:upstream-main",
      {
        cwd: AIVORELAY_REPO_PATH,
        stdio: "inherit",
      },
    );
  } catch (err) {
    console.warn(
      "Could not fetch remote upstream, using merge-base fallback...",
    );
  }
}

// Fetch all commits ahead of upstream-main
const format = "%h\t%H\t%ad\t%an\t%s";
const rawLog = execSync(
  `git log upstream-main..main --pretty=format:"${format}" --date=short`,
  {
    cwd: AIVORELAY_REPO_PATH,
    maxBuffer: 50 * 1024 * 1024,
    encoding: "utf-8",
  },
);

interface Commit {
  shortHash: string;
  hash: string;
  date: string;
  author: string;
  subject: string;
  category: CategoryKey;
}

type CategoryKey =
  | "tts"
  | "profiles"
  | "text_processing"
  | "browser_connector"
  | "live_audio_monitor"
  | "cloud_stt"
  | "voice_commands"
  | "hardware_audio"
  | "overlay_ui"
  | "system_stability"
  | "maintenance_releases"
  | "core_features";

interface CategoryMeta {
  title: string;
  description: string;
  icon: string;
}

const CATEGORIES: Record<CategoryKey, CategoryMeta> = {
  tts: {
    title: "Text-to-Speech (TTS) Engine & Voice Workflows",
    description:
      "Kokoro, Qwen, Edge Read Aloud, Windows WinRT, Cloud TTS, Voice Gallery, MP3/Opus file conversion, and Listen Later queue.",
    icon: "🔊",
  },
  profiles: {
    title: "Transcription Profiles & Application-Aware Routing",
    description:
      "Per-executable/title matching (code.exe, Teams, etc.), per-profile prompts/languages, and instant profile cycling.",
    icon: "🎚️",
  },
  text_processing: {
    title: "Smart Decapitalize, Text Replacement & Formatting",
    description:
      "Passive Backspace listener to decapitalize resumed speech, regex substitution, fuzzy n-gram custom words, filler/stutter removal.",
    icon: "✏️",
  },
  browser_connector: {
    title: "Browser Connector & Screen Region Capture",
    description:
      "Local HTTP bridge (port 38243) + Chrome/Edge extension to relay voice, text selections, and multi-monitor screenshots to ChatGPT/Claude.",
    icon: "📤",
  },
  live_audio_monitor: {
    title: "Live Sound Transcription & System Audio Loopback",
    description:
      "WASAPI speaker loopback capture to transcribe PC audio, diarized speaker segments, and real-time floating preview windows.",
    icon: "📺",
  },
  cloud_stt: {
    title: "Cloud & Remote STT Providers",
    description:
      "Gemini Live / Flash STT, Soniox real-time streaming, Deepgram Nova-2, Groq, and custom OpenAI-compatible endpoints.",
    icon: "☁️",
  },
  voice_commands: {
    title: "Voice Command Center",
    description:
      "Speech-triggered PowerShell execution, LLM on-the-fly script synthesis, and safety confirmation modals.",
    icon: "🗣️",
  },
  hardware_audio: {
    title: "Audio Capture & Hardware Integration",
    description:
      "Automatic preferred microphone switching (e.g. Remote Audio / USB headset), audio feedback chimes, and release-tail buffering.",
    icon: "🎙️",
  },
  overlay_ui: {
    title: "Recording Overlay & Visualizer Enhancements",
    description:
      "Shareable v4 style codes, user presets, icon frames (coins, orbs, capsules), 3D perspective bar layers, and error dismissals.",
    icon: "🎨",
  },
  system_stability: {
    title: "System Integration, WebView2 Recovery & Autostart",
    description:
      "WebView2 ProcessFailed crash recovery & window reconstruction, Task Scheduler admin autostart, tray blinking, and rdev key listener.",
    icon: "🛡️",
  },
  core_features: {
    title: "Core STT, History & General Enhancements",
    description:
      "Model metadata, prompt limits, history player exclusivity, subtitle generation (SRT/VTT), and general dictation flow.",
    icon: "⚡",
  },
  maintenance_releases: {
    title: "Releases, Microsoft Store & Upstream Sync",
    description:
      "Version bumps, store propagation sync logs, upstream intakes from cjpais/Handy, documentation, and chores.",
    icon: "📦",
  },
};

function categorizeCommit(subject: string): CategoryKey {
  const s = subject.toLowerCase();

  // TTS
  if (
    s.includes("tts") ||
    s.includes("kokoro") ||
    s.includes("qwen") ||
    s.includes("voice gallery") ||
    s.includes("listen later") ||
    s.includes("read aloud") ||
    s.includes("edge-tts") ||
    s.includes("edge tts") ||
    s.includes("mp3") ||
    s.includes("opus") ||
    (s.includes("text-to-speech") && !s.includes("speech-to-text"))
  ) {
    return "tts";
  }

  // Profiles & App-aware
  if (
    s.includes("profile") ||
    s.includes("active app") ||
    s.includes("active_app") ||
    s.includes("app rule") ||
    s.includes("application-aware") ||
    s.includes("cycle profile")
  ) {
    return "profiles";
  }

  // Text Processing & Decapitalize
  if (
    s.includes("decapitalize") ||
    s.includes("backspace") ||
    s.includes("text replace") ||
    s.includes("text-replace") ||
    s.includes("fuzzy") ||
    s.includes("custom word") ||
    s.includes("filler") ||
    s.includes("stutter") ||
    s.includes("clean-up")
  ) {
    return "text_processing";
  }

  // Browser Connector & Region Capture
  if (
    s.includes("connector") ||
    s.includes("browser") ||
    s.includes("relay") ||
    s.includes("region capture") ||
    s.includes("region-capture") ||
    s.includes("screenshot") ||
    s.includes("chatgpt") ||
    s.includes("claude")
  ) {
    return "browser_connector";
  }

  // Live Sound / Audio Loopback
  if (
    s.includes("live sound") ||
    s.includes("live monitor") ||
    s.includes("loopback") ||
    s.includes("diariz") ||
    s.includes("speaker") ||
    s.includes("live preview") ||
    s.includes("preview window")
  ) {
    return "live_audio_monitor";
  }

  // Cloud STT
  if (
    s.includes("gemini") ||
    s.includes("soniox") ||
    s.includes("deepgram") ||
    s.includes("groq") ||
    s.includes("remote stt") ||
    s.includes("remote-stt") ||
    s.includes("cloud stt")
  ) {
    return "cloud_stt";
  }

  // Voice Commands
  if (
    s.includes("voice command") ||
    s.includes("powershell") ||
    s.includes("command center") ||
    s.includes("command-confirm")
  ) {
    return "voice_commands";
  }

  // Hardware Audio & Capture
  if (
    s.includes("mic") ||
    s.includes("microphone") ||
    s.includes("audio feedback") ||
    s.includes("audio cue") ||
    s.includes("buffer") ||
    s.includes("recorder") ||
    s.includes("cpal")
  ) {
    return "hardware_audio";
  }

  // Overlay & UI
  if (
    s.includes("overlay") ||
    s.includes("preset") ||
    s.includes("3d") ||
    s.includes("bar") ||
    s.includes("hud") ||
    s.includes("theme") ||
    s.includes("icon frame")
  ) {
    return "overlay_ui";
  }

  // System Stability & Integration
  if (
    s.includes("webview") ||
    s.includes("crash") ||
    s.includes("tray") ||
    s.includes("autostart") ||
    s.includes("admin") ||
    s.includes("shortcut") ||
    s.includes("rdev") ||
    s.includes("handykeys") ||
    s.includes("key_listener")
  ) {
    return "system_stability";
  }

  // Maintenance & Releases
  if (
    s.includes("sync") ||
    s.includes("store") ||
    s.includes("bump version") ||
    s.includes("release") ||
    s.includes("chore:") ||
    s.includes("docs:") ||
    s.includes("docs(sync)") ||
    s.includes("merge branch") ||
    s.includes("merge remote") ||
    s.includes("upstream")
  ) {
    return "maintenance_releases";
  }

  return "core_features";
}

const lines = rawLog.trim().split("\n").filter(Boolean);
const commits: Commit[] = lines.map((line) => {
  const [shortHash, hash, date, author, ...rest] = line.split("\t");
  const subject = rest.join("\t").trim();
  return {
    shortHash,
    hash,
    date,
    author,
    subject,
    category: categorizeCommit(subject),
  };
});

const totalCommits = commits.length;
const categoryCounts: Record<CategoryKey, number> = {} as any;
for (const key of Object.keys(CATEGORIES) as CategoryKey[]) {
  categoryCounts[key] = 0;
}
for (const c of commits) {
  categoryCounts[c.category] = (categoryCounts[c.category] || 0) + 1;
}

const newestDate = commits[0]?.date || "Unknown";
const oldestDate = commits[commits.length - 1]?.date || "Unknown";

// Generate Markdown
let md = `# Cousin Repository Tracker: AIVORelay (${totalCommits} Commits Ahead)

> **Repository**: [MaxITService/AIVORelay](${GITHUB_REPO_URL})  
> **Upstream Foundation**: [cjpais/Handy](https://github.com/cjpais/Handy)  
> **Compare Diff**: [cjpais/Handy:main...MaxITService:AIVORelay:main](https://github.com/cjpais/Handy/compare/main...MaxITService:AIVORelay:main)  
> **Span**: ${oldestDate} to ${newestDate} (${totalCommits} commits)  
> **Last Generated**: ${new Date().toISOString().split("T")[0]}  
> **Update Command**: \`bun run scripts/track-aivorelay.ts\`

---

## 📌 Executive Overview

**AIVORelay** is the sister/cousin fork of Handy created by Maxim Fomin (\`MaxITService\`). While our fork (**ZER0**) focuses deeply on **extreme real-time performance, pure-Rust DSP/VAD, multi-STT consensus inference, dual-GPU concurrency (CUDA + Vulkan), and minimal terminal design**, AIVORelay focused on **comprehensive feature expansion, cloud relays, text-to-speech (TTS), and deep Windows desktop utility integration**.

Both apps share the same upstream roots and solve real user problems in complementary ways. This document tracks all **${totalCommits} commits** ahead of upstream to preserve knowledge, draw architectural inspiration, and easily follow future updates.

---

## 📊 Functional Distribution

| Icon | Functional Area | Commits | Description |
| :---: | :--- | :---: | :--- |
`;

for (const [key, meta] of Object.entries(CATEGORIES)) {
  const count = categoryCounts[key as CategoryKey];
  const pct = ((count / totalCommits) * 100).toFixed(1);
  md += `| ${meta.icon} | **${meta.title}** | **${count}** (${pct}%) | ${meta.description} |\n`;
}

md += `
---

## 💡 Top Architectural Inspirations for ZER0

Here are the highest-value capabilities implemented in AIVORelay that directly inform ZER0's roadmap:

### 1. Unified Text-to-Speech (TTS) Platform (Kokoro + Edge Read Aloud + WinRT)
* **What AIVORelay built**:
  - \`src-tauri/src/managers/edge_tts.rs\`: Zero-cost, high-quality Microsoft Edge Read Aloud client using native WebSockets (\`kothok-edge-tts\`). No API key, zero friction.
  - \`src-tauri/src/managers/local_kokoro.rs\`: Local, offline Kokoro-82M int8 runtime via \`sherpa-onnx\`.
  - \`src-tauri/src/managers/windows_tts.rs\`: WinRT / SAPI installed system voices with apartment isolation.
  - Batch document converter (\`.txt\` / \`.md\` to audio files) with watch folders.
  - Interactive "Speak Selected Text" global hotkey + dedicated playback HUD overlay.
* **ZER0 Opportunity**: Our \`PRODUCT.md\` explicitly specifies ZER0 as the base for a future **unified STT + TTS + local Brain platform**. AIVORelay provides an exact, working blueprint.

### 2. Application-Aware Transcription Profiles
* **What AIVORelay built**:
  - \`src-tauri/src/active_app.rs\`: Fast Win32 API calls (\`GetForegroundWindow\`, \`GetWindowThreadProcessId\`, \`QueryFullProcessImageNameW\`) evaluated **strictly upon hotkey press** (zero idle polling).
  - Matches executable names (\`code.exe\`, \`slack.exe\`), window titles, or paths.
  - Dynamically routes transcription to custom LLM prompts, languages, or models.
* **ZER0 Opportunity**: Dictating code into Cursor/VSCode versus dictating chat messages in Discord requires different prompts/formats. Bringing this into ZER0 adds immense power with zero latency penalty.

### 3. Smart Decapitalize After Manual Edit (Backspace Monitor)
* **What AIVORelay built**:
  - \`src-tauri/src/text_replacement_decapitalize.rs\`: Passive backspace listener. If a user dictates text, spots a typo, hits backspace to edit, and continues speaking within a timeout window, the next phrase automatically decapitalizes its first letter so the sentence flows naturally!
* **ZER0 Opportunity**: Solves a universal frustration in continuous voice dictation without impacting performance.

### 4. WebView2 Crash Resilience & Window Reconstruction
* **What AIVORelay built**:
  - \`src-tauri/src/webview_recovery.rs\`: Listens to WebView2 \`ICoreWebView2ProcessFailedEventArgs\`. When gaming hooks (e.g. RivaTuner \`RTSSHooks64.dll\`) crash the webview process, AIVORelay catches it and reconstructs the windows instead of dying or showing a permanent black/white box.
* **ZER0 Opportunity**: Makes Windows desktop stability rock-solid.

### 5. Browser Connector (AI Relay to ChatGPT/Claude) & Screen Capture
* **What AIVORelay built**:
  - \`src-tauri/src/region_capture.rs\`: Multi-monitor screen region capture overlay.
  - Local HTTP bridge (\`127.0.0.1:38243\`) connecting to a dedicated Chrome extension.
  - Push-to-talk sends voice + selected text + screenshot directly into ChatGPT/Claude web interfaces.

---

## 🗂️ Categorized Commit Index

`;

for (const [key, meta] of Object.entries(CATEGORIES)) {
  const catCommits = commits.filter((c) => c.category === key);
  md += `### ${meta.icon} ${meta.title} (${catCommits.length} commits)\n\n`;
  md += `> ${meta.description}\n\n`;
  md += `| Hash | Date | Author | Subject |\n`;
  md += `| :---: | :---: | :---: | :--- |\n`;
  for (const c of catCommits) {
    const link = `[\`${c.shortHash}\`](${GITHUB_REPO_URL}/commit/${c.hash})`;
    md += `| ${link} | \`${c.date}\` | ${c.author} | ${c.subject.replace(/\|/g, "\\|")} |\n`;
  }
  md += `\n`;
}

md += `
---

## 📜 Complete Chronological Commit Log (${totalCommits} Commits)

_Ordered newest to oldest._

| # | Hash | Date | Author | Category | Subject |
| :---: | :---: | :---: | :---: | :---: | :--- |
`;

commits.forEach((c, index) => {
  const num = totalCommits - index;
  const catMeta = CATEGORIES[c.category];
  const link = `[\`${c.shortHash}\`](${GITHUB_REPO_URL}/commit/${c.hash})`;
  md += `| ${num} | ${link} | \`${c.date}\` | ${c.author} | ${catMeta.icon} ${c.category} | ${c.subject.replace(/\|/g, "\\|")} |\n`;
});

md += `
---

## 🔄 How to Keep This Document Updated in the Future

When new commits are pushed to AIVORelay:

\`\`\`bash
# 1. In AIVORelay directory, pull the latest changes:
cd C:\\Users\\Z\\Downloads\\PROJECTS\\STT_BRAIN_TTS\\AIVORelay
git pull origin main

# 2. In ZER0 directory, run the tracking script:
cd C:\\Users\\Z\\Downloads\\PROJECTS\\Handy_V2
bun run scripts/track-aivorelay.ts
\`\`\`

The script will automatically re-parse the git history, re-classify commits, update the statistics, and regenerate this tracker.
`;

fs.writeFileSync(OUTPUT_DOC_PATH, md, "utf-8");
console.log(`Successfully generated tracker at: ${OUTPUT_DOC_PATH}`);
console.log(`Document contains all ${totalCommits} commits.`);
