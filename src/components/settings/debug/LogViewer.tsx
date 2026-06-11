import React, { useEffect, useState, useRef } from "react";
import { useTranslation } from "react-i18next";
import { commands } from "@/bindings";
import { toast } from "sonner";
import { SettingsGroup } from "../../ui/SettingsGroup";

type LogSeverity = "all" | "error" | "warn" | "info" | "debug" | "trace";

interface LogLine {
  raw: string;
  severity: LogSeverity;
  timestamp: string;
  message: string;
}

export const LogViewer: React.FC = () => {
  const { t } = useTranslation();
  const [rawLogs, setRawLogs] = useState<string>("");
  const [linesLimit, setLinesLimit] = useState<number>(200);
  const [searchQuery, setSearchQuery] = useState<string>("");
  const [severityFilter, setSeverityFilter] = useState<LogSeverity>("all");
  const [autoRefresh, setAutoRefresh] = useState<boolean>(true);
  const [loading, setLoading] = useState<boolean>(false);

  const consoleEndRef = useRef<HTMLDivElement>(null);
  const consoleContainerRef = useRef<HTMLDivElement>(null);

  const fetchLogs = async (silent = false) => {
    if (!silent) setLoading(true);
    try {
      const res = await commands.getRecentLogs(linesLimit);
      if (res.status === "ok") {
        setRawLogs(res.data);
      } else {
        console.error("Failed to fetch logs:", res.error);
      }
    } catch (err) {
      console.error("Error fetching logs:", err);
    } finally {
      if (!silent) setLoading(false);
    }
  };

  // Poll for logs if auto-refresh is enabled
  useEffect(() => {
    fetchLogs();
    let interval: ReturnType<typeof setInterval> | null = null;
    if (autoRefresh) {
      interval = setInterval(() => {
        fetchLogs(true);
      }, 2000);
    }
    return () => {
      if (interval) clearInterval(interval);
    };
  }, [autoRefresh, linesLimit]);

  // Scroll to bottom on load or when logs change (only if user was already near bottom)
  useEffect(() => {
    if (consoleContainerRef.current) {
      const { scrollTop, scrollHeight, clientHeight } = consoleContainerRef.current;
      const isNearBottom = scrollHeight - scrollTop - clientHeight < 100;
      if (isNearBottom) {
        consoleEndRef.current?.scrollIntoView({ behavior: "smooth" });
      }
    }
  }, [rawLogs]);

  // Parse raw log lines into structured logs
  const parseLogs = (raw: string): LogLine[] => {
    if (!raw.trim()) return [];
    return raw.split("\n").map((line) => {
      let severity: LogSeverity = "info";
      let timestamp = "";
      let message = line;

      // Extract severity if present (e.g. "[INFO]" or "[DEBUG]")
      const upperLine = line.toUpperCase();
      if (upperLine.includes("[ERROR]") || upperLine.includes(" ERROR ")) {
        severity = "error";
      } else if (upperLine.includes("[WARN]") || upperLine.includes(" WARN ")) {
        severity = "warn";
      } else if (upperLine.includes("[DEBUG]") || upperLine.includes(" DEBUG ")) {
        severity = "debug";
      } else if (upperLine.includes("[TRACE]") || upperLine.includes(" TRACE ")) {
        severity = "trace";
      } else if (upperLine.includes("[INFO]") || upperLine.includes(" INFO ")) {
        severity = "info";
      }

      // Simple regex check for timestamps
      const tsMatch = line.match(/^\[([^\]]+)\]/);
      if (tsMatch) {
        timestamp = tsMatch[1];
        message = line.substring(tsMatch[0].length).trim();
      }

      return { raw: line, severity, timestamp, message };
    });
  };

  const logLines = parseLogs(rawLogs);

  // Filter logs based on severity and search query
  const filteredLines = logLines.filter((line) => {
    const matchesSeverity =
      severityFilter === "all" || line.severity === severityFilter;
    const matchesSearch =
      searchQuery === "" ||
      line.raw.toLowerCase().includes(searchQuery.toLowerCase());
    return matchesSeverity && matchesSearch;
  });

  const handleClearLogs = async () => {
    if (confirm("Are you sure you want to truncate the log files?")) {
      try {
        const res = await commands.clearLogs();
        if (res.status === "ok") {
          toast.success("Log files cleared successfully");
          setRawLogs("");
          fetchLogs();
        } else {
          toast.error(`Failed to clear logs: ${res.error}`);
        }
      } catch (err) {
        toast.error("Failed to clear logs");
      }
    }
  };

  const handleCopyLogs = () => {
    const textToCopy = filteredLines.map((l) => l.raw).join("\n");
    if (!textToCopy) {
      toast.warning("No logs to copy");
      return;
    }
    navigator.clipboard.writeText(textToCopy);
    toast.success("Logs copied to clipboard");
  };

  const getLineColorClass = (severity: LogSeverity) => {
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

  const getSeverityBadge = (severity: LogSeverity) => {
    switch (severity) {
      case "error":
        return <span className="text-[9px] bg-red-950 text-red-400 px-1 py-0.5 rounded border border-red-900/50">ERR</span>;
      case "warn":
        return <span className="text-[9px] bg-yellow-950 text-yellow-400 px-1 py-0.5 rounded border border-yellow-900/50">WRN</span>;
      case "debug":
        return <span className="text-[9px] bg-zinc-900 text-zinc-400 px-1 py-0.5 rounded border border-zinc-800/50">DBG</span>;
      case "trace":
        return <span className="text-[9px] bg-zinc-950 text-zinc-500 px-1 py-0.5 rounded border border-zinc-900/50">TRC</span>;
      case "info":
      default:
        return <span className="text-[9px] bg-blue-950 text-blue-400 px-1 py-0.5 rounded border border-blue-900/50">INF</span>;
    }
  };

  const severityOptions: { value: LogSeverity; label: string }[] = [
    { value: "all", label: "All Levels" },
    { value: "error", label: "Error" },
    { value: "warn", label: "Warn" },
    { value: "info", label: "Info" },
    { value: "debug", label: "Debug" },
    { value: "trace", label: "Trace" },
  ];

  return (
    <SettingsGroup title="App Diagnostics & Logs">
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
              <option value={50}>{t("debug.logViewer.lastLines", { count: 50 })}</option>
              <option value={100}>{t("debug.logViewer.lastLines", { count: 100 })}</option>
              <option value={200}>{t("debug.logViewer.lastLines", { count: 200 })}</option>
              <option value={500}>{t("debug.logViewer.lastLines", { count: 500 })}</option>
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
              onClick={() => fetchLogs(false)}
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
              onClick={handleCopyLogs}
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
              onClick={handleClearLogs}
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
              {rawLogs.trim() ? t("debug.logViewer.noFilterMatch") : t("debug.logViewer.noEntries")}
            </div>
          ) : (
            filteredLines.map((line, idx) => (
              <div
                key={idx}
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
            {t("debug.logViewer.showing", { filtered: filteredLines.length, total: logLines.length })}
          </span>
          <span>
            {t("debug.logViewer.levelFilter", { level: severityFilter.toUpperCase() })}
          </span>
        </div>
      </div>
    </SettingsGroup>
  );
};
