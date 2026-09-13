import { render } from "@solidjs/web";
import { platform } from "@tauri-apps/plugin-os";
import App from "./App";
import { i18nReady } from "@/i18n";
import {
  applyTheme,
  getStoredTheme,
  syncThemeFromSettings,
  applyAccentColor,
  getStoredAccentColor,
  syncAccentColorFromSettings,
  applyUiScale,
  getStoredUiScale,
  syncUiScaleFromSettings,
} from "@/lib/utils/theme";
import { listen } from "@tauri-apps/api/event";
import { useModelStore } from "./stores/modelStore";
import type { Theme } from "@/bindings";

// Set the platform before render so CSS can scope per-platform (e.g. the
// macOS scrollbar rules in App.css key off `:root[data-platform="macos"]`).
document.documentElement.dataset.platform = platform();

// Apply the last-known theme, accent and UI scale synchronously before the
// first paint to avoid a flash of the wrong palette, then reconcile with the
// persisted settings once they load.
applyTheme(getStoredTheme());
syncThemeFromSettings();
applyAccentColor(getStoredAccentColor());
syncAccentColorFromSettings();
applyUiScale(getStoredUiScale());
syncUiScaleFromSettings();

// Load the model catalog and register its event listeners. The Models page
// renders a spinner until `loading` clears, and `loading` only clears when this
// runs — without it that page waits forever.
void useModelStore().initialize();

listen<Theme>("theme-changed", (event) => applyTheme(event.payload));
listen<string>("accent-color-changed", (event) =>
  applyAccentColor(event.payload),
);

// Translations load lazily (one chunk per language); wait for the initial one so
// the first paint carries no raw keys. No StrictMode — Solid has no counterpart
// and the component body runs exactly once.
i18nReady.then(() => {
  render(() => <App />, document.getElementById("root") as HTMLElement);
});
