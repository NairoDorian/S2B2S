// GENERATED FILE — DO NOT EDIT.
//
// Source: `scripts/app-meta.ts` — run `bun run meta:sync` to regenerate.
//
// Do not hardcode the application's name, slug, identifier, repository or
// storage prefix anywhere else under `src/`. Import from here instead.

/** Product name, for anything the user sees. */
export const APP_NAME = "ZER0";

/** Machine slug. Never shown to the user. */
export const APP_SLUG = "zer0";

/** `localStorage` key prefix for view-only UI preferences. */
export const STORAGE_PREFIX = "zer0.";

/** Machine slug of the pre-rename identity: the basename 0.9.x wrote. */
export const LEGACY_SLUG = "handy";

/** Bundle identifier — the OS-level application identity. */
export const APP_IDENTIFIER = "com.nairodorian.zer0";

/** Repository URL, for links out of the app. */
export const REPO_URL = "https://github.com/NairoDorian/S2B2S";

/** Releases page — where an update or a portable installer link points. */
export const RELEASES_URL =
  "https://github.com/NairoDorian/S2B2S/releases/latest";

/**
 * The product-name folder leaf a platform may use (kept for the migration's
 * symmetry); the app-data folder itself is named after the identifier.
 */
export const DATA_DIR_NAME = "ZER0";

/** The folder name a pre-rename install used; read by the migration only. */
export const LEGACY_DATA_DIR_NAME = "Handy";

/** The bundle identifier a pre-rename install used. */
export const LEGACY_IDENTIFIER = "com.pais.handy";

/** Prefix of every environment flag the backend reads. Shown in Settings. */
export const ENV_PREFIX = "ZER0_";

/** The previous environment-flag prefix, still honoured by the backend. */
export const LEGACY_ENV_PREFIX = "HANDY_";

/** Magic string in the `portable` marker beside the executable. */
export const PORTABLE_MARKER = "ZER0 Portable Mode";

/** The marker a pre-rename release wrote. */
export const LEGACY_PORTABLE_MARKER = "Handy Portable Mode";

/** The previous `localStorage` prefix, migrated on first read. */
export const LEGACY_STORAGE_PREFIX = "handy.";

/**
 * File-name infix that marks a recording as a Multi-STT one.
 *
 * Mirrors `app_identity::multi_recording_file_name`: a history row records
 * the file name the recorder wrote, so the UI has to recognise the same shape
 * the backend produced.
 */
export const MULTI_RECORDING_INFIX = "-multi-";

/**
 * Whether a history row's WAV belongs to a Multi-STT recording.
 *
 * Both basenames are accepted. History is a database of recordings the user
 * still has on disk, and rows written before the rename point at
 * `${LEGACY_SLUG}-multi-…` files that were never moved — judging them by the
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
 * runs again. Both the read and every write are guarded: `localStorage` throws
 * outright in a locked-down webview, and a preference is never worth a crash.
 *
 * @param key suffix after `STORAGE_PREFIX`, e.g. `"theme"` or `"sidebar.width"`
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
