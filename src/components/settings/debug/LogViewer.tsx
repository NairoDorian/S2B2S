import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { commands } from "@/bindings";
import { toast } from "sonner";
import { SettingsGroup } from "../../ui/SettingsGroup";

type LogSeverity = "all" | "error" | "warn" | "info" | "debug" | "trace";
type LineSeverity = Exclude<LogSeverity, "all">;

interface LogLine {
  raw: string;
  severity: LineSeverity;
  timestamp: string;
  message: string;
  live: boolean;
}

interface LogEventPayload {
  message: string;
  level: number;
}

// Hard cap for the in-memory console so a chatty app can never balloon memory.
const MAX_LINES = 2000;

// Payload emitted by tauri-plugin-log's `Webview` target on the `log://log`
// event. `level` is the numeric LogLevel repr: Trace=1, Debug=2, Info=3,
// Warn=4, Error=5.
const LIVE_LEVEL_TO_SEVERITY: Record<number, LineSeverity> = {
  1: "trace",
  2: "debug",
  3: "info",
  4: "warn",
  5: "error",
};

const severityFromText = (line: string): LineSeverity => {
  const upper = line.toUpperCase();
  if (upper.includes("[ERROR]") || upper.includes(" ERROR ")) return "error";
  if (upper.includes("[WARN]") || upper.includes(" WARN ")) return "warn";
  if (upper.includes("[DEBUG]") || upper.includes(" DEBUG ")) return "debug";
  if (upper.includes("[TRACE]") || upper.includes(" TRACE ")) return "trace";
  return "info";
};

// File lines look like:
//   [2026-08-15][00:04:36][s2b2s_app_lib::managers::history][DEBUG] message
const parseFileLine = (raw: string): LogLine => {
  const severity = severityFromText(raw);
  const tsMatch = raw.match(/^\[([^\]]+)\]/);
  const timestamp = tsMatch ? tsMatch[1] : "";
  const message = tsMatch ? raw.substring(tsMatch[0].length).trim() : raw;
  return { raw, severity, timestamp, message, live: false };
};

