import React, { useState, useCallback, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import {
  Code2,
  Play,
  Copy,
  Check,
  RotateCcw,
  Sparkles,
  Terminal,
  Activity,
  CheckCircle2,
  AlertCircle,
  AlertTriangle,
  Info,
} from "lucide-react";
import { toast } from "sonner";
import { commands } from "@/bindings";
import { SettingContainer } from "../../ui/SettingContainer";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Button } from "../../ui/Button";

interface IpcCommandOption {
  id: string;
  label: string;
  desc: string;
}

const AVAILABLE_COMMANDS: IpcCommandOption[] = [
  {
    id: "get_app_info",
    label: "get_app_info",
    desc: "Fetch app version, OS, arch, and Tauri engine",
  },
  {
    id: "get_app_settings",
    label: "get_app_settings",
    desc: "Fetch complete persisted AppSettings JSON",
  },
  {
    id: "get_default_settings",
    label: "get_default_settings",
    desc: "Fetch default AppSettings JSON schema",
  },
  {
    id: "get_system_ram",
    label: "get_system_ram",
    desc: "Query total, used, and free system memory",
  },
  {
    id: "get_app_dir_path",
    label: "get_app_dir_path",
    desc: "Query resolved App Data configuration directory",
  },
  {
    id: "get_log_dir_path",
    label: "get_log_dir_path",
    desc: "Query resolved system log file directory",
  },
  {
    id: "get_available_models",
    label: "get_available_models",
    desc: "Query registered STT speech models",
  },
  {
    id: "get_tts_config",
    label: "get_tts_config",
    desc: "Query active TTS voice engine configuration",
  },
  {
    id: "get_brain_config",
    label: "get_brain_config",
    desc: "Query active Brain LLM configuration",
  },
];

