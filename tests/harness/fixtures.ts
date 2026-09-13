// Phase 0(b): the Playwright fixtures the migration net is built on.
//
// Import `test`/`expect` from here, not from `@playwright/test`, in any spec
// that needs the app to actually render. The two init scripts below are what
// make that possible, and their ORDER matters:
//
//   1. the generated fixtures, as plain globals, and THEN
//   2. `tauri-mock.js`, which reads those globals while it builds its responses.
//
// Playwright evaluates init scripts in registration order, so this file is the
// one place that ordering lives.
//
// The fixture values are *placeholders*, not the app's real defaults (see
// `generate-fixtures.ts`). They exist to make the window boot and paint, so
// assert on structure and on what a click changes — never on a value that came
// from here.
import { test as base, expect } from "@playwright/test";
import { homedir } from "node:os";
import { join } from "node:path";
import { settingsFixture, ipcFixtures } from "./fixtures.generated";

/**
 * The appdata directory, resolved from the environment at run time — never
 * written into a file. The mock's `plugin:path|resolve_directory` answer uses
 * it so a path display shows something realistic without this harness (or the
 * repo) carrying any machine-specific value.
 */
const appDataDir = process.env.APPDATA ?? join(homedir(), "AppData", "Roaming");

/**
 * Shallow overrides merged over `settingsFixture` before the app boots.
 *
 * The generated fixture is placeholders for a reason (see its header), but some
 * of those placeholders do not merely leave a value blank — they gate whether a
 * page renders at all. `post_process_enabled: false` means the Post Process
 * entry is not in the sidebar, so a test of anything on that page has to reach
 * it first. This is that lever, and it exists for *reachability* only: the
 * assertions remain about behaviour, not about the value that was set.
 *
 * A test opts in with `test.use({ settingsOverrides: { … } })` inside its
 * `describe`, which re-creates this fixture and therefore re-registers the init
 * scripts with the merged object.
 */
export type SettingsOverrides = Record<string, unknown>;

export const test = base.extend<{
  tauriHarness: void;
  settingsOverrides: SettingsOverrides;
}>({
  settingsOverrides: [{}, { option: true }],
  tauriHarness: [
    async ({ page, settingsOverrides }, use) => {
      await page.addInitScript(
        (fixtures) => {
          const w = window as unknown as Record<string, unknown>;
          w.__TAURI_MOCK_SETTINGS__ = fixtures.settings;
          w.__TAURI_MOCK_IPC__ = fixtures.ipc;
          w.__TAURI_MOCK_PATHS__ = fixtures.paths;
        },
        {
          settings: { ...settingsFixture, ...settingsOverrides },
          ipc: ipcFixtures,
          paths: { appDataDir },
        },
      );
      await page.addInitScript({ path: "tests/harness/tauri-mock.js" });
      await use();
    },
    { auto: true },
  ],
});

export { expect };
