import { createSignal, For, Show } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import type {
  NativeStreamingLatencyKind,
  NativeStreamingLatencyPreset,
} from "@/bindings";
import {
  DEFAULT_LATENCY_PRESET,
  R2T2_CHUNK_MS_MAX,
  R2T2_CHUNK_MS_MIN,
  clampChunkMs,
  isContinuousLatency,
  latencyStops,
} from "@/lib/streamingLatency";
import type { JSX } from "@solidjs/web";

interface StreamingLatencyControlProps {
  kind: NativeStreamingLatencyKind;
  /** Selected preset (preset families); ignored for R2T2. */
  preset: NativeStreamingLatencyPreset;
  /** Selected chunk size in ms (R2T2); ignored for the preset families. */
  chunkMs: number;
  onPreset: (preset: NativeStreamingLatencyPreset) => void;
  onChunkMs: (chunkMs: number) => void;
  /** Visible label; defaults to "Streaming latency". */
  label?: string;
  /** Drop the explanatory paragraph (inline uses with their own help). */
  hideHint?: boolean;
  class?: string;
}

/**
 * The one latency control for every native-streaming family, in milliseconds.
 *
 * R2T2's latency is a free chunk size (80–2000 ms), so it gets a continuous
 * slider with a number field. The FastConformer families only accept the
 * settings they were trained on, so their slider snaps to those stops and
 * labels each with its ms — the same unit, the same gesture, and the words
 * "fast" / "balanced" no longer stand in for a number.
 *
 * The in-progress value lives in local state and only the committed value
 * reaches the callbacks (`change` fires on release and on blur/Enter), so a
 * drag writes the settings file once, not once per pixel. The draft is
 * dropped on commit, so a rejected write rolled back by the store shows the
 * value the backend actually uses.
 */