export const DeveloperHub: React.FC = () => {
  const { t } = useTranslation();
  const [selectedCommand, setSelectedCommand] =
    useState<string>("get_app_info");
  const [commandOutput, setCommandOutput] = useState<string>(
    "// Select an IPC command and press Run",
  );
  const [isRunning, setIsRunning] = useState<boolean>(false);
  const [copied, setCopied] = useState<boolean>(false);
  const [ramInfo, setRamInfo] = useState<{
    total_mb: number;
    used_mb: number;
    free_mb: number;
  } | null>(null);

  useEffect(() => {
    // Initial fetch of system ram info if available
    commands
      .getSystemRam()
      .then((res) => {
        if (res.status === "ok") setRamInfo(res.data);
      })
      .catch(() => {});
  }, []);

  const handleRunCommand = useCallback(async () => {
    setIsRunning(true);
    try {
      const result = await invoke(selectedCommand);
      setCommandOutput(JSON.stringify(result, null, 2));
      toast.success(`IPC '${selectedCommand}' executed successfully`);
    } catch (err: unknown) {
      const errMsg = err instanceof Error ? err.message : String(err);
      setCommandOutput(`// Error invoking ${selectedCommand}:\n${errMsg}`);
      toast.error(`IPC error: ${errMsg}`);
    } finally {
      setIsRunning(false);
    }
  }, [selectedCommand]);

  const handleCopyOutput = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(commandOutput);
      setCopied(true);
      toast.success("Command output copied to clipboard");
      setTimeout(() => setCopied(false), 2000);
    } catch {
      toast.error("Failed to copy command output");
    }
  }, [commandOutput]);

  const handleTestToast = (type: "info" | "success" | "warning" | "error") => {
    switch (type) {
      case "info":
        toast.info("Sample informative notification from Developer Hub");
        break;
      case "success":
        toast.success("Sample success notification from Developer Hub");
        break;
      case "warning":
        toast.warning("Sample warning notification from Developer Hub");
        break;
      case "error":
        toast.error("Sample error notification from Developer Hub");
        break;
    }
  };

  return (
    <SettingsGroup title="Developer Hub & IPC Diagnostics">
      {/* Live System RAM Telemetry */}
      {ramInfo && (
        <SettingContainer
          title="System Memory Telemetry"
          description="Real-time physical host memory status"
          grouped={true}
          layout="stacked"
        >
          <div className="grid grid-cols-3 gap-3 w-full">
            <div className="p-3 rounded-xl bg-black/40 border border-neutral-800 text-center">
              <span className="text-[11px] text-neutral-400 block uppercase tracking-wider font-semibold">
                Total RAM
              </span>
              <span className="text-sm font-mono font-bold text-neutral-100 mt-0.5 block">
                {(ramInfo.total_mb / 1024).toFixed(1)} GB
              </span>
            </div>
            <div className="p-3 rounded-xl bg-black/40 border border-neutral-800 text-center">
              <span className="text-[11px] text-neutral-400 block uppercase tracking-wider font-semibold">
                Used RAM
              </span>
              <span className="text-sm font-mono font-bold text-amber-400 mt-0.5 block">
                {(ramInfo.used_mb / 1024).toFixed(1)} GB
              </span>
            </div>
            <div className="p-3 rounded-xl bg-black/40 border border-neutral-800 text-center">
              <span className="text-[11px] text-neutral-400 block uppercase tracking-wider font-semibold">
                Free RAM
              </span>
              <span className="text-sm font-mono font-bold text-emerald-400 mt-0.5 block">
                {(ramInfo.free_mb / 1024).toFixed(1)} GB
              </span>
            </div>
          </div>
        </SettingContainer>
      )}

      {/* IPC Command Runner */}
      <SettingContainer
        title="Live IPC Command Runner"
        description="Directly execute and inspect Tauri backend IPC endpoints with real payloads"
        grouped={true}
        layout="stacked"
      >
        <div className="space-y-3 w-full">
          <div className="flex items-center gap-2">
            <select
              value={selectedCommand}
              onChange={(e) => setSelectedCommand(e.target.value)}
              className="flex-1 bg-black/60 border border-neutral-700 rounded-lg px-3 py-2 text-xs font-mono text-neutral-200 focus:outline-none focus:border-amber-500"
            >
              {AVAILABLE_COMMANDS.map((cmd) => (
                <option key={cmd.id} value={cmd.id}>
                  {cmd.label} — {cmd.desc}
                </option>
              ))}
            </select>
            <button
              type="button"
              onClick={handleRunCommand}
              disabled={isRunning}
              className="inline-flex items-center gap-1.5 px-4 py-2 text-xs font-medium rounded-lg bg-amber-600 hover:bg-amber-500 disabled:opacity-50 text-white transition-colors shrink-0"
            >
              <Play size={13} />
              <span>{isRunning ? "Running..." : "Run"}</span>
            </button>
          </div>

          <div className="relative">
            <pre className="p-4 rounded-xl bg-black/80 border border-neutral-800 text-xs font-mono text-neutral-300 max-h-56 overflow-y-auto whitespace-pre-wrap leading-relaxed">
              {commandOutput}
            </pre>
            <button
              type="button"
              onClick={handleCopyOutput}
              className="absolute top-2.5 right-2.5 p-1.5 rounded-md bg-neutral-800/80 hover:bg-neutral-700 border border-neutral-700 text-neutral-300 transition-colors"
              title="Copy output"
            >
              {copied ? (
                <Check size={13} className="text-emerald-400" />
              ) : (
                <Copy size={13} />
              )}
            </button>
          </div>
        </div>
      </SettingContainer>

      {/* Toast Notification Playground */}
      <SettingContainer
        title="Toast Notification Playground"
        description="Test and benchmark system notification alerts"
        grouped={true}
        layout="stacked"
      >
        <div className="flex flex-wrap gap-2 w-full">
          <button
            type="button"
            onClick={() => handleTestToast("info")}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg bg-neutral-800 hover:bg-neutral-700 border border-neutral-700 text-neutral-200 transition-colors"
          >
            <Info size={13} className="text-sky-400" />
            <span>Test Info Toast</span>
          </button>
          <button
            type="button"
            onClick={() => handleTestToast("success")}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg bg-neutral-800 hover:bg-neutral-700 border border-neutral-700 text-neutral-200 transition-colors"
          >
            <CheckCircle2 size={13} className="text-emerald-400" />
            <span>Test Success Toast</span>
          </button>
          <button
            type="button"
            onClick={() => handleTestToast("warning")}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg bg-neutral-800 hover:bg-neutral-700 border border-neutral-700 text-neutral-200 transition-colors"
          >
            <AlertTriangle size={13} className="text-amber-400" />
            <span>Test Warning Toast</span>
          </button>
          <button
            type="button"
            onClick={() => handleTestToast("error")}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg bg-neutral-800 hover:bg-neutral-700 border border-neutral-700 text-neutral-200 transition-colors"
          >
            <AlertCircle size={13} className="text-red-400" />
            <span>Test Error Toast</span>
          </button>
        </div>
      </SettingContainer>
    </SettingsGroup>
  );
};
