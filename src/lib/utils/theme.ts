import { commands, type Theme } from "@/bindings";
import { emit } from "@tauri-apps/api/event";
import { computeAccentPalette, DEFAULT_ACCENT_COLOR, parseHex } from "./color";

/**
 * Appearance theme handling.
 *
 * Handy already ships a full light palette and a full dark palette (see
 * `App.css`). This module lets the user pick which one is used instead of
 * always following the OS:
 *  - `system` removes the override so the `prefers-color-scheme` media query
 *    governs (the historical behaviour).
 *  - `light` / `dark` set `data-theme` on the document root, whose
 *    higher-specificity CSS selectors win over the media query.
 *
 * The choice is persisted in `AppSettings` (source of truth) and mirrored to
 * localStorage so it can be applied synchronously on boot, before React mounts,
 * avoiding a flash of the wrong palette.
 */

export const THEME_STORAGE_KEY = "handy.theme";
export const ACCENT_COLOR_STORAGE_KEY = "handy.accent_color";

export const THEME_OPTIONS: Theme[] = ["system", "light", "dark"];

const isTheme = (value: unknown): value is Theme =>
  value === "system" || value === "light" || value === "dark";

/** Apply a theme to the document root and remember it for the next launch. */
export const applyTheme = (theme: Theme): void => {
  const root = document.documentElement;
  if (theme === "system") {
    delete root.dataset.theme;
  } else {
    root.dataset.theme = theme;
  }
  try {
    localStorage.setItem(THEME_STORAGE_KEY, theme);
  } catch {
    // localStorage may be unavailable (e.g. private mode); the setting still
    // persists in AppSettings, so this only costs a one-frame flash on boot.
  }
};

/** Read the last-applied theme for synchronous boot-time application. */
export const getStoredTheme = (): Theme => {
  try {
    const stored = localStorage.getItem(THEME_STORAGE_KEY);
    if (isTheme(stored)) return stored;
  } catch {
    // ignore
  }
  return "system";
};

/** Apply the persisted theme from AppSettings (the source of truth). */
export const syncThemeFromSettings = async (): Promise<void> => {
  try {
    const result = await commands.getAppSettings();
    if (result.status === "ok") {
      applyTheme(result.data.theme ?? "system");
    }
  } catch (e) {
    console.warn("Failed to sync theme from settings:", e);
  }
};

/** Apply an accent color palette dynamically to the document root. */
export const applyAccentColor = (hex: string, broadcast = false): void => {
  if (!parseHex(hex)) {
    hex = DEFAULT_ACCENT_COLOR;
  }
  const palette = computeAccentPalette(hex);
  const root = document.documentElement;

  root.style.setProperty(
    "--light-color-logo-primary",
    palette.lightLogoPrimary,
  );
  root.style.setProperty("--light-color-logo-stroke", palette.lightLogoStroke);
  root.style.setProperty("--dark-color-logo-primary", palette.darkLogoPrimary);
  root.style.setProperty("--dark-color-logo-stroke", palette.darkLogoStroke);
  root.style.setProperty("--color-background-ui", palette.backgroundUi);
  root.style.setProperty("--color-logo-highlight", palette.logoHighlight);

  try {
    localStorage.setItem(ACCENT_COLOR_STORAGE_KEY, hex);
  } catch {
    // ignore
  }

  if (broadcast) {
    emit("accent-color-changed", hex).catch(console.warn);
  }
};

/** Read the last-applied accent color for synchronous boot-time application. */
export const getStoredAccentColor = (): string => {
  try {
    const stored = localStorage.getItem(ACCENT_COLOR_STORAGE_KEY);
    if (stored && parseHex(stored)) return stored;
  } catch {
    // ignore
  }
  return DEFAULT_ACCENT_COLOR;
};

/** Apply the persisted accent color from AppSettings (the source of truth). */
export const syncAccentColorFromSettings = async (): Promise<void> => {
  try {
    const result = await commands.getAppSettings();
    if (result.status === "ok") {
      const color = result.data.custom_accent_color || DEFAULT_ACCENT_COLOR;
      applyAccentColor(color, false);
    }
  } catch (e) {
    console.warn("Failed to sync accent color from settings:", e);
  }
};

export const UI_SCALE_STORAGE_KEY = "handy.ui_scale";
const MIN_UI_SCALE = 0.7;
const MAX_UI_SCALE = 1.6;

const clampUiScale = (value: unknown): number => {
  const n = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(n)) return 1;
  return Math.min(MAX_UI_SCALE, Math.max(MIN_UI_SCALE, n));
};

/**
 * Zoom the whole document. CSS zoom (supported by WebKit / WebView2 / Gecko
 * 126+) scales layout as well as text, so fixed-position chrome and popovers
 * keep their geometry. Mirrored to localStorage for a flash-free boot.
 */
export const applyUiScale = (scale: number): void => {
  const clamped = clampUiScale(scale);
  const style = document.documentElement.style as CSSStyleDeclaration & {
    zoom?: string;
  };
  style.zoom = clamped === 1 ? "" : String(clamped);
  try {
    localStorage.setItem(UI_SCALE_STORAGE_KEY, String(clamped));
  } catch {
    // Persisted in AppSettings anyway; only the boot flash is affected.
  }
};

export const getStoredUiScale = (): number => {
  try {
    return clampUiScale(localStorage.getItem(UI_SCALE_STORAGE_KEY) ?? 1);
  } catch {
    return 1;
  }
};

export const syncUiScaleFromSettings = async (): Promise<void> => {
  try {
    const result = await commands.getAppSettings();
    if (result.status === "ok") {
      applyUiScale(result.data.ui_scale ?? 1);
    }
  } catch (error) {
    console.error("Failed to sync UI scale from settings:", error);
  }
};
