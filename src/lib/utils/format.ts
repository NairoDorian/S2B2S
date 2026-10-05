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
 * A percentage into the `0…100` a bar can actually draw. This is the clamp for
 * every meter that already holds a percentage: the CPU/GPU bars
 * (`SystemMeters`), the download progress bars and the audio player's playback
 * position.
 */
export const clampPercent = (percent: number): number =>
  Math.max(0, Math.min(100, percent));

/**
 * A ratio (`0…1`) as a percentage in `0…100`, rounded for display.
 *
 * Every VAD readout goes through here rather than through `clampPercent`: the
 * Earshot score, its peak, the threshold, the level and the RNNoise probability
 * are all ratios, so a value that arrives slightly outside `0…1` renders as a
 * full or empty bar rather than overflowing the track or showing a number the
 * meter disagrees with.
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
