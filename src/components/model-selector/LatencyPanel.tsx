import React from "react";
import { useTranslation } from "react-i18next";
import { Check } from "lucide-react";
import type { NativeStreamingLatencyPreset } from "@/bindings";

/** Slowest-to-fastest is the mental model users have; list fastest first. */
export const LATENCY_PRESET_ORDER: NativeStreamingLatencyPreset[] = [
  "fastest",
  "fast",
  "balanced",
  "accurate",
];

/** Handy's default when a model has no stored preset. */
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

/**
 * Streaming-latency picker, shown from the status bar for models that expose a
 * native streaming latency extension.
 *
 * Each preset carries its trade-off inline, so the choice can be made without
 * remembering what "balanced" meant last time.
 */
export const LatencyPanel: React.FC<LatencyPanelProps> = ({
  selected,
  onSelect,
}) => {
  const { t } = useTranslation();

  return (
    <ul role="radiogroup" className="py-1">
      {LATENCY_PRESET_ORDER.map((preset) => {
        const isSelected = preset === selected;
        return (
          <li key={preset}>
            <button
              type="button"
              role="radio"
              aria-checked={isSelected}
              onClick={() => onSelect(preset)}
              className={`mx-1 flex w-[calc(100%-0.5rem)] items-start gap-2 rounded-md px-2 py-1.5 text-start transition-colors ${
                isSelected ? "bg-logo-primary/10" : "hover:bg-mid-gray/10"
              }`}
            >
              <Check
                className={`mt-0.5 h-3 w-3 shrink-0 ${
                  isSelected ? "text-logo-primary" : "text-transparent"
                }`}
              />
              <span className="min-w-0">
                <span
                  className={`block font-medium ${
                    isSelected ? "text-logo-primary" : "text-text/85"
                  }`}
                >
                  {t(latencyPresetLabelKey(preset))}
                </span>
                <span className="block text-[11px] leading-snug text-text/45">
                  {t(latencyPresetDescriptionKey(preset))}
                </span>
              </span>
            </button>
          </li>
        );
      })}
    </ul>
  );
};

export default LatencyPanel;
