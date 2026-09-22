// Shared log-file line parser for the Debug console.
//
// File lines look like:
//   [2026-09-12][20:30:04][app_lib][DEBUG] message
// Groups: 1 date, 2 time, 3 target, 4 level, 5 message — all five must be
// captured; dropping the target shifts every later index and renders the
// message as the string "undefined".
// A record whose payload contains newlines (e.g. multi-line SQL from
// rusqlite_migration) is written as one header line plus bare continuation
// lines that do not match the regex. Those continuations are joined into the
// preceding record so they inherit its time and severity instead of showing
// up as bogus INF rows with an empty timestamp.

export type Tag = "ERR" | "WRN" | "INF" | "DBG" | "TRC";

export interface LogLine {
  id: number;
  tag: Tag;
  time: string;
  /** Log target/module from the third bracket — shown muted after the time. */
  target: string;
  message: string;
  live: boolean;
  raw: string;
}

export const RECORD_HEADER =
  /^\[([^\]]+)\]\[([^\]]+)\]\[([^\]]*)\]\[(\w+)\]\s?(.*)$/;

export const tagFromLevel = (level: string): Tag => {
  const u = level.toUpperCase().slice(0, 4);
  if (u === "WARN") return "WRN";
  if (u === "ERRO") return "ERR";
  if (u === "TRAC") return "TRC";
  if (u === "DEBU") return "DBG";
  return "INF";
};

/** Parse a file tail into complete records (header + folded continuations). */
export const parseLogRecords = (data: string): LogLine[] => {
  const out: LogLine[] = [];
  for (const raw of data.split("\n")) {
    if (!raw.trim()) continue;
    const m = raw.match(RECORD_HEADER);
    if (m) {
      out.push({
        id: 0,
        tag: tagFromLevel(m[4] ?? ""),
        time: m[2] ?? "",
        target: m[3] ?? "",
        // A missing group must not surface as the literal word "undefined".
        message: m[5] ?? "",
        live: false,
        raw,
      });
      continue;
    }
    const last = out[out.length - 1];
    if (last) {
      // Continuation of the previous record: keep its time/tag, append text.
      last.message = `${last.message}\n${raw}`;
      last.raw = `${last.raw}\n${raw}`;
    } else {
      // Orphan continuation with no header in the tail window — keep the text
      // but do not fabricate a severity chip for it.
      out.push({
        id: 0,
        tag: "INF",
        time: "",
        target: "",
        message: raw,
        live: false,
        raw,
      });
    }
  }
  return out;
};
