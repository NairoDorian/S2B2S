import React, { useEffect, useState } from "react";
import { commands, type GpuVramStatus } from "@/bindings";

const GpuVramFooterIndicator: React.FC = () => {
  const [status, setStatus] = useState<GpuVramStatus | null>(null);

  const fetchStatus = async () => {
    try {
      const res = await commands.getActiveGpuVramStatus();
      if (res.status === "ok") {
        setStatus(res.data);
      }
    } catch (err) {
      console.error("Failed to fetch GPU VRAM status:", err);
    }
  };

  useEffect(() => {
    fetchStatus();
    const interval = setInterval(fetchStatus, 5000); // refresh every 5s
    return () => clearInterval(interval);
  }, []);

  if (!status || !status.is_supported) return null;

  const used = status.used_vram_mb;
  const total = status.total_vram_mb;
  const percentage = total > 0 ? Math.round((used / total) * 100) : 0;

  const getVramStatusColor = () => {
    if (percentage > 90) return "bg-red-400 animate-pulse";
    if (percentage > 75) return "bg-yellow-400";
    return "bg-green-400";
  };

  const formattedUsed = used >= 1024 ? `${(used / 1024).toFixed(1)} GB` : `${used} MB`;
  const formattedTotal = total >= 1024 ? `${(total / 1024).toFixed(1)} GB` : `${total} MB`;

  return (
    <>
      <span className="text-mid-gray/30 select-none">|</span>
      <button
        onClick={fetchStatus}
        className="flex items-center gap-1.5 hover:text-text/80 transition-colors cursor-pointer text-xs focus:outline-none"
        title={`VRAM: ${formattedUsed} / ${formattedTotal} (${percentage}%)`}
      >
        <span className="flex items-center gap-1">
          <span>📟</span>
          <span className="font-medium">VRAM</span>
        </span>
        <div className={`w-1.5 h-1.5 rounded-full ${getVramStatusColor()}`} />
      </button>
    </>
  );
};

export default GpuVramFooterIndicator;
