import { useTranslation } from "@/i18n/useTranslation";

const { t } = useTranslation();

export const formatModelSize = (sizeMb: number | null | undefined): string => {
  if (!sizeMb || !Number.isFinite(sizeMb) || sizeMb <= 0) {
    return t("common.unknownSize");
  }

  if (sizeMb >= 1024) {
    const sizeGb = sizeMb / 1024;
    const formatter = new Intl.NumberFormat(undefined, {
      minimumFractionDigits: sizeGb >= 10 ? 0 : 1,
      maximumFractionDigits: sizeGb >= 10 ? 0 : 1,
    });
    return `${formatter.format(sizeGb)} GB`;
  }

  const formatter = new Intl.NumberFormat(undefined, {
    minimumFractionDigits: sizeMb >= 100 ? 0 : 1,
    maximumFractionDigits: sizeMb >= 100 ? 0 : 1,
  });

  return `${formatter.format(sizeMb)} MB`;
};

/**
 * A percentage into the `0…100` a bar can actually draw. Callers that already
 * hold a percentage pass it straight through.
 */
export const clampPercent = (percent: number): number =>
  Math.max(0, Math.min(100, percent));

/**
 * A ratio (`0…1`, a playback position, a meter reading) as a percentage in
 * `0…100`, rounded for display.
 *
 * One clamp for every meter in the app: the CPU/GPU bars, the playback
 * position and the VAD level all arrive from a ratio or a raw percentage, and
 * a value outside `0…1`/`0…100` would otherwise overflow the bar's track or
 * show a number no bar agrees with.
 */
export const formatPercent = (ratio: number): string =>
  `${Math.round(clampPercent(ratio * 100))}%`;

/**
 * A duration as a clock: `m:ss` under an hour, `h:mm:ss` from one hour.
 * Fractions of a second are dropped; round the input first to round instead.
 */
export const formatClockSeconds = (totalSeconds: number): string => {
  const total = Math.max(0, Math.floor(totalSeconds));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mm = h > 0 ? String(m).padStart(2, "0") : String(m);
  return `${h > 0 ? `${h}:` : ""}${mm}:${String(s).padStart(2, "0")}`;
};
