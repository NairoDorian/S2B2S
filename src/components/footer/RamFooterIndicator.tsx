import React, { useCallback, useEffect, useRef, useState } from "react";
import { commands, type SystemRamInfo } from "@/bindings";

const formatMb = (mb: number): string => {
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
  return `${Math.round(mb)} MB`;
};

const RamFooterIndicator: React.FC = () => {
  const [status, setStatus] = useState<SystemRamInfo | null>(null);
  const [error, setError] = useState(false);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const fetchStatus = useCallback(async () => {
    try {
      const res = await commands.getSystemRam();
      if (res.status === "ok") {
        setStatus(res.data);
        setError(false);
      } else {
        setError(true);
      }
    } catch (err) {
      setError(true);
    }
  }, []);

  useEffect(() => {
    fetchStatus();
    timerRef.current = setInterval(fetchStatus, 5000);
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [fetchStatus]);

  if (error && !status) {
    return (
      <>
        <span className="text-mid-gray/30 select-none">|</span>
        <span className="flex items-center gap-1.5 text-xs text-mid-gray/40">
          <span>🧠</span>
          <span className="font-medium">RAM</span>
          <span className="tabular-nums">--</span>
        </span>
      </>
    );
  }

  if (!status) {
    return (
      <>
        <span className="text-mid-gray/30 select-none">|</span>
        <span className="flex items-center gap-1.5 text-xs text-mid-gray/40">
          <span>🧠</span>
          <span className="font-medium">RAM</span>
          <span className="tabular-nums animate-pulse">...</span>
        </span>
      </>
    );
  }

  const percentage = status.total_mb > 0
    ? Math.round((status.used_mb / status.total_mb) * 100)
    : 0;

  const getRamStatusColor = () => {
    if (percentage > 90) return "bg-red-400 animate-pulse";
    if (percentage > 75) return "bg-yellow-400";
    return "bg-green-400";
  };

  const tooltip = [
    `🧠 System RAM`,
    `  Used: ${formatMb(status.used_mb)} / ${formatMb(status.total_mb)} (${percentage}%)`,
    `  Free: ${formatMb(status.free_mb)}`,
  ].join("\n");

  return (
    <>
      <span className="text-mid-gray/30 select-none">|</span>
      <button
        onClick={fetchStatus}
        className="flex items-center gap-1.5 hover:text-text/80 transition-colors cursor-pointer text-xs focus:outline-none"
        title={tooltip}
      >
        <span className="flex items-center gap-1">
          <span>🧠</span>
          <span className="font-medium">RAM</span>
        </span>
        <div
          className={`w-1.5 h-1.5 rounded-full ${getRamStatusColor()}`}
        />
        <span className="tabular-nums text-text/70">{percentage}%</span>
      </button>
    </>
  );
};

export default RamFooterIndicator;
