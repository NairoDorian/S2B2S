/**
 * Fail the build when a stale product name is still in the tree.
 *
 *   bun run check:identity
 *
 * The project has been renamed once and will be renamed again. `app-meta.ts`
 * makes the *values* single-sourced, but it cannot stop a comment, a log line,
 * a doc, a workflow or a new component from spelling the name out by hand —
 * and those are exactly the places a rename misses, because nothing compiles
 * against a comment.
 *
 * So this walks every tracked file and reports two kinds of leak:
 *
 * 1. **The old name.** Any occurrence of `APP.legacy.name` (as a word, any
 *    case) outside an exemption below. Renaming again means moving the current
 *    name into `legacy` and updating this list — the code itself needs no
 *    edit, which is the point.
 * 2. **A hardcoded current name in code.** `src/**` and `src-tauri/src/**`
 *    must read the name from `appIdentity.ts` / `app_identity.rs`, never
 *    spell it. Prose — docs, comments in scripts, release notes, locales —
 *    is allowed to name the product; it is the *code* that has to survive a
 *    rename untouched.
 *
 * Exemptions are per file and carry their reason, so the next person can tell
 * "this is somebody else's identifier" from "this is our own stale string".
 * Nothing is exempted repository-wide: a broad ignore is how a leak survives.
 */

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { extname } from "node:path";

import { APP, GENERATED_PATHS } from "./app-meta";

const LEGACY = APP.legacy;

/** Binary formats and generated output are not read as text. */
const SKIP_EXT = new Set([
  ".png",
  ".ico",
  ".icns",
  ".jpg",
  ".jpeg",
  ".gif",
  ".webp",
  ".wav",
  ".mp3",
  ".ogg",
  ".gguf",
  ".bin",
  ".pdf",
  ".zip",
  ".dll",
  ".exe",
  ".so",
  ".dylib",
  ".pdb",
  ".lockb",
]);

/**
 * Files whose only job is to hold the legacy spellings, plus the surfaces that
 * legitimately name the product.
 *
 * Each entry needs an `allow` regexp: a *line* in that file passes only when it
 * matches. A file-level exemption with no reason to be narrow is the first step
 * to an exemption that hides a real leak.
 */
interface Exemption {
  /** Repo-relative path, or a prefix ending in `/`. */
  file: string;
  /** A line matching this is a genuine, unreplaceable occurrence. */
  allow: RegExp;
  reason: string;
}

