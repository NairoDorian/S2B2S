/* oxlint-disable jsx-a11y/prefer-tag-over-role */
import { For } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { Check } from "@/components/icons/lucide";
import type { NativeStreamingLatencyPreset } from "@/bindings";
import type { JSX } from "@solidjs/web";

export const LATENCY_PRESET_ORDER: NativeStreamingLatencyPreset[] = [
  "fastest",
  "fast",
  "balanced",
  "accurate",
];
export const DEFAULT_LATENCY_PRESET: NativeStreamingLatencyPreset = "accurate";

export const latencyPresetLabelKey = (
  preset: NativeStreamingLatencyPreset,
): string => `modelSelector.latencySelector.${preset}`;
export const latencyPresetDescriptionKey = (
  preset: NativeStreamingLatencyPreset,
): string => `modelSelector.latencySelector.descriptions.${preset}`;

interface LatencyPanelProps {
  selected: NativeStreamingLatencyPreset;
  onSelect: (preset: NativeStreamingLatencyPreset) => void;
}

export const LatencyPanel = (props: LatencyPanelProps): JSX.Element => {
  const { t } = useTranslation();

  return (
    <ul role="radiogroup" class="py-1">
      <For each={LATENCY_PRESET_ORDER}>
        {(preset) => {
          // An accessor: the row outlives a change of selection.
          const isSelected = () => preset === props.selected;
          return (
            <li>
              <button
                type="button"
                role="radio"
                aria-checked={isSelected() ? "true" : "false"}
                onClick={() => props.onSelect(preset)}
                class={`mx-1 flex w-[calc(100%-0.5rem)] items-start gap-2 rounded-md px-2 py-1.5 text-start transition-colors ${isSelected() ? "bg-accent/10" : "hover:bg-mid-gray/10"}`}
              >
                <Check
                  class={`mt-0.5 h-3 w-3 shrink-0 ${isSelected() ? "text-accent" : "text-transparent"}`}
                />
                <span class="min-w-0">
                  <span
                    class={`block font-medium ${isSelected() ? "text-accent" : "text-text/85"}`}
                  >
                    {t(latencyPresetLabelKey(preset))}
                  </span>
                  <span class="block text-[11px] leading-snug text-text/45">
                    {t(latencyPresetDescriptionKey(preset))}
                  </span>
                </span>
              </button>
            </li>
          );
        }}
      </For>
    </ul>
  );
};

export default LatencyPanel;
