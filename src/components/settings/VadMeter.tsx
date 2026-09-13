import { createSignal, createEffect, type Accessor } from "solid-js";
import type { JSX } from "@solidjs/web";
import { useTranslation } from "@/i18n/useTranslation";
import { events, type VadTestEvent } from "@/bindings";

const STALL_MS = 1500;
const PEAK_DECAY = 0.02;

export interface VadFrames {
  frame: Accessor<VadTestEvent | null>;
  peak: Accessor<number>;
  stalled: Accessor<boolean>;
}

export function useVadFrames(active: Accessor<boolean>): VadFrames {
  const [frame, setFrame] = createSignal<VadTestEvent | null>(null);
  const [peak, setPeak] = createSignal(0);
  const [stalled, setStalled] = createSignal(false);
  let lastEventAt = 0;

  createEffect(
    () => active(),
    (isActive) => {
      if (!isActive) {
        setFrame(null);
        setPeak(0);
        setStalled(false);
        return;
      }
      lastEventAt = Date.now();
      let cancelled = false;
      let unlisten: (() => void) | undefined;
      events.vadTestEvent
        .listen((event) => {
          lastEventAt = Date.now();
          setStalled(false);
          setFrame(event.payload);
          const score = event.payload.score ?? 0;
          setPeak((prev) => Math.max(score, prev - PEAK_DECAY));
        })
        .then((fn) => {
          if (cancelled) fn();
          else unlisten = fn;
        });
      const watchdog = setInterval(() => {
        if (Date.now() - lastEventAt > STALL_MS) setStalled(true);
      }, 500);
      return () => {
        cancelled = true;
        unlisten?.();
        clearInterval(watchdog);
      };
    },
  );

  return { frame, peak, stalled };
}

const pct = (v: number) => `${Math.round(Math.min(1, Math.max(0, v)) * 100)}%`;

interface VadStatusChipProps {
  frames: VadFrames;
}

export const VadStatusChip = (props: VadStatusChipProps) => {
  const { t } = useTranslation();
  const voiced = () => props.frames.frame()?.voiced ?? false;
  return (
    <span
      class={`px-2 py-0.5 rounded-full text-xs font-medium border transition-colors ${
        props.frames.stalled()
          ? "bg-amber-500/10 text-amber-400 border-amber-500/20"
          : voiced()
            ? "bg-emerald-500/15 text-emerald-400 border-emerald-500/30"
            : "bg-mid-gray/10 text-text/60 border-mid-gray/20"
      }`}
    >
      {props.frames.stalled()
        ? t("settings.advanced.vadLiveTest.noSignal")
        : voiced()
          ? t("settings.advanced.vadLiveTest.speech")
          : t("settings.advanced.vadLiveTest.silence")}
    </span>
  );
};

interface VadMeterProps {
  frames: VadFrames;
  threshold: number;
  denoiseThreshold?: number;
  children?: JSX.Element;
}

export const VadMeter = (props: VadMeterProps) => {
  const { t } = useTranslation();
  const score = () => props.frames.frame()?.score ?? null;
  const voiced = () => props.frames.frame()?.voiced ?? false;
  const kept = () => props.frames.frame()?.kept ?? false;
  const level = () => props.frames.frame()?.level ?? 0;
  const denoiseProb = () => props.frames.frame()?.denoise_prob ?? null;

  return (
    <div class="flex flex-col gap-2 rounded-md p-3 bg-card/40 border border-mid-gray/15">
      <div class="flex items-center justify-between text-xs text-text/70">
        <span>
          {t("settings.advanced.vadLiveTest.score", {
            score: score() === null ? "—" : score()?.toFixed(2),
          })}
        </span>
        <span>
          {t("settings.advanced.vadLiveTest.threshold", {
            threshold: props.threshold.toFixed(2),
          })}
        </span>
      </div>
      <div class="relative h-3 rounded-full bg-mid-gray/20 overflow-hidden">
        <div
          class={`h-full rounded-full transition-[width] duration-75 ${
            voiced() ? "bg-emerald-500" : "bg-mid-gray/60"
          }`}
          style={{ width: pct(score() ?? 0) }}
        />
        <div
          class="absolute top-0 h-full w-0.5 bg-text/40"
          style={{ left: pct(props.frames.peak()) }}
        />
        <div
          class="absolute top-0 h-full w-0.5 bg-accent"
          style={{ left: pct(props.threshold) }}
        />
      </div>
      <div class="flex items-center gap-2 text-xs text-text/60">
        <span class="w-20 shrink-0">
          {t("settings.advanced.vadLiveTest.level")}
        </span>
        <div class="relative flex-grow h-1.5 rounded-full bg-mid-gray/20 overflow-hidden">
          <div
            class="h-full rounded-full bg-blue-400/70 transition-[width] duration-75"
            style={{ width: pct(level()) }}
          />
        </div>
        <span
          class={`shrink-0 px-1.5 py-0.5 rounded text-[10px] font-medium border ${
            kept()
              ? "bg-accent/15 text-text border-accent/30"
              : "bg-mid-gray/10 text-text/50 border-mid-gray/20"
          }`}
        >
          {kept()
            ? t("settings.advanced.vadLiveTest.kept")
            : t("settings.advanced.vadLiveTest.dropped")}
        </span>
      </div>
      {denoiseProb() !== null && (
        <div class="flex items-center gap-2 text-xs text-text/60">
          <span class="w-20 shrink-0">
            {t("settings.advanced.vadLiveTest.denoiseProb")}
          </span>
          <div class="relative flex-grow h-1.5 rounded-full bg-mid-gray/20 overflow-hidden">
            <div
              class="h-full rounded-full bg-violet-400/70 transition-[width] duration-75"
              style={{ width: pct(denoiseProb() ?? 0) }}
            />
            {(props.denoiseThreshold ?? 0) > 0 && (
              <div
                class="absolute top-0 h-full w-0.5 bg-accent"
                style={{ left: pct(props.denoiseThreshold ?? 0) }}
              />
            )}
          </div>
          <span class="shrink-0 w-10 text-right tabular-nums">
            {denoiseProb()?.toFixed(2)}
          </span>
        </div>
      )}
      {props.children}
    </div>
  );
};
