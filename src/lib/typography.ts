import type { TypographySettings_Serialize as TypographySettings } from "@/bindings";
import { readPref, writePref } from "@/lib/appIdentity";

export const FONT_ROLES = [
  "ui",
  "menus",
  "overlay",
  "transcript",
  "mono",
] as const;
export type FontRole = (typeof FONT_ROLES)[number];

export const DEFAULT_TYPOGRAPHY: TypographySettings = {
  ui: "Libron",
  menus: "inherit",
  overlay: "inherit",
  transcript: "inherit",
  mono: "inherit",
};

export const SYSTEM_SANS =
  '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Arial, sans-serif';
const LIBRON_STACK = `"Libron", ${SYSTEM_SANS}`;

/** One family name, never a CSS declaration or an arbitrary font stack. */
export const normalizeFont = (value: unknown, role: FontRole): string => {
  if (typeof value !== "string") return DEFAULT_TYPOGRAPHY[role];
  const name = value.trim();
  const characters = Array.from(name);
  if (
    !name ||
    characters.length > 128 ||
    characters.some((character) => {
      const code = character.codePointAt(0)!;
      return code < 32 || (code >= 127 && code <= 159);
    })
  )
    return DEFAULT_TYPOGRAPHY[role];
  return role === "ui" && name === "inherit" ? "Libron" : name;
};

export const normalizeTypography = (value: unknown): TypographySettings => {
  const source = value && typeof value === "object" ? value : {};
  return Object.fromEntries(
    FONT_ROLES.map((role) => [
      role,
      normalizeFont((source as Record<string, unknown>)[role], role),
    ]),
  ) as TypographySettings;
};

/** Quote a family so punctuation in an installed font's name stays literal. */
export const fontStack = (font: string, ui = "Libron"): string => {
  if (font === "inherit") return fontStack(ui);
  if (font === "system-sans") return SYSTEM_SANS;
  if (font === "system-serif") return 'Georgia, "Times New Roman", serif';
  if (font === "system-mono")
    return 'ui-monospace, "Cascadia Mono", "SF Mono", Consolas, Menlo, monospace';
  if (font === "Libron") return LIBRON_STACK;
  const quoted = font.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  return `"${quoted}", ${LIBRON_STACK}`;
};

export const applyTypography = (value: unknown): void => {
  const fonts = normalizeTypography(value);
  for (const role of FONT_ROLES) {
    document.documentElement.style.setProperty(
      `--app-font-${role}`,
      fontStack(fonts[role], fonts.ui),
    );
  }
  writePref("typography", JSON.stringify(fonts));
};

export const getStoredTypography = (): TypographySettings => {
  try {
    return normalizeTypography(JSON.parse(readPref("typography") ?? "null"));
  } catch {
    return { ...DEFAULT_TYPOGRAPHY };
  }
};
