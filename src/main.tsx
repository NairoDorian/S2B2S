import React from "react";
import ReactDOM from "react-dom/client";
import { platform } from "@tauri-apps/plugin-os";
import App from "./App";
import { installCompatShims } from "./lib/compat";
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
} from "./lib/utils/theme";

installCompatShims();

// Set platform before render so CSS can scope per-platform (e.g. scrollbar styles)
document.documentElement.dataset.platform = platform();

// Apply the last-known theme & accent synchronously before render to avoid a flash of
// the wrong palette, then reconcile with the persisted setting once it loads.
applyTheme(getStoredTheme());
syncThemeFromSettings();
applyAccentColor(getStoredAccentColor());
syncAccentColorFromSettings();
applyUiScale(getStoredUiScale());
syncUiScaleFromSettings();

// Initialize i18n
import { i18nReady } from "./i18n";

// Initialize model store (loads models and sets up event listeners)
import { useModelStore } from "./stores/modelStore";
useModelStore.getState().initialize();

// Translations load lazily (one chunk per language); wait for the initial
// one so the first paint carries no raw keys.
i18nReady.then(() => {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
});
