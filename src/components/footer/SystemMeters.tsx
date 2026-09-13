import { createSignal, createEffect, Show } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { commands, events, type SystemStatsEvent } from "@/bindings";

function Meter(props: { label: string; percent: number; title: string }) {
  // Accessors: the parent's `percent` expression re-evaluates on every stats
  // event, and this read must be live for the bar to move.
  const clamped = () => Math.max(0, Math.min(100, props.percent));
  const tone = () =>
    clamped() >= 90 ? "bg-error" : clamped() >= 75 ? "bg-warning" : "bg-accent";
  return (
    <div class="flex items-center gap-1 shrink-0" title={props.title}>
      <span class="font-mono text-[10px] uppercase tracking-wider text-text/50">
        {props.label}
      </span>
      <div class="w-8 h-1.5 bg-mid-gray/20 overflow-hidden">
        <div
          class={`h-full ${tone()} transition-[width] duration-500`}
          style={{ width: `${clamped()}%` }}
        />
      </div>
      <span class="font-mono text-[10px] w-7 text-end text-text/70">
        {`${Math.round(clamped())}%`}
      </span>
    </div>
  );
}

function SystemMeters() {
  const { t } = useTranslation();
  const [stats, setStats] = createSignal<SystemStatsEvent | null>(null);

  createEffect(
    () => undefined,
    () => {
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
    },
  );

  // All reads are inside the JSX: the component body runs once, so a body
  // read of `stats()` would snapshot null at mount and the meters would
  // never render — let alone update.
  const gb = (mb: number) => `${(mb / 1024).toFixed(1)} GB`;
  const memPercent = (s: SystemStatsEvent) =>
    s.mem_total_mb > 0 ? (s.mem_used_mb / s.mem_total_mb) * 100 : 0;
  const vramPercent = (s: SystemStatsEvent) =>
    s.vram_total_mb && s.vram_total_mb > 0 && s.vram_used_mb != null
      ? (s.vram_used_mb / s.vram_total_mb) * 100
      : null;

  return (
    <Show when={stats()}>
      {(s) => (
        <div class="flex items-center gap-2 shrink-0">
          <Meter
            label={t("footer.meters.cpu")}
            percent={s().cpu_percent ?? 0}
            title={t("footer.meters.cpuTitle", {
              percent: (s().cpu_percent ?? 0).toFixed(0),
            })}
          />
          <Meter
            label={t("footer.meters.ram")}
            percent={memPercent(s())}
            title={t("footer.meters.ramTitle", {
              used: gb(s().mem_used_mb),
              total: gb(s().mem_total_mb),
            })}
          />
          {s().gpu_percent != null && (
            <Meter
              label={t("footer.meters.gpu")}
              percent={s().gpu_percent!}
              title={t("footer.meters.gpuTitle", {
                name: s().gpu_name ?? "GPU",
                percent: s().gpu_percent!.toFixed(0),
                temp: s().gpu_temp_c != null ? `${s().gpu_temp_c}°C` : "—",
              })}
            />
          )}
          {vramPercent(s()) != null && (
            <Meter
              label={t("footer.meters.vram")}
              percent={vramPercent(s())!}
              title={t("footer.meters.vramTitle", {
                used: gb(s().vram_used_mb ?? 0),
                total: gb(s().vram_total_mb ?? 0),
              })}
            />
          )}
        </div>
      )}
    </Show>
  );
}

export default SystemMeters;
