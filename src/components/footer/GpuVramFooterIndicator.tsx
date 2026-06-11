import React, { useCallback, useEffect, useRef, useState } from "react";
import { commands, type GpuVramStatus } from "@/bindings";

const formatMb = (mb: number): string => {
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
  return `${Math.round(mb)} MB`;
};

const GpuVramFooterIndicator: React.FC = () => {
  const [status, setStatus] = useState<GpuVramStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const fetchStatus = useCallback(async (silent = false) => {
    if (!silent) setLoading(true);
    try {
      const res = await commands.getActiveGpuVramStatus();
      if (res.status === "ok") {
        setStatus(res.data);
      }
    } catch (err) {
      console.error("Failed to fetch GPU VRAM status:", err);
    } finally {
      if (!silent) setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchStatus(true);
    timerRef.current = setInterval(() => fetchStatus(true), 1000);
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [fetchStatus]);

  if (!status || !status.is_supported) return null;

  const total = status.total_vram_mb;
  const used = status.used_vram_mb;
  const percentage = total > 0 ? Math.round((used / total) * 100) : 0;

  const getVramStatusColor = () => {
    if (percentage > 90) return "bg-red-400 animate-pulse";
    if (percentage > 75) return "bg-yellow-400";
    return "bg-green-400";
  };

  const adapterName = status.adapter_name ? ` on ${status.adapter_name}` : "";
  const tooltipParts: string[] = [
    `📟 VRAM${adapterName}`,
    `  System: ${formatMb(used)} / ${formatMb(total)} (${percentage}%)`,
    `  Free: ${formatMb(status.free_vram_mb)}`,
    `  App: ${formatMb(status.process_used_mb)} / ${formatMb(status.process_budget_mb)}`,
  ];

  if (status.llm_servers && status.llm_servers.length > 0) {
    for (const srv of status.llm_servers) {
      tooltipParts.push(`  ${srv.name} (PID ${srv.pid})`);
    }
  }

  const lastUpdated = new Date(
    status.updated_at_unix_ms ?? Date.now(),
  ).toLocaleTimeString();
  tooltipParts.push(`  Updated: ${lastUpdated}`);
  tooltipParts.push("Click to refresh");

  return (
    <>
      <span className="text-mid-gray/30 select-none">|</span>
      <button
        onClick={() => fetchStatus(false)}
        className="flex items-center gap-1.5 hover:text-text/80 transition-colors cursor-pointer text-xs focus:outline-none"
        title={tooltipParts.join("\n")}
      >
        <span className="flex items-center gap-1">
          <span>📟</span>
          <span className="font-medium">VRAM</span>
        </span>
        <div
          className={`w-1.5 h-1.5 rounded-full ${getVramStatusColor()} ${loading ? "animate-pulse" : ""}`}
        />
        <span className="tabular-nums text-text/70">{percentage}%</span>
      </button>
    </>
  );
};

export default GpuVramFooterIndicator;
