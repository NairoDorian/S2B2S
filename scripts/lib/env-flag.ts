// Environment flags for the build scripts, under one prefix.
//
// This mirrors `src-tauri/src/utils.rs` (`app_env_var` / `app_env_flag`) rule for
// rule, so a flag means the same thing whether the app or a script reads it:
//
//   - the prefix comes from `app-meta.ts`, so no call site spells it;
//   - the pre-rename prefix is tried second, with a warning, because these flags
//     live in shell profiles, CI jobs and Nix wrappers that were written before
//     the rename and would otherwise silently stop working;
//   - "1", "true", "yes" and "on" are true; "0", "false", "no", "off" and the
//     empty string are false, case-insensitively.
//
// Duplicating the rule instead of importing it is deliberate: `utils.rs` is Rust
// inside a crate this runs before, and a build script that pulled in a compiled
// helper to read one variable would be the more fragile of the two.

import { APP } from "../app-meta";

/** Truthiness of a flag *value* — `utils::env_flag_truthy`. */
export function envFlagTruthy(value: string): boolean {
  return !["", "0", "false", "no", "off"].includes(value.trim().toLowerCase());
}

/**
 * Read an application variable by its suffix alone, e.g. `"NO_PRUNE"`.
 *
 * Returns `undefined` when neither spelling is set. The suffix is the part
 * *after* the prefix and includes its separator: `NO_PRUNE` reads
 * `<PREFIX>NO_PRUNE`.
 */
export function appEnvVar(suffix: string): string | undefined {
  const name = `${APP.envPrefix}${suffix}`;
  const value = process.env[name];
  if (value !== undefined) return value;

  const legacy = `${APP.legacy.envPrefix}${suffix}`;
  const legacyValue = process.env[legacy];
  if (legacy === name || legacyValue === undefined) return undefined;

  console.warn(
    `[env] ${legacy} is set — that is the pre-0.9.7 name, please rename it to ${name}`,
  );
  return legacyValue;
}

/** Read an application flag by its suffix alone, as a boolean. */
export function appEnvFlag(suffix: string): boolean {
  const value = appEnvVar(suffix);
  return value === undefined ? false : envFlagTruthy(value);
}

/** The current spelling of a flag, for messages that tell the user to set it. */
export function appEnvFlagName(suffix: string): string {
  return `${APP.envPrefix}${suffix}`;
}
