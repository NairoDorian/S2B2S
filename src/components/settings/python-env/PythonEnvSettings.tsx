import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import {
  AlertCircle,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Download,
  FolderOpen,
  Loader2,
  Package,
  RefreshCw,
  Terminal,
  Zap,
} from "lucide-react";
import { commands } from "@/bindings";
import type { BackendStatus, PythonEnvStatus } from "@/bindings";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface LogLine {
  id: number;
  context: string;
  line: string;
  level: "info" | "warn" | "error";
}

type OperationState = "idle" | "running" | "done" | "error";

// ---------------------------------------------------------------------------
// Small UI helpers
// ---------------------------------------------------------------------------

const StatusDot: React.FC<{
  installed: boolean;
  loading?: boolean;
}> = ({ installed, loading }) => {
  if (loading) {
    return <Loader2 className="w-4 h-4 text-amber-400 animate-spin shrink-0" />;
  }
  if (installed) {
    return <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />;
  }
  return <AlertCircle className="w-4 h-4 text-amber-400 shrink-0" />;
};

const LevelColor: Record<string, string> = {
  info: "text-zinc-300",
  warn: "text-amber-400",
  error: "text-red-400",
};

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export const PythonEnvSettings: React.FC = () => {
  const { t } = useTranslation();

  // ── State ──────────────────────────────────────────────────────────────
  const [status, setStatus] = useState<PythonEnvStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [gpu, setGpu] = useState(true);
  const [opState, setOpState] = useState<OperationState>("idle");
  const [activeOp, setActiveOp] = useState<string | null>(null);
  const [busyBackend, setBusyBackend] = useState<string | null>(null);
  const [logs, setLogs] = useState<LogLine[]>([]);
  const [logExpanded, setLogExpanded] = useState(true);
  const logRef = useRef<HTMLDivElement>(null);
  const logCounter = useRef(0);

  // ── Log append ──────────────────────────────────────────────────────────
  const pushLog = useCallback(
    (context: string, line: string, level: string = "info") => {
      const id = ++logCounter.current;
      setLogs((prev) => {
        const next = [
          ...prev,
          { id, context, line, level: level as LogLine["level"] },
        ];
        return next.length > 300 ? next.slice(next.length - 300) : next;
      });
    },
    [],
  );

  // Auto-scroll log
  useEffect(() => {
    if (logRef.current) {
      logRef.current.scrollTop = logRef.current.scrollHeight;
    }
  }, [logs]);

  // ── Event listener ──────────────────────────────────────────────────────
  useEffect(() => {
    const unlisten = listen<{ context: string; line: string; level: string }>(
      "python-env-progress",
      (e) => {
        pushLog(e.payload.context, e.payload.line, e.payload.level);
      },
    );

    const unlistenStatus = listen<PythonEnvStatus>("python-env-status", (e) => {
      setStatus(e.payload);
    });

    return () => {
      unlisten.then((fn) => fn());
      unlistenStatus.then((fn) => fn());
    };
  }, [pushLog]);

  // ── Load status on mount ────────────────────────────────────────────────
  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const s = await commands.getPythonEnvStatus();
      setStatus(s);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // ── Generic operation runner ────────────────────────────────────────────
  const runOp = useCallback(
    async (label: string, fn: () => Promise<unknown>) => {
      setOpState("running");
      setActiveOp(label);
      setLogs([]);
      pushLog("system", `▶ ${label}`, "info");
      try {
        await fn();
        pushLog("system", "✅ Done", "info");
        setOpState("done");
        await refresh();
      } catch (err) {
        pushLog("system", `❌ ${String(err)}`, "error");
        setOpState("error");
      } finally {
        setActiveOp(null);
        setBusyBackend(null);
      }
    },
    [pushLog, refresh],
  );

  const busy = opState === "running";

  // ── Actions ─────────────────────────────────────────────────────────────
  const handleInstallUv = () => runOp("Install uv", () => commands.installUv());

  const handleCreateVenv = () =>
    runOp("Create venv (Python 3.12)", () => commands.createPythonVenv());

  const handleInstallBackend = (id: string) => {
    setBusyBackend(id);
    runOp(`Install ${id}`, () => commands.setupBackend(id, gpu));
  };

  const handleInstallAll = () =>
    runOp("Install all backends", () => commands.setupAllBackends(gpu));

  const handleFullGpu = () =>
    runOp("Full GPU setup", () => commands.fullGpuSetup());

  const handleOpenFolder = () => commands.openVenvFolder();

  // ── Render ───────────────────────────────────────────────────────────────
  const uvInstalled = !!status?.uv_version;
  const venvOk = !!status?.venv_exists && !!status?.python_version;

  const ttsBackends =
    status?.backends.filter((b) => b.category === "tts") ?? [];
  const sttBackends =
    status?.backends.filter((b) => b.category === "stt") ?? [];

  return (
    <div className="w-full max-w-2xl flex flex-col gap-4">
      {/* ── Header ───────────────────────────────────────────────────── */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Terminal className="w-5 h-5 text-logo-primary" />
          <h2 className="text-base font-semibold text-white">
            {t("settings.pythonEnv.title")}
          </h2>
        </div>
        <button
          onClick={refresh}
          disabled={loading || busy}
          className="p-1.5 rounded-md hover:bg-white/10 text-zinc-400 hover:text-white transition-colors disabled:opacity-40"
          title="Refresh status"
        >
          <RefreshCw className={`w-4 h-4 ${loading ? "animate-spin" : ""}`} />
        </button>
      </div>

      {/* ── Environment Info Card ─────────────────────────────────────── */}
      <div className="bg-white/5 border border-white/10 rounded-xl p-4 flex flex-col gap-3">
        {/* uv row */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <StatusDot installed={uvInstalled} loading={loading} />
            <span className="text-sm text-zinc-300">
              {t("settings.pythonEnv.uvManager")}
            </span>
          </div>
          <div className="flex items-center gap-2">
            {status?.uv_version && (
              <span className="text-xs text-zinc-500 font-mono">
                {status.uv_version}
              </span>
            )}
            {!uvInstalled && !loading && (
              <button
                onClick={handleInstallUv}
                disabled={busy}
                className="flex items-center gap-1.5 px-3 py-1 rounded-md bg-logo-primary/20 hover:bg-logo-primary/40 text-logo-primary text-xs font-medium transition-colors disabled:opacity-40"
              >
                <Download className="w-3 h-3" />
                {t("settings.pythonEnv.installUv")}
              </button>
            )}
          </div>
        </div>

        {/* Python / venv row */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <StatusDot installed={venvOk} loading={loading} />
            <span className="text-sm text-zinc-300">
              {t("settings.pythonEnv.venv")}
            </span>
          </div>
          <div className="flex items-center gap-2">
            {status?.python_version && (
              <span className="text-xs text-zinc-500 font-mono">
                {status.python_version}
              </span>
            )}
            <button
              onClick={handleCreateVenv}
              disabled={busy || !uvInstalled}
              className="flex items-center gap-1.5 px-3 py-1 rounded-md bg-white/8 hover:bg-white/15 text-zinc-300 text-xs font-medium transition-colors disabled:opacity-40"
              title={!uvInstalled ? "Install uv first" : "Recreate venv"}
            >
              <RefreshCw className="w-3 h-3" />
              {venvOk ? "Recreate" : "Create"}
            </button>
          </div>
        </div>

        {/* Venv path row */}
        {status?.venv_path && (
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2 min-w-0">
              <FolderOpen className="w-4 h-4 text-zinc-500 shrink-0" />
              <span
                className="text-xs text-zinc-500 font-mono truncate"
                title={status.venv_path}
              >
                {status.venv_path}
              </span>
            </div>
            <button
              onClick={handleOpenFolder}
              className="px-2 py-1 text-xs text-zinc-400 hover:text-white rounded-md hover:bg-white/10 transition-colors shrink-0 ml-2"
            >
              {t("settings.pythonEnv.open")}
            </button>
          </div>
        )}

        {/* GPU / CPU toggle */}
        <div className="flex items-center justify-between pt-1 border-t border-white/8">
          <div className="flex items-center gap-2">
            <Zap className="w-4 h-4 text-amber-400" />
            <span className="text-sm text-zinc-300">
              {t("settings.pythonEnv.acceleration")}
            </span>
          </div>
          <div className="flex gap-1 bg-white/8 rounded-lg p-0.5">
            <button
              onClick={() => setGpu(false)}
              className={`px-3 py-1 rounded-md text-xs font-medium transition-all ${
                !gpu
                  ? "bg-white/20 text-white"
                  : "text-zinc-400 hover:text-zinc-200"
              }`}
            >
              CPU
            </button>
            <button
              onClick={() => setGpu(true)}
              className={`px-3 py-1 rounded-md text-xs font-medium transition-all ${
                gpu
                  ? "bg-amber-500/30 text-amber-300"
                  : "text-zinc-400 hover:text-zinc-200"
              }`}
            >
              CUDA
            </button>
          </div>
        </div>
      </div>

      {/* ── TTS Backends ─────────────────────────────────────────────── */}
      <BackendTable
        title="TTS Backends"
        backends={ttsBackends}
        busy={busy}
        venvOk={venvOk}
        busyBackend={busyBackend}
        onInstall={handleInstallBackend}
        loading={loading}
      />

      {/* ── STT Backends ─────────────────────────────────────────────── */}
      <BackendTable
        title="STT Backends"
        backends={sttBackends}
        busy={busy}
        venvOk={venvOk}
        busyBackend={busyBackend}
        onInstall={handleInstallBackend}
        loading={loading}
      />

      {/* ── Bulk actions ─────────────────────────────────────────────── */}
      <div className="flex gap-2 flex-wrap">
        <BulkBtn
          label="Install All"
          icon={<Package className="w-3.5 h-3.5" />}
          onClick={handleInstallAll}
          disabled={busy || !venvOk}
          variant="primary"
        />
        <BulkBtn
          label="Full GPU Setup"
          icon={<Zap className="w-3.5 h-3.5" />}
          onClick={handleFullGpu}
          disabled={busy}
          variant="gpu"
        />
      </div>

      {/* ── Live log ─────────────────────────────────────────────────── */}
      {(logs.length > 0 || busy) && (
        <div className="bg-black/40 border border-white/8 rounded-xl overflow-hidden">
          {/* Log header */}
          <button
            onClick={() => setLogExpanded((v) => !v)}
            className="w-full flex items-center justify-between px-3 py-2 hover:bg-white/5 transition-colors"
          >
            <div className="flex items-center gap-2">
              <Terminal className="w-3.5 h-3.5 text-zinc-500" />
              <span className="text-xs font-medium text-zinc-400">
                {activeOp ?? "Log"}
              </span>
              {busy && (
                <Loader2 className="w-3 h-3 text-amber-400 animate-spin" />
              )}
              {opState === "done" && (
                <CheckCircle2 className="w-3 h-3 text-emerald-400" />
              )}
              {opState === "error" && (
                <AlertCircle className="w-3 h-3 text-red-400" />
              )}
            </div>
            {logExpanded ? (
              <ChevronDown className="w-3.5 h-3.5 text-zinc-500" />
            ) : (
              <ChevronRight className="w-3.5 h-3.5 text-zinc-500" />
            )}
          </button>

          {/* Log body */}
          {logExpanded && (
            <div
              ref={logRef}
              className="max-h-48 overflow-y-auto px-3 pb-3 flex flex-col gap-0.5 font-mono text-[11px] leading-relaxed"
            >
              {logs.map((l) => (
                <div key={l.id} className={`flex gap-2 ${LevelColor[l.level]}`}>
                  <span className="text-zinc-600 shrink-0 select-none">
                    [{l.context}]
                  </span>
                  <span className="break-all">{l.line}</span>
                </div>
              ))}
              {busy && (
                <div className="flex gap-1 items-center text-zinc-500 mt-1">
                  <Loader2 className="w-3 h-3 animate-spin" />
                  <span>{t("settings.pythonEnv.running")}</span>
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
};

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

const BackendTable: React.FC<{
  title: string;
  backends: BackendStatus[];
  busy: boolean;
  venvOk: boolean;
  busyBackend: string | null;
  onInstall: (id: string) => void;
  loading: boolean;
}> = ({ title, backends, busy, venvOk, busyBackend, onInstall, loading }) => {
  if (backends.length === 0) return null;

  return (
    <div className="bg-white/5 border border-white/10 rounded-xl overflow-hidden">
      <div className="px-4 py-2.5 border-b border-white/8 bg-white/3">
        <h3 className="text-xs font-semibold text-zinc-400 uppercase tracking-wider">
          {title}
        </h3>
      </div>
      <div className="divide-y divide-white/5">
        {backends.map((b) => {
          const isThisBusy = busyBackend === b.id;
          return (
            <div
              key={b.id}
              className="flex items-center justify-between px-4 py-3 hover:bg-white/3 transition-colors"
            >
              <div className="flex items-center gap-3">
                <StatusDot
                  installed={b.installed}
                  loading={loading || isThisBusy}
                />
                <div>
                  <p className="text-sm text-zinc-200 font-medium">{b.label}</p>
                  <p className="text-xs text-zinc-500">
                    {b.installed ? "Ready" : "Not installed"}
                  </p>
                </div>
              </div>
              <button
                onClick={() => onInstall(b.id)}
                disabled={busy || !venvOk}
                className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all disabled:opacity-40 ${
                  b.installed
                    ? "bg-white/8 hover:bg-white/15 text-zinc-300"
                    : "bg-logo-primary/20 hover:bg-logo-primary/35 text-logo-primary"
                }`}
              >
                {isThisBusy ? (
                  <Loader2 className="w-3 h-3 animate-spin" />
                ) : b.installed ? (
                  <RefreshCw className="w-3 h-3" />
                ) : (
                  <Download className="w-3 h-3" />
                )}
                {b.installed ? "Reinstall" : "Install"}
              </button>
            </div>
          );
        })}
      </div>
    </div>
  );
};

const BulkBtn: React.FC<{
  label: string;
  icon: React.ReactNode;
  onClick: () => void;
  disabled: boolean;
  variant: "primary" | "gpu";
}> = ({ label, icon, onClick, disabled, variant }) => {
  const cls =
    variant === "gpu"
      ? "bg-amber-500/15 hover:bg-amber-500/30 text-amber-300 border border-amber-500/25"
      : "bg-logo-primary/15 hover:bg-logo-primary/30 text-logo-primary border border-logo-primary/25";

  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className={`flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-all disabled:opacity-40 ${cls}`}
    >
      {icon}
      {label}
    </button>
  );
};