const EXEMPTIONS: Exemption[] = [
  // --- the machinery itself -------------------------------------------------
  {
    file: "scripts/app-meta.ts",
    allow: new RegExp(LEGACY.name, "i"),
    reason:
      "Defines the legacy record. This is the one file a rename edits by hand.",
  },
  {
    file: "scripts/check-identity.ts",
    allow: new RegExp(LEGACY.name, "i"),
    reason: "This file: the patterns and reasons it matches against.",
  },
  {
    file: "src-tauri/nsis/installer.nsi",
    allow: new RegExp(LEGACY.portableMarker, "i"),
    reason:
      "Reader accepts both the current and the 0.9.x portable marker, so a portable install made before the rename keeps working.",
  },
  {
    file: "src-tauri/src/settings.rs",
    allow: /handy_keys|keyboard_implementation|wire value/,
    reason:
      'The `"handy_keys"` serde value is a frozen wire value: it is already in every user\'s settings_store.json, and a new spelling would drop the setting.',
  },
  {
    file: "src-tauri/src/settings.rs",
    allow: /handy-keys/,
    reason:
      "Names the upstream keyboard crate on the enum variant built from it.",
  },
  {
    file: "src-tauri/src/settings.rs",
    allow: /handy-computer\//,
    reason:
      "Model ids in the catalog and in the migration table, carrying the external HF org. Renaming them would break every download.",
  },
  {
    file: "src/bindings.ts",
    allow: /handy[_-]keys/,
    reason:
      'Generated from the Rust enum, so it carries both spellings the settings.rs exemptions above already carry: the frozen `"handy_keys"` wire value, and the sentence naming the upstream `handy-keys` crate.',
  },

  // --- someone else's identifiers -------------------------------------------
  {
    file: "src-tauri/src/secure_input.rs",
    allow: /handy[_-]keys/,
    reason:
      "`handy-keys` is the upstream macOS keyboard crate this file binds to.",
  },
  {
    file: "src-tauri/src/shortcut/native_keys.rs",
    allow: /handy-keys/,
    reason: "The same upstream crate, in this backend's module doc.",
  },
  {
    file: "src-tauri/src/shortcut/tauri_impl.rs",
    allow: /handy-keys/,
    reason: "Names the same upstream crate in a log prefix.",
  },
  {
    file: "src-tauri/src/shortcut/handler.rs",
    allow: /handy-keys/,
    reason: "Names the same upstream crate in a doc comment.",
  },
  {
    file: "src-tauri/Cargo.toml",
    allow: /handy-keys/,
    reason: "The dependency line for the crate above.",
  },
  {
    file: "src-tauri/Cargo.lock",
    allow: /handy-keys/,
    reason: "Generated lockfile entry for the crate above.",
  },
  {
    file: "src-tauri/src/catalog/",
    allow: /handy-computer|blob\.handy\.computer/,
    reason:
      "Hugging Face org and download host that actually serve the models. Renaming these breaks every download; they are an external address, not our name.",
  },
  {
    file: "src-tauri/src/managers/model.rs",
    allow: /handy-computer|blob\.handy\.computer/,
    reason:
      "Same external model host, in the download fallback URLs and their test.",
  },
  {
    file: "scripts/gen_catalog.py",
    allow: /handy-computer|blob\.handy\.computer/,
    reason: "Generates the catalog; the org and host above are its inputs.",
  },
  {
    file: "src/lib/modelId.ts",
    allow: /handy-computer/,
    reason:
      "The one place the external HF org is stripped off a model id for display. The org is not ours — and the pattern stops at the org name, because the source spells the separator as an escaped slash (`/^handy-computer\\//`) that a pattern including it would miss.",
  },
  {
    file: "scripts/tauri-runner.ts",
    allow: /"handy", "transcribe_cpp_cache"/,
    reason:
      "The CPU lane's private cache root. `get_cache_root()` in transcribe.cpp's own build script (`bindings/rust/sys/build.rs`, a separate repo) puts every persistent build artifact under `<LOCALAPPDATA>/handy/transcribe_cpp_cache/`, and that directory already exists on every developer's machine, holding the GPU caches. The CPU cache has to be a *sibling* of those, not a tree of its own, which is the whole reason it is named here: this path is not ours to rename.",
  },

  // --- attribution, which must survive the rename ---------------------------
  {
    file: "LICENSE",
    allow: /[Hh]andy/,
    reason:
      "The upstream MIT notice is a legal requirement and is never edited.",
  },
  {
    file: "README.md",
    allow: /[Hh]andy|NairoDorian/,
    reason:
      "Attribution only: the fork's origin sentence under `Why ZER0?`, the upstream project named in `License` and `Acknowledgments`, and the handful of upstream issue references kept as labelled history. Every path, flag, binary name and app-data directory in this file is ZER0's own.",
  },
  {
    file: "AGENTS.md",
    allow: /[Hh]andy/,
    reason: "Records what this fork came from, for the next contributor.",
  },
  {
    file: "src/i18n/locales/",
    allow: /began as a fork of|"handy"/,
    reason:
      "The about page's upstream acknowledgement — a credit under the same MIT licence the code is distributed with, not a self-reference. Its own name is in the `handy` key, and the sentence names it. Kept in English in every locale, like the sibling credits above it.",
  },
  {
    file: "src/components/settings/about/AboutSettings.tsx",
    allow: /acknowledgments\.handy/,
    reason: "Renders the upstream acknowledgement above.",
  },
  {
    file: "src/content/release-notes/README.md",
    allow: /[Hh]andy|cjpais/,
    reason:
      "States what this project is a fork of and how the notes are written. Attribution, like the historical notes below it — and listed per file so a *new* note still cannot name the old product as itself.",
  },
  {
    file: "src/content/release-notes/0.9.0.md",
    allow: /[Hh]andy|cjpais/,
    reason: "A historical upstream release note; it describes that release.",
  },
  {
    file: "src/content/release-notes/0.9.6.md",
    allow: /[Hh]andy/,
    reason: "Records which upstream release this fork's 0.9.6 tracked.",
  },
  {
    file: "src/components/whats-new/markdown.test.ts",
    allow: /handy-computer\/transcribe\.cpp/,
    reason:
      "Asserts the parser against the real 0.9.0 release note, whose transcribe.cpp link is already exempted above — the test pins the exemption's content, it does not name the product.",
  },
  {
    file: "CHANGELOG.md",
    allow: /[Hh]andy/,
    reason: "Historical entries; rewritten, not erased.",
  },
  {
    file: "docs/",
    allow: /[Hh]andy|cjpais/,
    reason:
      "Design docs and plans that record the fork's history against upstream.",
  },
  {
    file: "CONTRIBUTING.md",
    allow: /[Hh]andy/,
    reason: "Contributor guide; names upstream where the lineage matters.",
  },
  {
    file: "BUILD.md",
    allow: /[Hh]andy/,
    reason:
      "The fork note names upstream as the source, and the `Handy_Multi_STT` branch is named. Every build path, bundle name and binary in this file is ZER0's own.",
  },
  {
    file: ".github/PULL_REQUEST_TEMPLATE.md",
    allow: /[Hh]andy|cjpais/,
    reason: "Same, for the PR template's upstream-links checklist.",
  },
  {
    file: ".github/workflows/nix-check.yml",
    allow: /handy-computer/,
    reason:
      "The Cachix *cache* name — an external address, not our name. It is declared on cachix.io and renamed there, so it cannot be derived from app-meta.ts; the workflows are otherwise checked, which is why this is one narrow entry and not a folder-wide ignore.",
  },
  {
    file: "bun.lock",
    allow: /[Hh]andy/,
    reason:
      'Package-manager lockfile: its workspace entry still carries the pre-rename package name ("handy-app") until the next `bun install` rewrites it.',
  },
  {
    file: ".nix/",
    allow: /[Hh]andy/,
    reason: "Nix helper generated against the upstream package set.",
  },
];