const formatClock = (date: Date): string => {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(
    date.getSeconds(),
  )}`;
};

const liveLineFrom = (payload: LogEventPayload): LogLine => {
  const severity = LIVE_LEVEL_TO_SEVERITY[payload.level] ?? "info";
  const timestamp = formatClock(new Date());
  return {
    raw: `[${timestamp}] ${payload.message}`,
    severity,
    timestamp,
    message: payload.message,
    live: true,
  };
};

export const LogViewer: React.FC = () => {
  const { t } = useTranslation();
  const [lines, setLinesState] = useState<LogLine[]>([]);
  const [linesLimit, setLinesLimit] = useState<number>(200);
  const [searchQuery, setSearchQuery] = useState<string>("");
  const [severityFilter, setSeverityFilter] = useState<LogSeverity>("all");
  const [autoRefresh, setAutoRefresh] = useState<boolean>(true);
  const [paused, setPaused] = useState<boolean>(false);
  const [loading, setLoading] = useState<boolean>(false);

  const pausedRef = useRef(false);
  const pendingRef = useRef<LogLine[]>([]);
  const consoleEndRef = useRef<HTMLDivElement>(null);
  const consoleContainerRef = useRef<HTMLDivElement>(null);

  // Live events append unconditionally (they are real log emissions). The next
  // file refresh reconciles any overlap with the on-disk log file.
  const appendLiveLines = useCallback((incoming: LogLine[]) => {
    if (incoming.length === 0) return;
    setLinesState((prev) => {
      const next = prev.concat(incoming);
      return next.length > MAX_LINES
        ? next.slice(next.length - MAX_LINES)
        : next;
    });
  }, []);

  // Merge a fresh batch of file lines into the console:
  //   1. Drop live-streamed lines whose on-disk counterpart arrived.
  //   2. Append file lines that are not already present (count-aware, so
  //      repeated identical lines survive).
  const mergeFileLines = useCallback((fetched: LogLine[]) => {
    setLinesState((prev) => {
      const rawCounts = new Map<string, number>();
      for (const l of prev) {
        rawCounts.set(l.raw, (rawCounts.get(l.raw) ?? 0) + 1);
      }
      const supersededLive = new Set<LogLine>();
      for (const l of prev) {
        if (!l.live) continue;
        if (
          fetched.some(
            (f) => f.severity === l.severity && f.message.includes(l.message),
          )
        ) {
          supersededLive.add(l);
        }
      }
      const out: LogLine[] = [];
      for (const l of prev) {
        if (!supersededLive.has(l)) out.push(l);
      }
      for (const f of fetched) {
        const c = rawCounts.get(f.raw) ?? 0;
        if (c > 0) {
          rawCounts.set(f.raw, c - 1);
          continue;
        }
        out.push(f);
      }
      return out.length > MAX_LINES ? out.slice(out.length - MAX_LINES) : out;
    });
  }, []);

  const fetchLogs = useCallback(
    async (silent = false) => {
      if (!silent) setLoading(true);
      try {
        const res = await commands.getRecentLogs(linesLimit);
        if (res.status === "ok") {
          const fetched = res.data
            .split("\n")
            .filter((l) => l.trim().length > 0)
            .map(parseFileLine);
          mergeFileLines(fetched);
        } else {
          console.error("Failed to fetch logs:", res.error);
        }
      } catch (err) {
        console.error("Error fetching logs:", err);
      } finally {
        if (!silent) setLoading(false);
      }
    },
    [linesLimit, mergeFileLines],
  );

  // Poll the log file if auto-refresh is enabled. The live event stream below
  // still provides real-time appends between polls.
  useEffect(() => {
    void fetchLogs();
    let interval: ReturnType<typeof setInterval> | null = null;
    if (autoRefresh) {
      interval = setInterval(() => {
        if (!pausedRef.current) void fetchLogs(true);
      }, 2000);
    }
    return () => {
      if (interval) clearInterval(interval);
    };
  }, [autoRefresh, fetchLogs]);

  // Subscribe to the backend log stream so new lines appear instantly without
  // waiting for the next file poll.
  useEffect(() => {
    const unlistenPromise = listen<LogEventPayload>("log://log", (event) => {
      const line = liveLineFrom(event.payload);
      if (pausedRef.current) {
        pendingRef.current.push(line);
        if (pendingRef.current.length > MAX_LINES) pendingRef.current.shift();
        return;
      }
      appendLiveLines([line]);
    });
    return () => {
      void unlistenPromise.then((fn) => fn());
    };
  }, [appendLiveLines]);

  // Flush buffered live lines and re-sync with the file when unpausing.
  useEffect(() => {
    pausedRef.current = paused;
    if (!paused) {
      if (pendingRef.current.length > 0) {
        const buffered = pendingRef.current;
        pendingRef.current = [];
        appendLiveLines(buffered);
      }
      void fetchLogs(true);
    }
  }, [paused, appendLiveLines, fetchLogs]);

  // Keep the view pinned to the latest line unless the user has scrolled up.
  useEffect(() => {
    if (consoleContainerRef.current) {
      const { scrollTop, scrollHeight, clientHeight } =
        consoleContainerRef.current;
      const isNearBottom = scrollHeight - scrollTop - clientHeight < 100;
      if (isNearBottom) {
        consoleEndRef.current?.scrollIntoView({ behavior: "auto" });
      }
    }
  }, [lines]);

  const filteredLines = lines.filter((line) => {
    const matchesSeverity =
      severityFilter === "all" || line.severity === severityFilter;
    const matchesSearch =
      searchQuery === "" ||
      line.raw.toLowerCase().includes(searchQuery.toLowerCase());
    return matchesSeverity && matchesSearch;
  });

  const handleClearLogs = async () => {
    if (confirm(t("debug.logViewer.clearConfirm"))) {
      try {
        const res = await commands.clearLogs();
        if (res.status === "ok") {
          toast.success(t("debug.logViewer.clearSuccess"));
          pendingRef.current = [];
          setLinesState([]);
        } else {
          toast.error(t("debug.logViewer.clearFailure", { error: res.error }));
        }
      } catch (err) {
        console.error("Failed to clear logs:", err);
        toast.error(t("debug.logViewer.clearFailed"));
      }
    }
  };

  const handleCopyLogs = async () => {
    const textToCopy = filteredLines.map((l) => l.raw).join("\n");
    if (!textToCopy) {
      toast.warning(t("debug.logViewer.copyEmpty"));
      return;
    }
    try {
      await navigator.clipboard.writeText(textToCopy);
      toast.success(t("debug.logViewer.copySuccess"));
    } catch (err) {
      console.error("Failed to copy logs:", err);
      toast.error(t("debug.logViewer.copyFailed"));
    }
  };

  const getLineColorClass = (severity: LineSeverity) => {
    switch (severity) {
      case "error":
        return "text-red-400 font-medium";
      case "warn":
        return "text-yellow-400";
      case "debug":
        return "text-zinc-500";
      case "trace":
        return "text-zinc-600";
      case "info":
      default:
        return "text-zinc-300";
    }
  };

  const getSeverityBadge = (severity: LineSeverity) => {
    switch (severity) {
      case "error":
        return (
          <span className="text-[9px] bg-red-950 text-red-400 px-1 py-0.5 rounded border border-red-900/50">
            ERR
          </span>
        );
      case "warn":
        return (
          <span className="text-[9px] bg-yellow-950 text-yellow-400 px-1 py-0.5 rounded border border-yellow-900/50">
            WRN
          </span>
        );
      case "debug":
        return (
          <span className="text-[9px] bg-zinc-900 text-zinc-400 px-1 py-0.5 rounded border border-zinc-800/50">
            DBG
          </span>
        );
      case "trace":
        return (
          <span className="text-[9px] bg-zinc-950 text-zinc-500 px-1 py-0.5 rounded border border-zinc-900/50">
            TRC
          </span>
        );
      case "info":
      default:
        return (
          <span className="text-[9px] bg-blue-950 text-blue-400 px-1 py-0.5 rounded border border-blue-900/50">
            INF
          </span>
        );
    }
  };

  const severityOptions: { value: LogSeverity; label: string }[] = [
    { value: "all", label: t("debug.logViewer.severityAll") },
    { value: "error", label: t("debug.logViewer.severityError") },
    { value: "warn", label: t("debug.logViewer.severityWarn") },
    { value: "info", label: t("debug.logViewer.severityInfo") },
    { value: "debug", label: t("debug.logViewer.severityDebug") },
    { value: "trace", label: t("debug.logViewer.severityTrace") },
  ];

  return (
    <SettingsGroup title={t("debug.logViewer.title")}>
      <div className="flex flex-col gap-3">
        {/* Controls Bar */}
        <div className="flex flex-wrap gap-2 items-center justify-between bg-mid-gray/5 p-3 border border-mid-gray/10 rounded-lg">
          {/* Filters */}
          <div className="flex flex-wrap gap-2 items-center">
            <select
              value={severityFilter}
              onChange={(e) => setSeverityFilter(e.target.value as LogSeverity)}
              className="bg-background border border-mid-gray/20 rounded px-2.5 py-1 text-xs text-text/80 focus:outline-none focus:border-logo-primary/50 cursor-pointer"
            >
              {severityOptions.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {opt.label}
                </option>
              ))}
            </select>

            <select
              value={linesLimit}
              onChange={(e) => setLinesLimit(Number(e.target.value))}
              className="bg-background border border-mid-gray/20 rounded px-2.5 py-1 text-xs text-text/80 focus:outline-none focus:border-logo-primary/50 cursor-pointer"
            >
              <option value={50}>
                {t("debug.logViewer.lastLines", { count: 50 })}
              </option>
              <option value={100}>
                {t("debug.logViewer.lastLines", { count: 100 })}
              </option>
              <option value={200}>
                {t("debug.logViewer.lastLines", { count: 200 })}
              </option>
              <option value={500}>
                {t("debug.logViewer.lastLines", { count: 500 })}
              </option>
            </select>

            <input
              type="text"
              placeholder={t("debug.logViewer.searchPlaceholder")}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="bg-background border border-mid-gray/20 rounded px-2.5 py-1 text-xs text-text/80 focus:outline-none focus:border-logo-primary/50 w-44"
            />
          </div>

          {/* Actions */}
          <div className="flex gap-2 items-center">
            <div className="flex items-center gap-1.5 text-xs text-mid-gray mr-1">
              <span
                className={`inline-block w-2 h-2 rounded-full shrink-0 ${
                  paused ? "bg-mid-gray" : "bg-emerald-500 animate-pulse"
                }`}
              />
              <span className="shrink-0">
                {paused
                  ? t("debug.logViewer.paused")
                  : t("debug.logViewer.live")}
              </span>
              <span className="shrink-0">·</span>
              <span className="shrink-0">
                {t("debug.logViewer.lineCount", { count: lines.length })}
              </span>
            </div>

            <button
              onClick={() => setPaused((p) => !p)}
              className="px-2.5 py-1 rounded border border-mid-gray/20 text-xs text-text/80 hover:bg-mid-gray/10 transition-colors cursor-pointer"
            >
              {paused
                ? t("debug.logViewer.resume")
                : t("debug.logViewer.pause")}
            </button>

            <label className="flex items-center gap-1.5 text-xs text-text/60 cursor-pointer select-none">
              <input
                type="checkbox"
                checked={autoRefresh}
                onChange={(e) => setAutoRefresh(e.target.checked)}
                className="rounded border-mid-gray/30 text-logo-primary focus:ring-0 cursor-pointer"
              />
              <span>{t("debug.logViewer.autoRefresh")}</span>
            </label>

            <button
              onClick={() => void fetchLogs(false)}
              disabled={loading}
              className="px-2.5 py-1 rounded border border-mid-gray/20 text-xs text-text/80 hover:bg-mid-gray/10 transition-colors flex items-center gap-1 cursor-pointer disabled:opacity-50"
            >
              <svg
                className={`w-3 h-3 ${loading ? "animate-spin" : ""}`}
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M4 4v5h.582m15.356 2A8.001 8.001 0 1121.21 8H18.2"
                />
              </svg>
              <span>{t("debug.logViewer.refresh")}</span>
            </button>

            <button
              onClick={() => void handleCopyLogs()}
              className="px-2.5 py-1 rounded border border-mid-gray/20 text-xs text-text/80 hover:bg-mid-gray/10 transition-colors flex items-center gap-1 cursor-pointer"
            >
              <svg
                className="w-3.5 h-3.5"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M8 5H6a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2v-1M8 5a2 2 0 002 2h2a2 2 0 002-2M8 5a2 2 0 012-2h2a2 2 0 012 2m0 0h2a2 2 0 012 2v3m2 4H10m0 0l3-3m-3 3l3 3"
                />
              </svg>
              <span>{t("debug.logViewer.copy")}</span>
            </button>

            <button
              onClick={() => void handleClearLogs()}
              className="px-2.5 py-1 rounded border border-red-500/20 text-xs text-red-400 hover:bg-red-500/10 transition-colors flex items-center gap-1 cursor-pointer"
            >
              <svg
                className="w-3.5 h-3.5"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                />
              </svg>
              <span>{t("debug.logViewer.clear")}</span>
            </button>
          </div>
        </div>

        {/* Log Stream Terminal */}
        <div
          ref={consoleContainerRef}
          className="bg-black border border-zinc-800 rounded-lg p-3 h-80 overflow-y-auto font-mono text-[11px] leading-relaxed shadow-inner flex flex-col gap-1.5"
        >
          {filteredLines.length === 0 ? (
            <div className="text-zinc-600 text-center py-10 italic">
              {lines.length > 0
                ? t("debug.logViewer.noFilterMatch")
                : t("debug.logViewer.noEntries")}
            </div>
          ) : (
            filteredLines.map((line, idx) => (
              <div
                key={`${line.raw}-${idx}`}
                className={`flex gap-2 items-start hover:bg-zinc-900/50 p-0.5 rounded transition-colors whitespace-pre-wrap break-all ${getLineColorClass(
                  line.severity,
                )}`}
              >
                {/* Timestamp */}
                {line.timestamp && (
                  <span className="text-zinc-600 select-none flex-shrink-0">
                    [{line.timestamp}]
                  </span>
                )}

                {/* Severity Badge */}
                <span className="flex-shrink-0 select-none">
                  {getSeverityBadge(line.severity)}
                </span>

                {/* Message */}
                <span className="flex-1 select-text selection:bg-logo-primary/30 selection:text-white">
                  {line.message}
                </span>
              </div>
            ))
          )}
          <div ref={consoleEndRef} />
        </div>

        {/* Counter Summary */}
        <div className="text-[10px] text-text/40 flex justify-between px-1">
          <span>
            {t("debug.logViewer.showing", {
              filtered: filteredLines.length,
              total: lines.length,
            })}
          </span>
          <span>
            {t("debug.logViewer.levelFilter", {
              level: severityFilter.toUpperCase(),
            })}
          </span>
        </div>
      </div>
    </SettingsGroup>
  );
};
