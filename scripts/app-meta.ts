// scripts/app-meta.ts
//
// THE single source of truth for ZER0's identity and version.
//
// This file is the only place in the repository where the project's name,
// bundle identifier, author, repository slug or version is written by hand.
// Every other spelling of the name — slug, app-data folder, environment-flag
// prefix, portable marker, storage prefix — is computed from the name itself
// by `spellingsOf`, so renaming is: edit `NAME`, run `bun run meta:sync`.
// Everything else is DERIVED, in one of two ways:
//
//   MIRRORS    a value that has to live inside a file we do not generate —
//              `package.json`, `Cargo.toml`, `tauri.conf.json`, `Cargo.lock`,
//              `installer.nsi`, `index.html`, `flake.nix`. Each mirror is a
//              regex anchored on the value, and the script refuses to run
//              unless the anchor matches exactly the expected number of
//              times. A rename that moves one of these lines is a hard error
//              naming the file and the anchor, never a silent no-op.
//
//   GENERATED  whole files written from the constants below, each carrying a
//              "do not edit" header and checked byte-for-byte by `--check`:
//                src-tauri/src/app_identity.rs   every Rust call site
//                src/lib/appIdentity.ts          every frontend call site
//                nix/module.nix                  the NixOS module
//                nix/hm-module.nix               the home-manager module
//
// Why generate instead of using `env!("CARGO_PKG_*")` alone: Cargo exposes the
// crate name, version, description and authors, but not the product name, the
// bundle identifier, the app-data folder, the env-flag prefix or the portable
// marker — and mixing two mechanisms means two places to look. One generated
// module holds all of them, so a call site always reads `app_identity::NAME`
// and never spells "ZER0" itself.
//
// When it runs:
//   bun run meta:sync          write the mirrors and regenerate the modules
//   bun run meta:check         verify both; exit 1 on any drift (CI gate)
//   bun run meta:set <x.y.z>   set the version, then sync
//   bun run meta:bump <level>  major | minor | patch, then sync
//
// `bun run precommit` runs `meta:sync` then `meta:check` as its first two
// steps, and stages exactly the files `identityPaths()` lists afterwards.

import { existsSync, readFileSync, writeFileSync } from "fs";
import { join, resolve } from "path";

const root = resolve(import.meta.dirname, "..");
const TAG = "[meta]";

/** This file, repo-relative — the only hand-edited definition site. */
const META_TS = "scripts/app-meta.ts";

/**
 * Every spelling of a name, from the name alone.
 *
 * A name is one word. The slug, the app-data folder, the environment prefix,
 * the portable marker and the storage prefix are all that same word wearing a
 * different case or a different suffix, so they are computed rather than
 * written out — which is what makes a rename one edit instead of six, and
 * makes the inconsistent states unrepresentable: a name can no longer end up
 * with another name's env prefix.
 *
 * `identifier` is deliberately NOT here. A bundle identifier is the OS-level
 * identity that owns the user's app-data folder, and the folder name is what
 * the OS shows for that identity; deriving it from the name would move every
 * existing install's models, history and settings onto the next rename. Two
 * separate facts, two separate fields.
 */
function spellingsOf(name: string) {
  const slug = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return {
    /** Machine slug: crate name, binary name, `package.json` name. */
    slug,
    /** `<app data>/<name>/` — what the user sees in `%APPDATA%`. */
    dataDirName: name,
    /** Prefix of every environment flag the application reads. */
    envPrefix: `${slug.toUpperCase()}_`,
    /** Magic string written into the `portable` marker beside the executable. */
    portableMarker: `${name} Portable Mode`,
    /** `localStorage` key prefix for view-only UI preferences. */
    storagePrefix: `${slug}.`,
  };
}

/** The product name — the one hand-edited word of the current identity. */
const NAME = "ZER0";

/**
 * The name this project carried before the rename.
 *
 * A rename cannot derive the old name from the new one, so this word is the
 * one hand-edited value of the legacy record; every other spelling of it is
 * derived below, exactly as for the current name.
 */
const LEGACY_NAME = "Handy";

/** Reverse-domain prefix of the pre-rename bundle identifier. */
const LEGACY_PUBLISHER = "com.pais";

/** The pre-rename spellings, derived once so the `legacy` block can spread. */
const LEGACY = spellingsOf(LEGACY_NAME);

/**
 * EVERY hand-written identity value. Change a value here and run
 * `bun run meta:sync`; nothing else in the repository needs touching.
 */
export const APP = {
  /** Product name: window titles, tray tooltip, installer, bundle artifacts. */
  name: NAME,
  ...spellingsOf(NAME),
  /** Bundle identifier — the OS-level application identity. See above. */
  identifier: "com.nairodorian.zer0",
  author: "NairoDorian",
  description: `${NAME} — real-time speech to text`,
  repoOwner: "NairoDorian",
  repoName: "S2B2S",
  version: "0.9.7",
  /**
   * The 0.9.x identity, kept as a record of what was rather than as a setting.
   * Never written anywhere — only READ, so an install made before the rename
   * keeps its models, history, settings, env flags and portable layout. See
   * `app_identity::LEGACY_*`.
   */
  legacy: {
    name: LEGACY_NAME,
    ...LEGACY,
    identifier: `${LEGACY_PUBLISHER}.${LEGACY.slug}`,
  },
} as const;

/** The repository URL, from the slug above. */
export const REPO_URL = `https://github.com/${APP.repoOwner}/${APP.repoName}`;

/** Releases page — the fallback every update path points at. */
export const RELEASES_URL = `${REPO_URL}/releases/latest`;

/** The updater manifest the Tauri updater plugin polls. */
export const UPDATER_ENDPOINT = `${RELEASES_URL}/download/latest.json`;

/** Where the deb/rpm place the app-private transcribe.cpp runtime libraries. */
export const LINUX_LIB_DIR = `/usr/lib/${APP.name}`;

/** `Cargo.toml` `[package] authors` is a TOML array, not a plain string. */
const AUTHORS_TOML = `["${APP.author}"]`;

