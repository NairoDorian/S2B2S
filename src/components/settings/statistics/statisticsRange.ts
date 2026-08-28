import type { StatisticsRange } from "@/bindings";

export type StatisticsRangeOption =
  | "today"
  | "sevenDays"
  | "thirtyDays"
  | "allTime";

export const STATISTICS_RANGE_OPTIONS: StatisticsRangeOption[] = [
  "today",
  "sevenDays",
  "thirtyDays",
  "allTime",
];

export const buildStatisticsRange = (
  option: StatisticsRangeOption,
  nowMs = Date.now(),
): StatisticsRange => {
  if (option === "allTime") {
    return { start_ms: 0, end_ms: nowMs + 1 };
  }

  const start = new Date(nowMs);
  start.setHours(0, 0, 0, 0);

  if (option === "sevenDays") {
    start.setDate(start.getDate() - 6);
  } else if (option === "thirtyDays") {
    start.setDate(start.getDate() - 29);
  }

  return { start_ms: start.getTime(), end_ms: nowMs + 1 };
};
