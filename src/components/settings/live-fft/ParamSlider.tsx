import { createSignal, createEffect, untrack } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { RotateCcw } from "@/components/icons/lucide";
import { SettingContainer } from "@/components/ui/SettingContainer";

interface ParamSliderProps {
  label: string;
  description: string;
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (value: number) => void;
  log?: boolean;
  integer?: boolean;
  unit?: string;
  disabled?: boolean;
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

export const ParamSlider = (props: ParamSliderProps) => {
  const { t } = useTranslation();
  const log = () => props.log ?? false;
  const integer = () => props.integer ?? false;
  const disabled = () => props.disabled ?? false;
  // The text field seeds from the prop once (explicit snapshot); the effect
  // below keeps it in step with the value from then on.
  const [text, setText] = createSignal(untrack(() => String(props.value)));
  createEffect(
    () => props.value,
    (value) => {
      setText(integer() ? String(Math.round(value)) : String(value));
    },
  );

  const clamp = (v: number): number => {
    let next = Math.min(props.max, Math.max(props.min, v));
    if (integer()) next = Math.round(next);
    else if (!log()) next = Math.round(next / props.step) * props.step;
    return Number(next.toPrecision(7));
  };

  const commitText = () => {
    const parsed = Number(text());
    if (Number.isFinite(parsed)) props.onChange(clamp(parsed));
    else setText(String(props.value));
  };

  const pos = () =>
    toPos(
      Math.min(props.max, Math.max(props.min, props.value)),
      props.min,
      props.max,
      log(),
    );
  const showReset = () =>
    props.defaultValue !== undefined && props.defaultValue !== props.value;

  return (
    <SettingContainer
      title={props.label}
      description={props.description}
      descriptionMode="tooltip"
      grouped
      layout="horizontal"
      disabled={disabled()}
    >
      <div class="flex items-center gap-2 w-full max-w-[320px] ms-auto">
        <input
          type="range"
          min={0}
          max={1000}
          step={1}
          value={Math.round(pos() * 1000)}
          disabled={disabled()}
          onInput={(e) => {
            const p = Number(e.currentTarget.value) / 1000;
            props.onChange(clamp(fromPos(p, props.min, props.max, log())));
          }}
          class="flex-1 min-w-[90px] accent-accent cursor-pointer disabled:cursor-not-allowed"
        />
        <input
          type="text"
          inputmode="decimal"
          value={text()}
          disabled={disabled()}
          onInput={(e) => {
            setText(e.currentTarget.value);
          }}
          onBlur={commitText}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              commitText();
              e.currentTarget.blur();
            }
          }}
          aria-label={props.label}
          class="w-[76px] shrink-0 rounded-md border border-mid-gray/20 bg-background px-2 py-1 text-xs font-mono text-end focus:outline-none focus:border-accent disabled:opacity-50"
        />
        <span class="w-8 shrink-0 text-[11px] text-mid-gray truncate">
          {props.format ? props.format(props.value) : (props.unit ?? "")}
        </span>
        <button
          type="button"
          onClick={() =>
            props.defaultValue !== undefined &&
            props.onChange(props.defaultValue)
          }
          disabled={disabled() || !showReset()}
          title={t("settings.liveFft.resetValue")}
          aria-label={t("settings.liveFft.resetValue")}
          class={`shrink-0 p-1 rounded-md text-mid-gray hover:text-text hover:bg-mid-gray/15 cursor-pointer disabled:cursor-default ${
            showReset() ? "opacity-100" : "opacity-0 pointer-events-none"
          }`}
        >
          <RotateCcw class="w-3 h-3" />
        </button>
      </div>
    </SettingContainer>
  );
};
