import { commands, type Theme } from "@/bindings";
import { emit } from "@tauri-apps/api/event";
import { readPref, writePref } from "@/lib/appIdentity";
import { computeAccentPalette, DEFAULT_ACCENT_COLOR, parseHex } from "./color";

/**
 * Appearance theme handling.
 *
 * The app ships a full light palette and a full dark palette (see `App.css`).
 * This module lets the user pick which one is used instead of always following
 * the OS:
 *  - `system` resolves the OS scheme to an explicit `data-theme` on the
 *    document root and follows it live.
 *  - `light` / `dark` set `data-theme` on the document root, whose
 *    higher-specificity CSS selectors win over the media query, and stop
 *    following the OS.
 *
 * The choice is persisted in `AppSettings` (source of truth) and mirrored to
 * localStorage so it can be applied synchronously on boot, before the app renders,
 * avoiding a flash of the wrong palette.
 *
 * The mirror keys are suffixes, not names: `readPref` / `writePref` apply the
 * application's storage prefix and fall back to the pre-rename one, so a theme
 * chosen before 0.9.7 survives the rename.
 */

/** Preference suffixes, resolved by `readPref` / `writePref`. */
const THEME_PREF = "theme";
const ACCENT_COLOR_PREF = "accent_color";

export const THEME_OPTIONS: Theme[] = ["system", "light", "dark"];

const isTheme = (value: unknown): value is Theme =>
  value === "system" || value === "light" || value === "dark";

const OS_DARK_QUERY = "(prefers-color-scheme: dark)";

/** Resolve `system` to an explicit `data-theme` from the OS setting. */
const applySystemTheme = (): void => {
  document.documentElement.dataset.theme = window.matchMedia(OS_DARK_QUERY)
    .matches
    ? "dark"
    : "light";
};

/** The OS dark-mode listener, installed once for the `system` theme. */
let systemThemeQuery: MediaQueryList | null = null;

/**
 * Whether the listener may write `data-theme`. It is installed the first time
 * `system` is applied and never removed, so a later explicit `light` / `dark`
 * has to switch it off here — or the next OS scheme change would override the
 * user's choice.
 */
let followSystem = false;

const onSystemThemeChange = (): void => {
  if (followSystem) applySystemTheme();
};

/** Apply a theme to the document root and remember it for the next launch. */
export const applyTheme = (theme: Theme): void => {
  const root = document.documentElement;
  followSystem = theme === "system";
  if (theme === "system") {
    // `system` resolves to an explicit attribute rather than removing the
    // override: the Tailwind `dark:` variant keys on `[data-theme="dark"]`
    // (App.css @custom-variant), and an absent attribute would leave every
    // `dark:` utility stuck on the OS media query — wrong whenever the app
    // runs on an OS whose scheme disagrees with the palette a user expects.
    // The resolved attribute selects the same palette the media query would,
    // so nothing changes visually; a live listener keeps it tracking the OS.
    applySystemTheme();
    if (!systemThemeQuery) {
      systemThemeQuery = window.matchMedia(OS_DARK_QUERY);
      systemThemeQuery.addEventListener("change", onSystemThemeChange);
    }
  } else {
    root.dataset.theme = theme;
  }
  writePref(THEME_PREF, theme);
};

/** Read the last-applied theme for synchronous boot-time application. */
export const getStoredTheme = (): Theme => {
  const stored = readPref(THEME_PREF);
  return isTheme(stored) ? stored : "system";
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

/**
 * Apply an accent color palette dynamically to the document root.
 *
 * Only the two *palette source* variables are written, never the active
 * `--color-accent`. The active token is a `var(--light|dark-color-accent)`
 * reference resolved by the media query / `data-theme` selectors in
 * `theme.css`, so the theme override keeps deciding which of the two applies.
 */
export const applyAccentColor = (hex: string, broadcast = false): void => {
  if (!parseHex(hex)) {
    hex = DEFAULT_ACCENT_COLOR;
  }
  const palette = computeAccentPalette(hex);
  const root = document.documentElement;

  root.style.setProperty("--light-color-accent", palette.light);
  root.style.setProperty("--dark-color-accent", palette.dark);
  root.style.setProperty("--color-background-ui", palette.backgroundUi);

  writePref(ACCENT_COLOR_PREF, hex);

  if (broadcast) {
    emit("accent-color-changed", hex).catch(console.warn);
  }
};

/** Read the last-applied accent color for synchronous boot-time application. */
export const getStoredAccentColor = (): string => {
  const stored = readPref(ACCENT_COLOR_PREF);
  return stored && parseHex(stored) ? stored : DEFAULT_ACCENT_COLOR;
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

export const UI_SCALE_PREF = "ui_scale";
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
  writePref(UI_SCALE_PREF, String(clamped));
};

export const getStoredUiScale = (): number =>
  clampUiScale(readPref(UI_SCALE_PREF) ?? 1);

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
