import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, events, type SystemStatsEvent } from "@/bindings";

/**
 * CPU / RAM / GPU / VRAM meters in the status bar. Fed by the backend's
 * 1 Hz `SystemStatsEvent`; the only command call is one snapshot on mount
 * so the bar is not empty for the first second.
 */
const Meter: React.FC<{ label: string; percent: number; title: string }> = ({
  label,
  percent,
  title,
}) => {
  const clamped = Math.max(0, Math.min(100, percent));
  const tone =
    clamped >= 90
      ? "bg-error"
      : clamped >= 75
        ? "bg-warning"
        : "bg-logo-primary";
  return (
    <div className="flex items-center gap-1 shrink-0" title={title}>
      <span className="font-mono text-[10px] uppercase tracking-wider text-text/50">
        {label}
      </span>
      <div className="w-8 h-1.5 bg-mid-gray/20 overflow-hidden">
        <div
          className={`h-full ${tone} transition-[width] duration-500`}
          style={{ width: `${clamped}%` }}
        />
      </div>
      <span className="font-mono text-[10px] w-7 text-end text-text/70">
        {`${Math.round(clamped)}%`}
      </span>
    </div>
  );
};

export const SystemMeters: React.FC = () => {
  const { t } = useTranslation();
  const [stats, setStats] = useState<SystemStatsEvent | null>(null);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    commands
      .getSystemStats()
      .then((s) => {
        if (!cancelled && s) setStats(s);
      })
      .catch(() => {});
    events.systemStatsEvent
      .listen((event) => setStats(event.payload))
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  if (!stats) return null;

  const cpuPercent = stats.cpu_percent ?? 0;
  const memPercent =
    stats.mem_total_mb > 0 ? (stats.mem_used_mb / stats.mem_total_mb) * 100 : 0;
  const vramPercent =
    stats.vram_total_mb && stats.vram_total_mb > 0 && stats.vram_used_mb != null
      ? (stats.vram_used_mb / stats.vram_total_mb) * 100
      : null;
  const gb = (mb: number) => `${(mb / 1024).toFixed(1)} GB`;

  return (
    <div className="flex items-center gap-2 shrink-0">
      <Meter
        label={t("footer.meters.cpu")}
        percent={cpuPercent}
        title={t("footer.meters.cpuTitle", {
          percent: cpuPercent.toFixed(0),
        })}
      />
      <Meter
        label={t("footer.meters.ram")}
        percent={memPercent}
        title={t("footer.meters.ramTitle", {
          used: gb(stats.mem_used_mb),
          total: gb(stats.mem_total_mb),
        })}
      />
      {stats.gpu_percent != null && (
        <Meter
          label={t("footer.meters.gpu")}
          percent={stats.gpu_percent}
          title={t("footer.meters.gpuTitle", {
            name: stats.gpu_name ?? "GPU",
            percent: stats.gpu_percent.toFixed(0),
            temp: stats.gpu_temp_c != null ? `${stats.gpu_temp_c}°C` : "—",
          })}
        />
      )}
      {vramPercent != null && (
        <Meter
          label={t("footer.meters.vram")}
          percent={vramPercent}
          title={t("footer.meters.vramTitle", {
            used: gb(stats.vram_used_mb ?? 0),
            total: gb(stats.vram_total_mb ?? 0),
          })}
        />
      )}
    </div>
  );
};

export default SystemMeters;
