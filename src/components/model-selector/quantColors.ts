/**
 * Colour ramp for quantization variants.
 *
 * The hues are ordinal, not decorative: they walk from warm (aggressive, small,
 * fastest, lowest fidelity) to cool (large, slowest, closest to the original
 * weights), so a glance at the dots tells you where a variant sits on the
 * size/quality trade-off even for quants you have never seen before.
 *
 * Matching is by prefix so unlisted variants of a known family (`IQ4_XS`,
 * `Q5_K`, `FP16`, …) still land on the right rung.
 */
const QUANT_TIERS: Array<[RegExp, string]> = [
  [/^I?Q2/i, "bg-rose-500"],
  [/^I?Q3/i, "bg-orange-500"],
  [/^I?Q4/i, "bg-amber-500"],
  [/^I?Q5/i, "bg-emerald-500"],
  [/^I?Q6/i, "bg-teal-500"],
  [/^I?Q7/i, "bg-cyan-500"],
  [/^I?Q8/i, "bg-sky-500"],
  [/^BF16/i, "bg-indigo-400"],
  [/^FP?16/i, "bg-indigo-400"],
  [/^FP?32/i, "bg-violet-400"],
];

const UNKNOWN_QUANT_COLOR = "bg-mid-gray/60";

/** Tailwind background class for a quantization's indicator dot. */
export function getQuantColor(quant: string): string {
  const match = QUANT_TIERS.find(([pattern]) => pattern.test(quant));
  return match ? match[1] : UNKNOWN_QUANT_COLOR;
}
