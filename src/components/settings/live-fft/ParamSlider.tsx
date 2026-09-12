import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { RotateCcw } from "lucide-react";
import { SettingContainer } from "@/components/ui/SettingContainer";

interface ParamSliderProps {
  label: string;
  description: string;
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (value: number) => void;
  /** Slider travels logarithmically (frequencies); needs `min > 0`. */
  log?: boolean;
  integer?: boolean;
  unit?: string;
  disabled?: boolean;
  /** Shown as a reset button when the value differs from it. */
  defaultValue?: number;
  format?: (value: number) => string;
}

const toPos = (v: number, min: number, max: number, log: boolean): number => {
  if (log) return Math.log(v / min) / Math.log(max / min);
  return (v - min) / (max - min);
};

const fromPos = (p: number, min: number, max: number, log: boolean): number => {
  if (log) return min * Math.pow(max / min, p);
  return min + (max - min) * p;
};

/**
 * A slider with a typed number field next to it, for the dense parameter
 * pages of the Live FFT analyser: large ranges (window samples, cutoff
 * frequencies) stay precise, and frequencies travel logarithmically.
 */
export const ParamSlider: React.FC<ParamSliderProps> = ({
  label,
  description,
  value,
  min,
  max,
  step,
  onChange,
  log = false,
  integer = false,
  unit,
  disabled = false,
  defaultValue,
  format,
}) => {
  const { t } = useTranslation();
  const [text, setText] = useState(String(value));
  useEffect(() => {
    setText(integer ? String(Math.round(value)) : String(value));
  }, [value, integer]);

  const clamp = (v: number): number => {
    let next = Math.min(max, Math.max(min, v));
    if (integer) next = Math.round(next);
    else if (!log) next = Math.round(next / step) * step;
    return Number(next.toPrecision(7));
  };

  const commitText = () => {
    const parsed = Number(text);
    if (Number.isFinite(parsed)) onChange(clamp(parsed));
    else setText(String(value));
  };

  const pos = toPos(Math.min(max, Math.max(min, value)), min, max, log);
  const showReset = defaultValue !== undefined && defaultValue !== value;

  return (
    <SettingContainer
      title={label}
      description={description}
      descriptionMode="tooltip"
      grouped
      layout="horizontal"
      disabled={disabled}
    >
      <div className="flex items-center gap-2 w-full max-w-[320px] ms-auto">
        <input
          type="range"
          min={0}
          max={1000}
          step={1}
          value={Math.round(pos * 1000)}
          disabled={disabled}
          onChange={(e) => {
            const p = Number(e.target.value) / 1000;
            onChange(clamp(fromPos(p, min, max, log)));
          }}
          className="flex-1 min-w-[90px] accent-accent cursor-pointer disabled:cursor-not-allowed"
        />
        <input
          type="text"
          inputMode="decimal"
          value={text}
          disabled={disabled}
          onChange={(e) => setText(e.target.value)}
          onBlur={commitText}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              commitText();
              (e.target as HTMLInputElement).blur();
            }
          }}
          aria-label={label}
          className="w-[76px] shrink-0 rounded-md border border-mid-gray/20 bg-background px-2 py-1 text-xs font-mono text-end focus:outline-none focus:border-accent disabled:opacity-50"
        />
        <span className="w-8 shrink-0 text-[11px] text-mid-gray truncate">
          {format ? format(value) : (unit ?? "")}
        </span>
        <button
          type="button"
          onClick={() => defaultValue !== undefined && onChange(defaultValue)}
          disabled={disabled || !showReset}
          title={t("settings.liveFft.resetValue")}
          aria-label={t("settings.liveFft.resetValue")}
          className={`shrink-0 p-1 rounded-md text-mid-gray hover:text-text hover:bg-mid-gray/15 cursor-pointer disabled:cursor-default ${
            showReset ? "opacity-100" : "opacity-0 pointer-events-none"
          }`}
        >
          <RotateCcw className="w-3 h-3" />
        </button>
      </div>
    </SettingContainer>
  );
};