export const StreamingLatencyControl = (
  props: StreamingLatencyControlProps,
): JSX.Element => {
  const { t } = useTranslation();
  const [draft, setDraft] = createSignal<number | null>(null);

  const continuous = () => isContinuousLatency(props.kind);
  const stops = () => latencyStops(props.kind);
  const label = () => props.label ?? t("modelSelector.latencySelector.title");

  // Preset families: the slider position is the index of the stop.
  const selectedIndex = () => {
    const index = stops().findIndex((stop) => stop.preset === props.preset);
    return index >= 0
      ? index
      : stops().findIndex((stop) => stop.preset === DEFAULT_LATENCY_PRESET);
  };
  const shownIndex = () => draft() ?? selectedIndex();
  const shownStop = () => stops()[shownIndex()];

  // R2T2: the slider position is the chunk size itself.
  const shownChunkMs = () => draft() ?? props.chunkMs;

  const shownMs = () =>
    continuous() ? shownChunkMs() : (shownStop()?.latencyMs ?? 0);

  const commitChunk = (raw: number) => {
    const next = clampChunkMs(raw);
    setDraft(null);
    if (next !== props.chunkMs) props.onChunkMs(next);
  };

  const commitIndex = (index: number) => {
    setDraft(null);
    const stop = stops()[index];
    if (stop && stop.preset !== props.preset) props.onPreset(stop.preset);
  };

  const onSliderInput = (value: number) => {
    if (Number.isFinite(value)) setDraft(value);
  };
  const onSliderChange = (value: number) => {
    if (!Number.isFinite(value)) return setDraft(null);
    if (continuous()) commitChunk(value);
    else commitIndex(value);
  };

  return (
    // `whitespace-normal`: the status bar is `nowrap`, and its popovers
    // inherit it, which would run the hint off the edge.
    <div class={`whitespace-normal ${props.class ?? "px-2 py-1.5"}`}>
      <div class="flex items-center justify-between gap-2">
        <span class="font-medium text-text/85">{label()}</span>
        <Show
          when={continuous()}
          fallback={
            <span class="text-text/85 tabular-nums">
              {t("modelSelector.latencySelector.ms", { ms: shownMs() })}
            </span>
          }
        >
          <span class="flex items-center gap-1">
            <input
              type="number"
              min={R2T2_CHUNK_MS_MIN}
              max={R2T2_CHUNK_MS_MAX}
              step={1}
              value={shownChunkMs()}
              aria-label={label()}
              onChange={(event) => {
                const parsed = Number.parseInt(event.currentTarget.value, 10);
                if (Number.isFinite(parsed)) commitChunk(parsed);
                else setDraft(null);
              }}
              onBlur={() => setDraft(null)}
              class="w-16 rounded-md border border-mid-gray/30 bg-transparent px-1.5 py-0.5 text-end text-text/85 tabular-nums focus:border-accent focus:outline-none"
            />
            <span class="text-text/45">
              {t("modelSelector.latencySelector.unit")}
            </span>
          </span>
        </Show>
      </div>

      <input
        type="range"
        min={continuous() ? R2T2_CHUNK_MS_MIN : 0}
        max={continuous() ? R2T2_CHUNK_MS_MAX : Math.max(0, stops().length - 1)}
        step={1}
        value={continuous() ? shownChunkMs() : shownIndex()}
        aria-label={label()}
        aria-valuetext={t("modelSelector.latencySelector.ms", {
          ms: shownMs(),
        })}
        onInput={(event) =>
          onSliderInput(Number.parseInt(event.currentTarget.value, 10))
        }
        onChange={(event) =>
          onSliderChange(Number.parseInt(event.currentTarget.value, 10))
        }
        class="mt-2 w-full accent-accent"
      />

      <Show
        when={continuous()}
        fallback={
          // One label under each trained setting; the selected one is lit.
          <div class="flex items-center justify-between text-[11px] text-text/40 tabular-nums">
            <For each={stops()}>
              {(stop, index) => (
                <button
                  type="button"
                  onClick={() => commitIndex(index())}
                  class={`rounded px-0.5 hover:bg-mid-gray/10 ${index() === shownIndex() ? "font-semibold text-accent" : ""}`}
                >
                  {stop.latencyMs}
                </button>
              )}
            </For>
          </div>
        }
      >
        <div class="flex items-center justify-between text-[11px] text-text/40 tabular-nums">
          <span>{R2T2_CHUNK_MS_MIN}</span>
          {/* A single-step nudge for the fine control the 1 ms step implies. */}
          <span class="flex items-center gap-2">
            <button
              type="button"
              aria-label={t("modelSelector.latencySelector.chunkSize.decrease")}
              onClick={() => commitChunk(shownChunkMs() - 1)}
              class="rounded px-1 hover:bg-mid-gray/10"
            >
              {"−"}
            </button>
            <button
              type="button"
              aria-label={t("modelSelector.latencySelector.chunkSize.increase")}
              onClick={() => commitChunk(shownChunkMs() + 1)}
              class="rounded px-1 hover:bg-mid-gray/10"
            >
              {"+"}
            </button>
          </span>
          <span>{R2T2_CHUNK_MS_MAX}</span>
        </div>
      </Show>

      <Show when={!continuous() && shownStop()}>
        {(stop) => (
          <p class="mt-1 text-[11px] text-text/55 tabular-nums">
            {t("modelSelector.latencySelector.lookahead", {
              ms: stop().lookaheadMs,
            })}
            <Show when={stop().preset === DEFAULT_LATENCY_PRESET}>
              {` · ${t("modelSelector.latencySelector.defaultMark")}`}
            </Show>
          </p>
        )}
      </Show>

      <Show when={!props.hideHint}>
        <p class="mt-1.5 text-[11px] leading-snug text-text/45">
          {continuous()
            ? t("modelSelector.latencySelector.chunkSize.hint")
            : t("modelSelector.latencySelector.stepsHint")}
        </p>
      </Show>
    </div>
  );
};

export default StreamingLatencyControl;
