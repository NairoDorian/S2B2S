/**
 * Color math and palette generation helpers for dynamic accent theme customization.
 */

export interface HSL {
  h: number; // 0 - 360
  s: number; // 0 - 100
  l: number; // 0 - 100
}

export interface RGB {
  r: number; // 0 - 255
  g: number; // 0 - 255
  b: number; // 0 - 255
}

export interface AccentPalette {
  lightLogoPrimary: string;
  lightLogoStroke: string;
  darkLogoPrimary: string;
  darkLogoStroke: string;
  backgroundUi: string;
  logoHighlight: string;
}

export const DEFAULT_ACCENT_COLOR = "#1FE0FF";

export const ACCENT_PRESETS = [
  { id: "cyan", name: "Neon Cyan", hex: "#1FE0FF" },
  { id: "magenta", name: "Neon Magenta", hex: "#FF2E88" },
  { id: "lime", name: "Terminal Green", hex: "#5CFF5C" },
  { id: "gold", name: "Gold", hex: "#E5A93B" },
  { id: "pink", name: "Rose", hex: "#DA5893" },
  { id: "amber", name: "Amber", hex: "#F59E0B" },
  { id: "emerald", name: "Emerald", hex: "#10B981" },
  { id: "sky", name: "Sky Blue", hex: "#0EA5E9" },
  { id: "indigo", name: "Indigo", hex: "#6366F1" },
  { id: "crimson", name: "Crimson", hex: "#F43F5E" },
  { id: "purple", name: "Purple", hex: "#A855F7" },
] as const;

export function parseHex(hex: string): RGB | null {
  const clean = hex.trim().replace(/^#/, "");
  if (clean.length === 3) {
    const r = parseInt(clean[0] + clean[0], 16);
    const g = parseInt(clean[1] + clean[1], 16);
    const b = parseInt(clean[2] + clean[2], 16);
    if (isNaN(r) || isNaN(g) || isNaN(b)) return null;
    return { r, g, b };
  }
  if (clean.length === 6) {
    const r = parseInt(clean.slice(0, 2), 16);
    const g = parseInt(clean.slice(2, 4), 16);
    const b = parseInt(clean.slice(4, 6), 16);
    if (isNaN(r) || isNaN(g) || isNaN(b)) return null;
    return { r, g, b };
  }
  return null;
}

export function rgbToHex({ r, g, b }: RGB): string {
  const clamp = (n: number) => Math.max(0, Math.min(255, Math.round(n)));
  const toHex = (n: number) => clamp(n).toString(16).padStart(2, "0");
  return `#${toHex(r)}${toHex(g)}${toHex(b)}`;
}

export function rgbToHsl({ r, g, b }: RGB): HSL {
  const rf = r / 255;
  const gf = g / 255;
  const bf = b / 255;

  const max = Math.max(rf, gf, bf);
  const min = Math.min(rf, gf, bf);
  let h = 0;
  let s = 0;
  const l = (max + min) / 2;

  if (max !== min) {
    const d = max - min;
    s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
    switch (max) {
      case rf:
        h = (gf - bf) / d + (gf < bf ? 6 : 0);
        break;
      case gf:
        h = (bf - rf) / d + 2;
        break;
      case bf:
        h = (rf - gf) / d + 4;
        break;
    }
    h *= 60;
  }

  return {
    h: Math.round(h),
    s: Math.round(s * 100),
    l: Math.round(l * 100),
  };
}

export function hslToRgb({ h, s, l }: HSL): RGB {
  const hNorm = ((h % 360) + 360) % 360;
  const sNorm = Math.max(0, Math.min(100, s)) / 100;
  const lNorm = Math.max(0, Math.min(100, l)) / 100;

  const c = (1 - Math.abs(2 * lNorm - 1)) * sNorm;
  const x = c * (1 - Math.abs(((hNorm / 60) % 2) - 1));
  const m = lNorm - c / 2;

  let rPrime = 0;
  let gPrime = 0;
  let bPrime = 0;

  if (hNorm >= 0 && hNorm < 60) {
    rPrime = c;
    gPrime = x;
    bPrime = 0;
  } else if (hNorm >= 60 && hNorm < 120) {
    rPrime = x;
    gPrime = c;
    bPrime = 0;
  } else if (hNorm >= 120 && hNorm < 180) {
    rPrime = 0;
    gPrime = c;
    bPrime = x;
  } else if (hNorm >= 180 && hNorm < 240) {
    rPrime = 0;
    gPrime = x;
    bPrime = c;
  } else if (hNorm >= 240 && hNorm < 300) {
    rPrime = x;
    gPrime = 0;
    bPrime = c;
  } else {
    rPrime = c;
    gPrime = 0;
    bPrime = x;
  }

  return {
    r: Math.round((rPrime + m) * 255),
    g: Math.round((gPrime + m) * 255),
    b: Math.round((bPrime + m) * 255),
  };
}

export function hslToHex(hsl: HSL): string {
  return rgbToHex(hslToRgb(hsl));
}

/**
 * Derives a complete, coordinated theme palette from a single base color.
 */
export function computeAccentPalette(baseHex: string): AccentPalette {
  const rgb = parseHex(baseHex) || parseHex(DEFAULT_ACCENT_COLOR)!;
  const hsl = rgbToHsl(rgb);

  // Light mode primary: normalized hex
  const lightLogoPrimary = rgbToHex(rgb);

  // Light mode stroke: dark tone matching the hue for sharp contrast on light surface
  const lightLogoStroke = hslToHex({
    h: hsl.h,
    s: Math.max(25, Math.round(hsl.s * 0.4)),
    l: 16,
  });

  // Dark mode primary: luminous, vibrant version for dark surface
  const darkLogoPrimary = hslToHex({
    h: hsl.h,
    s: Math.min(100, Math.round(hsl.s * 1.1)),
    l: Math.min(68, Math.max(54, Math.round(hsl.l * 1.05))),
  });

  // Dark mode stroke: pale pastel tint for soft outline on dark surface
  const darkLogoStroke = hslToHex({
    h: hsl.h,
    s: Math.max(30, Math.round(hsl.s * 0.5)),
    l: 86,
  });

  // UI background (buttons, toggles, slider tracks): rich saturated tone with high contrast against white text
  const backgroundUi = hslToHex({
    h: hsl.h,
    s: Math.min(100, Math.max(70, hsl.s)),
    l: 42,
  });

  // Logo highlight inner path
  const logoHighlight = hslToHex({
    h: hsl.h,
    s: Math.max(35, Math.round(hsl.s * 0.6)),
    l: 84,
  });

  return {
    lightLogoPrimary,
    lightLogoStroke,
    darkLogoPrimary,
    darkLogoStroke,
    backgroundUi,
    logoHighlight,
  };
}
