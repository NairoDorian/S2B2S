/**
 * Global single source of truth for the application version.
 *
 * EVERY other version location in this repository is DERIVED from this constant:
 *
 *  - `package.json`              — synced by `scripts/before-commit.ts`
 *  - `src-tauri/Cargo.toml`      — synced by `scripts/before-commit.ts`
 *  - `src-tauri/tauri.conf.json` — synced by `scripts/before-commit.ts` (drives
 *                                  the bundled artifact + updater feed version)
 *  - `src-tauri/Cargo.lock`      — refreshed by `cargo update` when Cargo.toml
 *                                  changes (run from before-commit.ts)
 *  - `__APP_VERSION__`           — injected into the frontend at build time by
 *                                  Vite's `define` (see vite.config.ts); React
 *                                  code reads this constant and never hardcodes
 *                                  a version string
 *
 * NEVER hardcode the application version anywhere else. Edit this constant (or
 * run `bun run before-commit --bump <major|minor|patch>`) and let the sync
 * script propagate it — this is what prevents silent version drift between the
 * package manager, the Cargo crate, and the Tauri bundle config.
 */
export const APP_VERSION = "0.1.4";
