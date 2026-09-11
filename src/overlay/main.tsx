import React from "react";
import ReactDOM from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import RecordingOverlay from "./RecordingOverlay";
import {
  applyTheme,
  getStoredTheme,
  syncThemeFromSettings,
  applyAccentColor,
  getStoredAccentColor,
  syncAccentColorFromSettings,
} from "@/lib/utils/theme";
import type { Theme } from "@/bindings";
import { i18nReady } from "@/i18n";

// A separate webview from the settings window, so the overlay has to set
// `data-theme` and accent CSS properties on its own document: last-known theme/accent
// before render (shared localStorage) to avoid a flash, reconcile with the persisted
// setting in case the overlay booted first, then follow live changes.
applyTheme(getStoredTheme());
syncThemeFromSettings();
applyAccentColor(getStoredAccentColor());
syncAccentColorFromSettings();

listen<Theme>("theme-changed", (event) => applyTheme(event.payload));
listen<string>("accent-color-changed", (event) =>
  applyAccentColor(event.payload),
);

// Translations load lazily (one chunk per language); wait for the initial
// one so the first paint carries no raw keys.
i18nReady.then(() => {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <RecordingOverlay />
    </React.StrictMode>,
  );
});