/** Paths whose *text* is prose and may legitimately spell the current name. */
const PROSE_EXT = new Set([".md", ".json", ".yml", ".yaml", ".nsi", ".nix"]);

const CODE_ROOTS = ["src/", "src-tauri/src/"];

/**
 * Exemption patterns are matched case-insensitively and without the `g` flag.
 *
 * Case matters here in a way it does not elsewhere: the same name appears as
 * `Handy`, `handy` and `HANDY` in the files this has to exempt, and a pattern
 * that catches one spelling but not the others turns a documented exemption
 * into a false leak. Matching case-insensitively is what makes an exemption
 * mean "this file, this identifier" rather than "this exact capitalisation".
 */
function isExempt(path: string, line: string): string | null {
  for (const entry of EXEMPTIONS) {
    const matches = entry.file.endsWith("/")
      ? path.startsWith(entry.file)
      : path === entry.file;
    if (!matches) continue;
    const re = new RegExp(entry.allow.source, "i");
    if (re.test(line)) return entry.reason;
  }
  return null;
}

/** Every tracked file, so `.gitignore` decides what is part of the project. */
function trackedFiles(): string[] {
  const out = execFileSync("git", ["ls-files", "-z"], {
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  return out.split("\0").filter(Boolean);
}

interface Leak {
  path: string;
  line: number;
  text: string;
  why: string;
}

const LEGACY_RE = new RegExp(`\\b${LEGACY.name}\\b`, "i");
const LEGACY_ID_RE = new RegExp(
  `${LEGACY.identifier.replace(/\./g, "\\.")}(?![\\w.])`,
  "i",
);

function scan(): Leak[] {
  const leaks: Leak[] = [];
  const currentRe = new RegExp(`\\b${APP.name}\\b`);

  for (const path of trackedFiles()) {
    if (SKIP_EXT.has(extname(path))) continue;
    // The generated mirrors and the file they are generated from are the
    // definition of these values, not a leak. `meta:check` is their gate: it
    // fails when one drifts from the generator, which is the failure that
    // actually matters — a hand-edit that reintroduces the old name inside a
    // generated file is caught there, because the next `meta:sync` reverts it.
    if (GENERATED_PATHS.has(path)) continue;

    let text: string;
    try {
      text = readFileSync(path, "utf8");
    } catch {
      continue;
    }
    // A NUL byte in the first block means binary; git tracks a few.
    if (text.includes("\0")) continue;

    const checkCurrent =
      CODE_ROOTS.some((root) => path.startsWith(root)) &&
      !PROSE_EXT.has(extname(path));

    const lines = text.split("\n");
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      if (line === undefined) continue;
      let why: string | null = null;
      if (LEGACY_RE.test(line) || LEGACY_ID_RE.test(line)) {
        why = `old name "${LEGACY.name}"`;
      } else if (checkCurrent && currentRe.test(line)) {
        why = `hardcoded "${APP.name}" — read it from appIdentity.ts / app_identity.rs`;
      }
      if (!why) continue;
      // A line that is itself an exemption's `allow` pattern lives in this file.
      if (isExempt(path, line)) continue;
      leaks.push({ path, line: i + 1, text: line.trim(), why });
    }
  }
  return leaks;
}

const leaks = scan();

if (leaks.length === 0) {
  console.log(
    `[identity] clean — no "${LEGACY.name}" outside ${EXEMPTIONS.length} documented exemptions`,
  );
  process.exit(0);
}

console.error(
  `[identity] ${leaks.length} stale reference${leaks.length === 1 ? "" : "s"}:`,
);
let last = "";
for (const leak of leaks) {
  if (leak.path !== last) {
    console.error(`\n  ${leak.path}`);
    last = leak.path;
  }
  console.error(`    ${leak.line}: ${leak.why}`);
  console.error(`      ${leak.text.slice(0, 160)}`);
}
console.error(
  "\nRead the name from appIdentity.ts / app_identity.rs — or, if this is " +
    "somebody else's identifier, add a narrow exemption in " +
    "scripts/check-identity.ts with the reason.",
);
process.exit(1);
