import { createSignal } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import type { JSX } from "@solidjs/web";

/**
 * Inclusive bounds of the R2T2 streaming chunk size, in whole milliseconds.
 *
 * These mirror `R2T2_CHUNK_MS_MIN` / `R2T2_CHUNK_MS_MAX` in the app's
 * `managers::native_streaming_latency` module, which in turn mirror
 * `k_r2t2_chunk_ms_min` / `k_r2t2_chunk_ms_max` in the fork. They exist here so
 * the slider has the right domain and the numeric field can clamp as you type;
 * they are deliberately NOT the authority — the settings command rejects an
 * out-of-range value and the native `stream_begin` rejects it again, because
 * the settings file is hand-editable and the UI is not a trust boundary.
 */
export const R2T2_CHUNK_MS_MIN = 80;
export const R2T2_CHUNK_MS_MAX = 2000;
export const R2T2_CHUNK_MS_DEFAULT = 320;

/** Clamp to the native range; used for typed input, which can hold anything. */
export const clampChunkMs = (ms: number): number =>
  Math.min(R2T2_CHUNK_MS_MAX, Math.max(R2T2_CHUNK_MS_MIN, Math.round(ms)));

interface ChunkSizePanelProps {
  selected: number;
  onSelect: (chunkMs: number) => void;
}

/**
 * Numeric chunk-size control for the R2T2 family.
 *
 * Unlike `LatencyPanel` — four discrete presets, each applied on click — this
 * is a continuous 80..=2000 ms range with a 1 ms step, so a naive binding would
 * write the settings file on every `input` event of a slider drag (hundreds of
 * writes for one gesture). Instead the in-progress value lives in local state
 * and only the committed value reaches `onSelect`: `onChange` on the slider
 * (fires on release, not per pixel) and on the number field (fires on
 * blur/Enter). The local draft is re-synced from the prop so an external change
 * — or a rejection that rolled the store back — cannot leave the control
 * showing a value the backend is not using.
 *
 * The label reads "Streaming chunk size", not "latency": the chunk duration is
 * a cost/compute knob, and end-to-end latency also includes queue wait and the
 * first committed text, so labelling it as latency would overpromise.
 */
export const ChunkSizePanel = (props: ChunkSizePanelProps): JSX.Element => {
  const { t } = useTranslation();
  const [draft, setDraft] = createSignal<number | null>(null);

  // The shown value: the local draft while the user is mid-gesture, otherwise
  // whatever the store currently holds.
  const value = () => draft() ?? props.selected;

  const commit = (raw: number) => {
    const next = clampChunkMs(raw);
    setDraft(null);
    if (next !== props.selected) props.onSelect(next);
  };

  const step = (delta: number) => commit(value() + delta);

  return (
    <div class="px-2 py-1.5">
      <div class="flex items-center justify-between gap-2">
        <span class="font-medium text-text/85">
          {t("modelSelector.latencySelector.chunkSize.label")}
        </span>
        <span class="flex items-center gap-1">
          <input
            type="number"
            min={R2T2_CHUNK_MS_MIN}
            max={R2T2_CHUNK_MS_MAX}
            step={1}
            value={value()}
            aria-label={t("modelSelector.latencySelector.chunkSize.label")}
            onChange={(event) => {
              const parsed = Number.parseInt(event.currentTarget.value, 10);
              if (Number.isFinite(parsed)) commit(parsed);
              else setDraft(null);
            }}
            onBlur={() => setDraft(null)}
            class="w-16 rounded-md border border-mid-gray/30 bg-transparent px-1.5 py-0.5 text-end text-text/85 tabular-nums focus:border-accent focus:outline-none"
          />
          <span class="text-text/45">
            {t("modelSelector.latencySelector.chunkSize.unit")}
          </span>
        </span>
      </div>

      <input
        type="range"
        min={R2T2_CHUNK_MS_MIN}
        max={R2T2_CHUNK_MS_MAX}
        step={1}
        value={value()}
        aria-label={t("modelSelector.latencySelector.chunkSize.label")}
        onInput={(event) =>
          setDraft(Number.parseInt(event.currentTarget.value, 10))
        }
        onChange={(event) =>
          commit(Number.parseInt(event.currentTarget.value, 10))
        }
        class="mt-2 w-full accent-accent"
      />

      <div class="flex items-center justify-between text-[11px] text-text/40 tabular-nums">
        <span>{R2T2_CHUNK_MS_MIN}</span>
        {/* A single-step nudge for keyboard/pointer users who want the same
            fine control the 1 ms slider step implies. */}
        <span class="flex items-center gap-2">
          <button
            type="button"
            aria-label={t("modelSelector.latencySelector.chunkSize.decrease")}
            onClick={() => step(-1)}
            class="rounded px-1 hover:bg-mid-gray/10"
          >
            −
          </button>
          <button
            type="button"
            aria-label={t("modelSelector.latencySelector.chunkSize.increase")}
            onClick={() => step(1)}
            class="rounded px-1 hover:bg-mid-gray/10"
          >
            +
          </button>
        </span>
        <span>{R2T2_CHUNK_MS_MAX}</span>
      </div>

      <p class="mt-1.5 text-[11px] leading-snug text-text/45">
        {t("modelSelector.latencySelector.chunkSize.hint")}
      </p>
    </div>
  );
};

export default ChunkSizePanel;