/** The one line of this file `--set` / `--bump` rewrite. */
const VERSION_RE = /^(\s*version: ")([^"]+)(",$)/m;

/** Release notes the What's New dialog reads, one `<version>.md` per release. */
const RELEASE_NOTES_DIR = "src/content/release-notes";

// ---------------------------------------------------------------------------
// generated modules
// ---------------------------------------------------------------------------

const GENERATED_BANNER = "GENERATED FILE — DO NOT EDIT.";

/**
 * `src-tauri/src/app_identity.rs` — what every Rust call site reads.
 *
 * `NAME` and `SLUG` duplicate `Cargo.toml` on purpose: the generator owns both
 * sides and `--check` proves they agree, so the duplication cannot drift.
 * `LEGACY_*` are read-only compatibility values.
 */
function rustModule(): string {
  const l = APP.legacy;
  return `//! ${GENERATED_BANNER}
//!
//! Source: \`${META_TS}\` — run \`bun run meta:sync\` to regenerate.
//!
//! Do not add a literal of the project's identity anywhere else in this crate.
//! Import from here instead, so a rename stays a one-line change.

/// Product name, for anything the user sees: window titles, tray tooltip,
/// log lines, the about page.
pub const NAME: &str = "${APP.name}";

/// Machine slug: the crate, the binary, \`${APP.slug}.exe\`, the \`package.json\`
/// name. Never shown to the user.
pub const SLUG: &str = "${APP.slug}";

/// Bundle identifier — the OS-level application identity.
pub const IDENTIFIER: &str = "${APP.identifier}";

/// Repository URL, for links out of the app.
pub const REPO_URL: &str = "${REPO_URL}";

/// Releases page — where an update or a portable installer link points.
pub const RELEASES_URL: &str = "${RELEASES_URL}";

/// Who this application says it is on outbound HTTP requests: model downloads,
/// GitHub API calls, LLM provider calls. \`name/version\` is the shape every one
/// of those services expects.
pub const USER_AGENT: &str = "${APP.name}/${APP.version}";

/// The folder under the OS app-data directory that holds models, history and
/// settings. Kept separate from \`IDENTIFIER\` because the folder name is also
/// what the user sees in \`%APPDATA%\` / \`~/.local/share\`.
pub const DATA_DIR_NAME: &str = "${APP.dataDirName}";

/// The folder name a pre-rename install used. Read at startup by the one-shot
/// app-data migration; never written.
pub const LEGACY_DATA_DIR_NAME: &str = "${l.dataDirName}";

/// The bundle identifier a pre-rename install used, named in the migration log
/// and in the portable-install notes.
pub const LEGACY_IDENTIFIER: &str = "${l.identifier}";

/// Prefix of every environment flag this application reads.
pub const ENV_PREFIX: &str = "${APP.envPrefix}";

/// The previous environment-flag prefix. Every flag lookup falls back to it and
/// warns, so an existing shell profile or Nix wrapper keeps working.
pub const LEGACY_ENV_PREFIX: &str = "${l.envPrefix}";

/// Magic string written into the \`portable\` marker beside the executable.
pub const PORTABLE_MARKER: &str = "${APP.portableMarker}";

/// The marker a pre-rename release wrote. Accepted on read, never written
/// unless an older empty marker is being upgraded.
pub const LEGACY_PORTABLE_MARKER: &str = "${l.portableMarker}";

/// Base name of the WAV files a recording is written to before transcription.
pub const RECORDING_BASENAME: &str = "${APP.slug}";

/// File name a single recording's WAV is saved under, in the recordings
/// directory.
///
/// A function rather than a \`format!\` at each call site so the shape of the
/// name lives in one place: the history row a transcription writes must name
/// the same file the recorder wrote.
pub fn recording_file_name(timestamp: i64) -> String {
    format!("{RECORDING_BASENAME}-{timestamp}.wav")
}

/// File name a Multi-STT recording's WAV is saved under. The \`-multi\` infix
/// keeps a parallel recording distinguishable from a plain one in the
/// recordings directory and in history.
pub fn multi_recording_file_name(timestamp: i64) -> String {
    format!("{RECORDING_BASENAME}-multi-{timestamp}.wav")
}
`;
}

/** Prettier's `printWidth` — the generated TypeScript must satisfy it. */
const TS_PRINT_WIDTH = 80;

/**
 * One `export const NAME = "value";` line, wrapped the way Prettier would.
 *
 * The generated module is checked by `prettier --check`, and Prettier never
 * breaks *inside* a string: a line that exceeds the print width is fixed by
 * moving the whole literal onto its own indented line. Emitting that shape here
 * keeps the file stable under both `meta:check` and `format:check` — a URL long
 * enough to overflow is otherwise the one value in the module that makes the two
 * disagree, and neither tool can be told to ignore the other.
 *
 * Values short enough to fit produce exactly what they produced before, so this
 * only ever changes a line that was going to fail the format check.
 */
function tsConst(name: string, value: string): string {
  const oneLine = `export const ${name} = "${value}";`;
  return oneLine.length <= TS_PRINT_WIDTH
    ? oneLine
    : `export const ${name} =\n  "${value}";`;
}

/**
 * `src/lib/appIdentity.ts` — what every frontend call site reads.
 *
 * The storage helpers live here rather than in a separate module so the legacy
 * prefix is used in exactly one place: a preference written by 0.9.6 is read
 * once and re-written under the new prefix.
 */
function tsModule(): string {
  const l = APP.legacy;
  return `// ${GENERATED_BANNER}
//
// Source: \`${META_TS}\` — run \`bun run meta:sync\` to regenerate.
//
// Do not hardcode the application's name, slug, identifier, repository or
// storage prefix anywhere else under \`src/\`. Import from here instead.

/** Product name, for anything the user sees. */
${tsConst("APP_NAME", APP.name)}

/** Machine slug. Never shown to the user. */
${tsConst("APP_SLUG", APP.slug)}

/** \`localStorage\` key prefix for view-only UI preferences. */
${tsConst("STORAGE_PREFIX", APP.storagePrefix)}

/** Machine slug of the pre-rename identity: the basename 0.9.x wrote. */
${tsConst("LEGACY_SLUG", l.slug)}

/** Bundle identifier — the OS-level application identity. */
${tsConst("APP_IDENTIFIER", APP.identifier)}

/** Repository URL, for links out of the app. */
${tsConst("REPO_URL", REPO_URL)}

/** Releases page — where an update or a portable installer link points. */
${tsConst("RELEASES_URL", RELEASES_URL)}

/** The folder under the OS app-data directory that holds models and history. */
${tsConst("DATA_DIR_NAME", APP.dataDirName)}

/** The folder name a pre-rename install used; read by the migration only. */
${tsConst("LEGACY_DATA_DIR_NAME", l.dataDirName)}

/** The bundle identifier a pre-rename install used. */
${tsConst("LEGACY_IDENTIFIER", l.identifier)}

/** Prefix of every environment flag the backend reads. Shown in Settings. */
${tsConst("ENV_PREFIX", APP.envPrefix)}

/** The previous environment-flag prefix, still honoured by the backend. */
${tsConst("LEGACY_ENV_PREFIX", l.envPrefix)}

/** Magic string in the \`portable\` marker beside the executable. */
${tsConst("PORTABLE_MARKER", APP.portableMarker)}

/** The marker a pre-rename release wrote. */
${tsConst("LEGACY_PORTABLE_MARKER", l.portableMarker)}

/** The previous \`localStorage\` prefix, migrated on first read. */
${tsConst("LEGACY_STORAGE_PREFIX", l.storagePrefix)}

/**
 * File-name infix that marks a recording as a Multi-STT one.
 *
 * Mirrors \`app_identity::multi_recording_file_name\`: a history row records
 * the file name the recorder wrote, so the UI has to recognise the same shape
 * the backend produced.
 */
export const MULTI_RECORDING_INFIX = "-multi-";

/**
 * Whether a history row's WAV belongs to a Multi-STT recording.
 *
 * Both basenames are accepted. History is a database of recordings the user
 * still has on disk, and rows written before the rename point at
 * \`\${LEGACY_SLUG}-multi-…\` files that were never moved — judging them by the
 * current basename alone would silently reclassify every one of them as a
 * plain transcription.
 */
export function isMultiRecordingFileName(fileName: string): boolean {
  return (
    fileName.startsWith(APP_SLUG + MULTI_RECORDING_INFIX) ||
    fileName.startsWith(LEGACY_SLUG + MULTI_RECORDING_INFIX)
  );
}

/**
 * Read a UI preference, falling back to the key a pre-rename release wrote.
 *
 * The legacy value is re-written under the current prefix and the old key
 * removed, so the migration happens once per key per browser profile and never
 * runs again. Both the read and every write are guarded: \`localStorage\` throws
 * outright in a locked-down webview, and a preference is never worth a crash.
 *
 * @param key suffix after \`STORAGE_PREFIX\`, e.g. \`"theme"\` or \`"sidebar.width"\`
 */
export function readPref(key: string): string | null {
  try {
    const current = localStorage.getItem(STORAGE_PREFIX + key);
    if (current !== null) return current;
    const legacy = localStorage.getItem(LEGACY_STORAGE_PREFIX + key);
    if (legacy === null) return null;
    localStorage.setItem(STORAGE_PREFIX + key, legacy);
    localStorage.removeItem(LEGACY_STORAGE_PREFIX + key);
    return legacy;
  } catch {
    return null;
  }
}

/** Write a UI preference, ignoring a storage that refuses to accept it. */
export function writePref(key: string, value: string): void {
  try {
    localStorage.setItem(STORAGE_PREFIX + key, value);
  } catch {
    // A preference is a convenience; a webview with storage disabled still runs.
  }
}

/** Remove a UI preference, under both the current and the legacy prefix. */
export function removePref(key: string): void {
  try {
    localStorage.removeItem(STORAGE_PREFIX + key);
    localStorage.removeItem(LEGACY_STORAGE_PREFIX + key);
  } catch {
    // See writePref.
  }
}
`;
}

/**
 * The NixOS module.
 *
 * Generated rather than mirrored because *every* name in it is derived: the
 * option path (`programs.<slug>`), the umask prose and the flake-input
 * example. A user writes the option path in their own `configuration.nix`,
 * so a rename is a breaking change to that file — but it is one the sync
 * makes consistently, in the module and in `flake.nix` together, instead of
 * leaving the two disagreeing.
 *
 * `bin/<slug>` is not cosmetic: `Cargo.toml`'s `[package] name` is a mirror,
 * so the built binary already carries the slug. A stale `bin/` here is an
 * `ExecStart` that cannot exec.
 */
function nixosModule(): string {
  return `# ${GENERATED_BANNER}
#
# Source: \`${META_TS}\` — run \`bun run meta:sync\` to regenerate.
#
# NixOS module for ${APP.name} speech-to-text.
#
# Handles system-level configuration that the package wrapper cannot:
#   - udev rule for /dev/uinput (the handy-keys evdev grab re-injects
#     through virtual input devices; dotool / ydotool typing uses it too)
#
# Note: users must add themselves to the "input" group for evdev hotkey access.
#
# Usage in your flake:
#
#   inputs.${APP.slug}.url = "${REPO_URL}";
#
#   nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
#     modules = [
#       ${APP.slug}.nixosModules.default
#       { programs.${APP.slug}.enable = true; }
#     ];
#   };
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.${APP.slug};
in
{
  options.programs.${APP.slug} = {
    enable = lib.mkEnableOption "${APP.name} offline speech-to-text";

    package = lib.mkOption {
      type = lib.types.package;
      defaultText = lib.literalExpression "${APP.slug}.packages.\\\${system}.${APP.slug}";
      description = "The ${APP.name} package to use.";
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];

    # handy-keys' evdev grab creates virtual input devices via /dev/uinput.
    # Default permissions are crw------- root root — open it to the input group.
    services.udev.extraRules = ''
      KERNEL=="uinput", GROUP="input", MODE="0660"
    '';
  };
}
`;
}

/**
 * The home-manager module — the per-user counterpart of {@link nixosModule},
 * generated for the same reasons. `ExecStart` names the binary by slug.
 */
function homeManagerModule(): string {
  return `# ${GENERATED_BANNER}
#
# Source: \`${META_TS}\` — run \`bun run meta:sync\` to regenerate.
#
# Home-manager module for ${APP.name} speech-to-text.
#
# Provides a systemd user service for autostart.
# Usage: imports = [ ${APP.slug}.homeManagerModules.default ];
#        services.${APP.slug}.enable = true;
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.${APP.slug};
in
{
  options.services.${APP.slug} = {
    enable = lib.mkEnableOption "${APP.name} speech-to-text user service";

    package = lib.mkOption {
      type = lib.types.package;
      defaultText = lib.literalExpression "${APP.slug}.packages.\\\${system}.${APP.slug}";
      description = "The ${APP.name} package to use.";
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.user.services.${APP.slug} = {
      Unit = {
        Description = "${APP.name} speech-to-text";
        After = [ "graphical-session.target" ];
        PartOf = [ "graphical-session.target" ];
      };
      Service = {
        ExecStart = "\${cfg.package}/bin/${APP.slug}";
        Restart = "on-failure";
        RestartSec = 5;
      };
      Install.WantedBy = [ "graphical-session.target" ];
    };
  };
}
`;
}

/** Every generated file, keyed by repo-relative path. */
function generatedFiles(): Map<string, string> {
  return new Map([
    ["src-tauri/src/app_identity.rs", rustModule()],
    ["src/lib/appIdentity.ts", tsModule()],
    ["nix/module.nix", nixosModule()],
    ["nix/hm-module.nix", homeManagerModule()],
  ]);
}

/**
 * The generated files plus the file they are generated from, as a set the
 * identity checker can skip.
 *
 * These spell the product name by definition — they *are* the constants. The
 * gate that guards them is `meta:check`, which fails when one drifts from the
 * generator, so a checker that also flagged their contents would only ever
 * report a false leak.
 */
export const GENERATED_PATHS: ReadonlySet<string> = new Set([
  META_TS,
  ...generatedFiles().keys(),
]);

/**
 * Every file `meta:sync` can write: the mirror files, the lockfile root block
 * and the generated modules.
 *
 * The pre-commit gate stages exactly these after its `meta:sync` step — never
 * `git add -u` over the whole tree, which would sweep every other dirty
 * tracked file into a commit the user staged selectively.
 */
export function identityPaths(): string[] {
  return [
    ...new Set([
      ...mirrors().map((mirror) => mirror.file),
      "src-tauri/Cargo.lock",
      ...generatedFiles().keys(),
    ]),
  ];
}

// ---------------------------------------------------------------------------
// the mirror table
// ---------------------------------------------------------------------------

/**
 * One value that has to live inside a hand-written file.
 *
 * `pattern` captures exactly three groups — `(before)(value)(after)` — and
 * carries no `g` flag. The script counts matches with a forced-global copy,
 * requires exactly `count` of them, requires all of them to agree, and
 * rewrites every one of them.
 *
 * Three groups is a hard contract, checked by {@link assertThreeGroups}: the
 * replacement callback is handed `(whole, before, value, after)` and a pattern
 * with only two groups would silently receive the *match offset* in `after`,
 * splicing a byte count into the file. Failing loudly is the only safe answer,
 * because the damage is otherwise invisible until something reads it back.
 */
interface Mirror {
  /** Repo-relative path, named in every message. */
  file: string;
  /** How the anchor is described when the count is wrong. */
  anchor: string;
  pattern: RegExp;
  /**
   * The value the mirror must hold. Usually a constant, but a version mirror
   * reads the override `--set` passes in, and a lockfile name mirror has to
   * match whatever the Cargo.toml name mirror was just written as.
   */
  value: (version: string, slug: string) => string;
  /** How many matches to expect. Default 1; every match is rewritten. */
  count?: number;
  /**
   * The section the anchor must live inside, for files where the same key
   * appears in more than one place. The regex must match the section header
   * plus its body and capture the body in group 1, with the header inside
   * match 0 — `^\[package\]\n([\s\S]*?)(?=^\[)` and the like.
   *
   * `Cargo.toml` needs it: `name = "…"` occurs in both `[package]` and `[lib]`,
   * and a bare top-level pattern would match whichever came first.
   */
  section?: RegExp;
}

/**
 * `Cargo.toml`'s `[package]` table — header plus body, body captured in
 * group 1. `[profile.dev]` and `[lib]` follow it, so the body ends at the next
 * table header. `[lib] name = "app_lib"` is deliberately NOT scoped here: it is
 * a structural name that never changes with the product's identity.
 */
const PACKAGE_TABLE = /^\[package\]\s*\n([\s\S]*?)(?=^\[)/m;

/**
 * Every mirror, derived from `APP`. A version mirror is listed here too, so
 * one table answers "where does this value live".
 */
function mirrors(): Mirror[] {
  const literal = (value: string) => () => value;
  return [
    // --- package.json ------------------------------------------------------
    {
      file: "package.json",
      anchor: `the top-level "name" key`,
      pattern: /^(\s*"name"\s*:\s*")([^"]+)(")/m,
      value: (_version, slug) => slug,
    },
    {
      file: "package.json",
      anchor: `the top-level "version" key`,
      pattern: /^(\s*"version"\s*:\s*")([^"]+)(")/m,
      value: (version) => version,
    },

    // --- src-tauri/Cargo.toml ---------------------------------------------
    {
      file: "src-tauri/Cargo.toml",
      anchor: `the [package] name`,
      pattern: /^(name\s*=\s*")([^"]+)(")/m,
      value: (_version, slug) => slug,
      section: PACKAGE_TABLE,
    },
    {
      file: "src-tauri/Cargo.toml",
      anchor: `the [package] version`,
      pattern: /^(version\s*=\s*")([^"]+)(")/m,
      value: (version) => version,
      section: PACKAGE_TABLE,
    },
    {
      file: "src-tauri/Cargo.toml",
      anchor: `the [package] description`,
      pattern: /^(description\s*=\s*")([^"]+)(")/m,
      value: literal(APP.description),
      section: PACKAGE_TABLE,
    },
    {
      file: "src-tauri/Cargo.toml",
      anchor: `the [package] authors array`,
      pattern: /^(authors\s*=\s*\[")([^"]+)("\])/m,
      value: literal(APP.author),
      section: PACKAGE_TABLE,
    },
    {
      file: "src-tauri/Cargo.toml",
      anchor: `the [package] default-run`,
      pattern: /^(default-run\s*=\s*")([^"]+)(")/m,
      value: (_version, slug) => slug,
      section: PACKAGE_TABLE,
    },

    // --- src-tauri/tauri.conf.json ----------------------------------------
    {
      file: "src-tauri/tauri.conf.json",
      anchor: `the top-level "productName" key`,
      pattern: /^(\s*"productName"\s*:\s*")([^"]+)(")/m,
      value: literal(APP.name),
    },
    {
      file: "src-tauri/tauri.conf.json",
      anchor: `the top-level "version" key`,
      pattern: /^(\s*"version"\s*:\s*")([^"]+)(")/m,
      value: (version) => version,
    },
    {
      file: "src-tauri/tauri.conf.json",
      anchor: `the top-level "identifier" key`,
      pattern: /^(\s*"identifier"\s*:\s*")([^"]+)(")/m,
      value: literal(APP.identifier),
    },
    {
      file: "src-tauri/tauri.conf.json",
      anchor: `the updater endpoint URL`,
      pattern:
        /^(\s*")(https:\/\/github\.com\/[^"]*\/releases\/latest\/download\/latest\.json)(")/m,
      value: literal(UPDATER_ENDPOINT),
    },
    {
      // The deb and rpm both install the app-private runtime there, and both
      // must keep moving together — hence count 2, rewritten as a pair.
      file: "src-tauri/tauri.conf.json",
      anchor: `the two linux files "/usr/lib/*" keys`,
      pattern: /("\/usr\/lib\/)([^/"]+)(":\s*"transcribe-libs")/,
      value: literal(APP.name),
      count: 2,
    },

    // src-tauri/Cargo.lock is handled by syncLockRoot() below, not here: no
    // fixed regex can name the workspace's own block, because the name is the
    // thing being synced. It is found by shape instead.

    // --- index.html --------------------------------------------------------
    {
      // The document title. The *window* title comes from tauri.conf.json's
      // productName and is set by the OS window, so this one is only what a
      // browser tab or a webview's own chrome shows — but it is the same fact
      // and belongs in the same table.
      file: "index.html",
      anchor: `the <title> element`,
      pattern: /^(\s*<title>)([^<]*)(<\/title>)$/m,
      value: literal(APP.name),
    },

    // --- flake.nix ---------------------------------------------------------
    // The Nix package's own attribute names, `pname` and `mainProgram` are not
    // cosmetic: `pname` names the derivation and `mainProgram` names the binary
    // the wrapper execs, so a value left behind after a rename is a package
    // that builds and then cannot start. Each anchor below is indentation- or
    // context-anchored, because `description` and `--set` both occur more than
    // once in this file with different values.
    {
      // Exactly two leading spaces: `meta.description` further down is
      // indented much further and holds different prose.
      file: "flake.nix",
      anchor: `the flake description (2-space indent)`,
      pattern: /^(  description = ")([^"]*)(";)$/m,
      value: literal(APP.description),
    },
    {
      file: "flake.nix",
      anchor: `the package attribute inside packages.<system>`,
      pattern:
        /^(          )([a-z0-9][a-z0-9-]*)( = pkgs\.rustPlatform\.buildRustPackage \{)$/m,
      value: (_version, slug) => slug,
    },
    {
      file: "flake.nix",
      anchor: `the pname`,
      pattern: /^(            pname = ")([^"]*)(";)$/m,
      value: (_version, slug) => slug,
    },
    {
      file: "flake.nix",
      anchor: `the meta.mainProgram`,
      pattern: /^(              mainProgram = ")([^"]*)(";)$/m,
      value: (_version, slug) => slug,
    },
    {
      file: "flake.nix",
      anchor: `the meta.homepage URL`,
      pattern: /^(              homepage = ")([^"]*)(";)$/m,
      value: literal(REPO_URL),
    },
    {
      // `self.packages.<system>.<name>` occurs three times — the `default`
      // alias and the two module defaults — and all three must agree, so this
      // is a counted mirror rather than three patterns.
      file: "flake.nix",
      anchor: `every self.packages.<system>.<name> reference`,
      pattern: /(self\.packages\.\$\{[^}]+\}\.)([a-z0-9][a-z0-9-]*)(;)/,
      value: (_version, slug) => slug,
      count: 3,
    },
    {
      file: "flake.nix",
      anchor: `the nixosModules package default`,
      pattern:
        /^(          programs\.)([a-z0-9][a-z0-9-]*)(\.package = lib\.mkDefault )/m,
      value: (_version, slug) => slug,
    },
    {
      file: "flake.nix",
      anchor: `the homeManagerModules package default`,
      pattern:
        /^(          services\.)([a-z0-9][a-z0-9-]*)(\.package = lib\.mkDefault )/m,
      value: (_version, slug) => slug,
    },
    {
      // Discriminated by the flag's own suffix, not by indentation: the
      // WEBKIT_DISABLE_DMABUF_RENDERER flag two lines above sits at the very
      // same indent and ends the same way.
      file: "flake.nix",
      anchor: `the updater-disable environment flag`,
      pattern: /^(\s*--set )([A-Z0-9_]*_DISABLE_UPDATER)( 1)$/m,
      value: literal(`${APP.envPrefix}DISABLE_UPDATER`),
    },
    {
      // Anchored on the shellHook that opens just above it — the "Run 'bun run
      // tauri dev'" echo a line below is otherwise indistinguishable.
      //
      // `\s*` rather than `\s*\n\s*`: the mirror reads the file as it is on
      // disk, and a Windows checkout gives these files CRLF endings. A literal
      // `\n` there silently matches nothing. Keeping the whole gap inside the
      // `before` group also means whatever the ending is, it is written back
      // byte for byte.
      file: "flake.nix",
      anchor: `the dev-shell greeting (first line of shellHook)`,
      pattern: /(shellHook = ''\s*echo ")([^"]*)(")/m,
      value: literal(`${APP.name} development environment`),
    },

    // --- src-tauri/nsis/installer.nsi -------------------------------------
    {
      // The installer writes the portable marker; `portable.rs` reads it. A
      // mismatch sends a portable install back to %APPDATA%. Anchored on the
      // literal `$0`-write form NSIS uses for a marker file, which is
      // distinguishable from the `$(localized)` FileWrite a few hundred lines
      // earlier by the opening quote.
      file: "src-tauri/nsis/installer.nsi",
      anchor: `the portable-marker FileWrite line`,
      pattern: /^(\s*FileWrite \$0 ")([A-Za-z][^"$]*)(")/m,
      value: literal(APP.portableMarker),
    },
  ];
}

/** One `[[package]]` block from `Cargo.lock`, as found by `splitLockBlocks`. */
interface LockBlock {
  /** Offset of the block's `[[package]]` header in the file. */
  start: number;
  /** Offset just past the block's last line. */
  end: number;
  name: string;
  version: string;
  /** Whether the block carries a `source = "…"` line. */
  sourced: boolean;
}

/** Split `Cargo.lock` into its `[[package]]` blocks, in file order. */
function splitLockBlocks(text: string): LockBlock[] {
  // Every header offset, plus a virtual one past the end to close the last
  // block. Slicing between consecutive offsets gives each block exactly once.
  const starts = [...text.matchAll(/^\[\[package\]\]$/gm)].map(
    (match) => match.index,
  );
  const bounds = [...starts, text.length];

  const blocks: LockBlock[] = [];
  for (let index = 0; index < starts.length; index += 1) {
    const start = starts[index];
    const end = bounds[index + 1];
    const body = text.slice(start, end);
    const name = body.match(/^name = "([^"]+)"/m)?.[1];
    const version = body.match(/^version = "([^"]+)"/m)?.[1];
    if (name === undefined || version === undefined) {
      // A block without a name or a version is not something to sync; say so
      // rather than silently skipping a block that might be ours.
      throw new Error(
        `${TAG} src-tauri/Cargo.lock: a [[package]] block at offset ${start} ` +
          `has no name or no version. Refusing to sync a lockfile this cannot ` +
          `parse.`,
      );
    }
    blocks.push({
      start,
      end,
      name,
      version,
      sourced: /^source = "/m.test(body),
    });
  }
  return blocks;
}

/**
 * Sync the workspace crate's own `[[package]]` block in `Cargo.lock`.
 *
 * Not a `Mirror`: the block's `name` is the thing being synced, so no pattern
 * can point at it by name. It is found by shape instead — Cargo writes a
 * `source` line for every package it fetched and none for the workspace's own
 * crate, so the sourceless block is ours, and there is exactly one of them.
 *
 * `cargo generate-lockfile` is deliberately not used: it would re-resolve the
 * `transcribe-cpp` git pin, which `scripts/check-transcribe-deps.ts` manages
 * and a stale resolution would silently move.
 *
 * @returns the two values found; throws when there is not exactly one
 * sourceless block.
 */
function syncLockRoot(
  slug: string,
  version: string,
  dryRun: boolean,
): { found: { name: string; version: string } } {
  const file = "src-tauri/Cargo.lock";
  const path = join(root, file);
  const text = readFileSync(path, "utf-8");
  const unsourced = splitLockBlocks(text).filter((block) => !block.sourced);

  if (unsourced.length !== 1) {
    throw new Error(
      `${TAG} ${file}: expected exactly one [[package]] block without a ` +
        `source line (the workspace crate), found ${unsourced.length}. ` +
        `Refusing to guess which one is ours.`,
    );
  }

  const [block] = unsourced;
  const found = { name: block.name, version: block.version };
  if (block.name === slug && block.version === version) return { found };

  if (!dryRun) {
    let updated = text.slice(0, block.start);
    updated += text
      .slice(block.start, block.end)
      .replace(/^name = "[^"]+"/m, `name = "${slug}"`)
      .replace(/^version = "[^"]+"/m, `version = "${version}"`);
    updated += text.slice(block.end);
    writeFileSync(path, updated);
  }
  return { found };
}

/**
 * Refuse a mirror whose pattern does not capture `(before)(value)(after)`.
 *
 * A `String.replace` callback is passed `(whole, ...groups, offset, string)`,
 * so a pattern with two groups puts the match *offset* where the code expects
 * the text after the value — which splices a number into the file and leaves
 * no trace of why. `$` in a pattern is fine; this is about group count only.
 */
function assertThreeGroups(mirror: Mirror): void {
  // Appending an empty alternative (`|`) makes the regex match the empty
  // string unconditionally, and a successful match always reports one slot per
  // group — so the count is read without needing a sample line. Four slots
  // means three groups: the whole match plus `before`, `value` and `after`.
  const probe = new RegExp(`${mirror.pattern.source}|`);
  const slots = "".match(probe)?.length ?? 0;
  if (slots === 4) return;
  const seen =
    slots === 0
      ? "is not a usable pattern"
      : `captures ${slots - 1} group${slots === 2 ? "" : "s"}`;
  throw new Error(
    `${TAG} ${mirror.file}: the mirror for "${mirror.anchor}" ${seen}; ` +
      `${META_TS} requires exactly 3 — before, value, after.`,
  );
}

/** Count matches of `pattern` with the `g` flag forced on. */
function countMatches(text: string, pattern: RegExp): number {
  const global = new RegExp(
    pattern.source,
    pattern.flags.includes("g") ? pattern.flags : `${pattern.flags}g`,
  );
  return [...text.matchAll(global)].length;
}

/**
 * The slice of `text` a mirror is allowed to look in, and where that slice
 * starts in the file. Without a `section` the whole file is in scope.
 */
function scopeOf(mirror: Mirror, text: string): { body: string; at: number } {
  if (!mirror.section) return { body: text, at: 0 };
  const found = countMatches(text, mirror.section);
  if (found !== 1) {
    throw new Error(
      `${TAG} ${mirror.file}: expected exactly one section for ` +
        `"${mirror.anchor}", found ${found}. Fix the section in ${META_TS}.`,
    );
  }
  const match = text.match(mirror.section);
  if (!match || match.index === undefined) {
    throw new Error(`${TAG} ${mirror.file}: section vanished between passes.`);
  }
  // Match 0 is the header plus the body, so the body starts exactly one body
  // length before the end of match 0. Exact even when the header text also
  // occurs inside the body.
  return {
    body: match[1],
    at: match.index + match[0].length - match[1].length,
  };
}

/**
 * Read one mirror's current value, refusing to guess when the anchor is
 * ambiguous or gone.
 */
function readMirror(mirror: Mirror): {
  current: string;
  text: string;
  at: number;
} {
  assertThreeGroups(mirror);
  const text = readFileSync(join(root, mirror.file), "utf-8");
  const { body, at } = scopeOf(mirror, text);
  const expected = mirror.count ?? 1;
  const found = countMatches(body, mirror.pattern);
  if (found !== expected) {
    throw new Error(
      `${TAG} ${mirror.file}: expected ${expected} × ${mirror.anchor}, found ` +
        `${found}. Fix the anchor in ${META_TS} — refusing to sync a file this ` +
        `cannot point at unambiguously.`,
    );
  }
  const match = body.match(mirror.pattern);
  if (!match) {
    // Unreachable: countMatches proved a match. Kept so the type is narrow.
    throw new Error(`${TAG} ${mirror.file}: anchor vanished between passes.`);
  }
  return { current: match[2], text, at };
}

/** Rewrite one mirror's value in place, leaving everything else byte-identical. */
function writeMirror(
  mirror: Mirror,
  read: { text: string; at: number },
  next: string,
): void {
  const { text, at } = read;
  const { body, at: sectionAt } = scopeOf(mirror, text);
  const global = new RegExp(
    mirror.pattern.source,
    mirror.pattern.flags.includes("g")
      ? mirror.pattern.flags
      : `${mirror.pattern.flags}g`,
  );
  const rewritten = body.replace(
    global,
    (_whole, before: string, _value: string, after: string) =>
      `${before}${next}${after}`,
  );
  if (rewritten === body) {
    throw new Error(`${TAG} ${mirror.file}: replacement changed nothing.`);
  }
  if (sectionAt !== at && mirror.section) {
    throw new Error(
      `${TAG} ${mirror.file}: the section moved between reading and writing.`,
    );
  }
  const updated =
    text.slice(0, sectionAt) + rewritten + text.slice(sectionAt + body.length);
  writeFileSync(join(root, mirror.file), updated);
}

// ---------------------------------------------------------------------------
// report helpers
// ---------------------------------------------------------------------------

function pad(value: string, width: number): string {
  return value.length >= width
    ? value
    : value + " ".repeat(width - value.length);
}

export interface MetaReport {
  /** Mirrors already holding the value they must hold. */
  inSync: string[];
  /** Mirrors holding something else — fixed by sync, fatal for check. */
  drifted: { file: string; was: string; now: string }[];
  /** Generated files that differ from their source — fatal for check. */
  stale: string[];
}

type SyncOptions = {
  /** Report only; write nothing. */
  dryRun?: boolean;
  /**
   * Version to write instead of `APP.version`. `--set` and `--bump` rewrite
   * the constant in this file first, so the module binding is already stale by
   * the time the mirrors run; they pass the new version through here.
   */
  versionOverride?: string;
};

// ---------------------------------------------------------------------------
// operations
// ---------------------------------------------------------------------------

/**
 * Point every mirror at the constants above and regenerate both modules.
 *
 * @param options.dryRun report what would change and write nothing.
 * @returns what was already correct, what was rewritten, what was regenerated.
 */
export function syncAppMeta(options: SyncOptions = {}): MetaReport {
  const { dryRun = false, versionOverride } = options;
  const report: MetaReport = { inSync: [], drifted: [], stale: [] };
  const version = versionOverride ?? APP.version;
  const { slug } = APP;
  // A path plus its anchor description is the label; `?` keeps `@` sane.
  const label = (file: string, anchor?: string) =>
    anchor === undefined ? file : `${file}  (${anchor})`;

  const width = Math.max(...mirrors().map((m) => m.file.length), 20) + 12;

  console.log(`${TAG} ${APP.name} ${version} — ${META_TS}`);

  for (const mirror of mirrors()) {
    const wanted = mirror.value(version, slug);
    const read = readMirror(mirror);
    const name = label(mirror.file, mirror.anchor);
    if (read.current === wanted) {
      report.inSync.push(name);
      console.log(`${TAG}   ${pad(name, width)}  ${read.current}`);
      continue;
    }
    report.drifted.push({ file: name, was: read.current, now: wanted });
    console.log(
      `${TAG}   ${pad(name, width)}  ${read.current} -> ${wanted}` +
        (dryRun ? "  (not written)" : ""),
    );
    if (!dryRun) writeMirror(mirror, read, wanted);
  }

  // Cargo.lock, by shape rather than by pattern — see syncLockRoot.
  const { name, version: held } = syncLockRoot(slug, version, dryRun).found;
  const lockLabel = label(
    "src-tauri/Cargo.lock",
    "the sourceless [[package]] block",
  );
  if (name === slug && held === version) {
    report.inSync.push(lockLabel);
    console.log(`${TAG}   ${pad(lockLabel, width)}  ${held}`);
  } else {
    report.drifted.push({
      file: lockLabel,
      was: `${name} ${held}`,
      now: `${slug} ${version}`,
    });
    console.log(
      `${TAG}   ${pad(lockLabel, width)}  ${name} ${held} -> ${slug} ${version}` +
        (dryRun ? "  (not written)" : ""),
    );
  }

  for (const [file, content] of generatedFiles()) {
    const path = join(root, file);
    const existing = existsSync(path) ? readFileSync(path, "utf-8") : null;
    if (existing === content) {
      report.inSync.push(file);
      console.log(`${TAG}   ${pad(file, width)}  generated`);
      continue;
    }
    report.stale.push(file);
    console.log(
      `${TAG}   ${pad(file, width)}  ` +
        (existing === null ? "missing -> generated" : "stale -> regenerated") +
        (dryRun ? "  (not written)" : ""),
    );
    if (!dryRun) writeFileSync(path, content);
  }

  const changed = report.drifted.length + report.stale.length;
  if (changed === 0) {
    console.log(`${TAG} ${report.inSync.length} location(s) already in sync.`);
  } else {
    console.log(
      `${TAG} ${changed} location(s) ${dryRun ? "would be updated" : "updated"}.`,
    );
  }
  return report;
}

/**
 * Report drift without writing. The caller decides the exit code from the
 * returned report.
 */
export function checkAppMeta(): MetaReport {
  const report = syncAppMeta({ dryRun: true });
  if (report.drifted.length > 0) {
    console.error(
      `${TAG} ${report.drifted.length} mirror(s) disagree with ${META_TS}. ` +
        `Run 'bun run meta:sync'.`,
    );
  }
  if (report.stale.length > 0) {
    console.error(
      `${TAG} ${report.stale.length} generated file(s) are stale: ` +
        `${report.stale.join(", ")}. Run 'bun run meta:sync'.`,
    );
  }
  reportReleaseNotes(APP.version);
  return report;
}

/** Advisory only: a missing notes file costs the What's New entry, not a build. */
function reportReleaseNotes(version: string): void {
  if (existsSync(join(root, RELEASE_NOTES_DIR, `${version}.md`))) return;
  console.log(
    `${TAG} note: ${RELEASE_NOTES_DIR}/${version}.md does not exist — the ` +
      `What's New dialog will skip this release.`,
  );
}

/** Rewrite the `version` field inside `APP`. The mirrors are the next step. */
function writeVersion(next: string, dryRun: boolean): void {
  const path = join(root, META_TS);
  const text = readFileSync(path, "utf-8");
  const found = countMatches(text, VERSION_RE);
  if (found !== 1) {
    throw new Error(
      `${TAG} ${META_TS}: expected exactly one version field, found ${found}.`,
    );
  }
  if (dryRun) {
    console.log(`${TAG} ${META_TS}  ${APP.version} -> ${next}  (not written)`);
    return;
  }
  writeFileSync(
    path,
    text.replace(
      VERSION_RE,
      (_whole, before: string, _value: string, after: string) =>
        `${before}${next}${after}`,
    ),
  );
  console.log(`${TAG} ${META_TS}  ${APP.version} -> ${next}`);
}

// ---------------------------------------------------------------------------
// semver helpers
// ---------------------------------------------------------------------------

type SemverParts = [major: number, minor: number, patch: number];

/** Parse a strict `major.minor.patch`. Returns null for anything else. */
export function parseSemver(version: string): SemverParts | null {
  const match = version.trim().match(/^(\d+)\.(\d+)\.(\d+)$/);
  if (!match) return null;
  return [Number(match[1]), Number(match[2]), Number(match[3])];
}

function formatSemver([major, minor, patch]: SemverParts): string {
  return `${major}.${minor}.${patch}`;
}

/** The next version after `current` at the given release level. */
export function bumpSemver(
  current: string,
  level: "major" | "minor" | "patch",
): string {
  const parts = parseSemver(current);
  if (!parts) {
    throw new Error(`${TAG} APP.version is not a plain x.y.z: ${current}`);
  }
  const [major, minor, patch] = parts;
  switch (level) {
    case "major":
      return formatSemver([major + 1, 0, 0]);
    case "minor":
      return formatSemver([major, minor + 1, 0]);
    case "patch":
      return formatSemver([major, minor, patch + 1]);
  }
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

function printUsage(): void {
  console.log(`Usage: bun ${META_TS} [options]

  (no arguments)              Sync every mirror to the constants in ${META_TS}
                              and regenerate app_identity.rs / appIdentity.ts.
  --check                     Report drift; exit 1 when anything disagrees.
  --set <x.y.z>               Set the version, then sync.
  --bump <major|minor|patch>  Increment the version, then sync.
  --dry-run                   Print what would change and write nothing.
  -h, --help                  Show this message.

${META_TS} defines ${APP.name}'s identity and version. Mirrored into:
  package.json (name, version)
  src-tauri/Cargo.toml (name, version, description, authors, default-run)
  src-tauri/tauri.conf.json (productName, version, identifier,
                             updater endpoint, linux lib dirs)
  src-tauri/Cargo.lock (root package name, version)
  src-tauri/nsis/installer.nsi (portable marker)
  index.html (document title)
  flake.nix (flake description, package attribute, pname, mainProgram,
             homepage, packages.<system> refs, module defaults, updater
             env flag, dev-shell greeting)
Generated from it:
  src-tauri/src/app_identity.rs
  src/lib/appIdentity.ts
  nix/module.nix
  nix/hm-module.nix`);
}

export function main(argv: string[]): number {
  let mode: "sync" | "check" | "set" | "bump" = "sync";
  let dryRun = false;
  let value: string | undefined;

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    switch (arg) {
      case "--check":
        mode = "check";
        break;
      case "--dry-run":
        dryRun = true;
        break;
      case "--set":
        mode = "set";
        value = argv[++index];
        break;
      case "--bump":
        mode = "bump";
        value = argv[++index];
        break;
      case "-h":
      case "--help":
        printUsage();
        return 0;
      default:
        console.error(`${TAG} unknown argument: ${arg}`);
        printUsage();
        return 1;
    }
  }

  try {
    switch (mode) {
      case "check": {
        const report = checkAppMeta();
        return report.drifted.length > 0 || report.stale.length > 0 ? 1 : 0;
      }

      case "set":
      case "bump": {
        if (value === undefined) {
          console.error(`${TAG} --${mode} needs a value.`);
          printUsage();
          return 1;
        }
        if (mode === "bump" && !["major", "minor", "patch"].includes(value)) {
          console.error(
            `${TAG} --bump expects major, minor or patch, got: ${value}`,
          );
          return 1;
        }
        const next =
          mode === "set"
            ? value
            : bumpSemver(APP.version, value as "major" | "minor" | "patch");
        if (!parseSemver(next)) {
          console.error(
            `${TAG} --set expects a plain x.y.z version, got: ${value}`,
          );
          return 1;
        }
        writeVersion(next, dryRun);
        // The module binding is stale after the rewrite, and `mirror.value`
        // would otherwise be handed APP.version — so pass the new version
        // through explicitly.
        syncAppMeta({ dryRun, versionOverride: next });
        return 0;
      }

      default: {
        syncAppMeta({ dryRun });
        return 0;
      }
    }
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    return 1;
  }
}

if (import.meta.main) {
  process.exit(main(process.argv.slice(2)));
}
