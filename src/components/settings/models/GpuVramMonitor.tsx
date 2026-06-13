import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type GpuVramStatus } from "@/bindings";
import { Cpu } from "lucide-react";

export const GpuVramMonitor: React.FC = () => {
  const [status, setStatus] = useState<GpuVramStatus | null>(null);
  const { t } = useTranslation();

  useEffect(() => {
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

    fetchStatus();
    const interval = setInterval(fetchStatus, 5000); // refresh every 5s
    return () => clearInterval(interval);
  }, []);

  if (!status || !status.is_supported) return null;

  const used = status.used_vram_mb;
  const total = status.total_vram_mb;
  const free = status.free_vram_mb;
  const percentage = total > 0 ? Math.round((used / total) * 100) : 0;

  return (
    <div className="bg-mid-gray/10 border border-mid-gray/20 rounded-lg p-4 flex flex-col gap-2 mb-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2 text-sm font-medium text-text/80">
          <Cpu className="w-4 h-4 text-logo-primary" />
          <span>{t("gpuVram.diagnostics")}</span>
        </div>
        <span className="text-xs font-mono text-text/60">
          {t("gpuVram.usage", { used, total, percentage })}
        </span>
      </div>
      <div className="w-full bg-mid-gray/20 rounded-full h-2 overflow-hidden">
        <div
          className={`h-full transition-all duration-500 rounded-full ${
            percentage > 90
              ? "bg-red-500"
              : percentage > 75
                ? "bg-yellow-500"
                : "bg-logo-primary"
          }`}
          style={{ width: `${percentage}%` }}
        />
      </div>
      <div className="flex justify-between text-[10px] text-text/40">
        <span>{t("gpuVram.used", { used })}</span>
        <span>{t("gpuVram.free", { free })}</span>
      </div>
    </div>
  );
};
