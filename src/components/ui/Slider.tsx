import { SettingContainer } from "./SettingContainer";
import { ResetButton } from "./ResetButton";
import type { JSX } from "@solidjs/web";

interface SliderProps {
  value: number;
  onChange: (value: number) => void;
  min: number;
  max: number;
  step?: number;
  disabled?: boolean;
  label: string;
  description: string;
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  showValue?: boolean;
  formatValue?: (value: number) => string;
  onReset?: () => void;
  isResetting?: boolean;
}

export const Slider = (props: SliderProps): JSX.Element => {
  // Accessors, not destructured locals: a component body runs once, so a
  // destructured prop is a mount-time snapshot and the slider would never
  // follow a settings change (or a reset written by another surface).
  const disabled = () => props.disabled ?? false;
  const showValue = () => props.showValue ?? true;
  const formatValue = () => props.formatValue ?? ((v: number) => v.toFixed(2));
  const pct = () => ((props.value - props.min) / (props.max - props.min)) * 100;

  const handleChange = (e: Event) => {
    props.onChange(parseFloat((e.target as HTMLInputElement).value));
  };

  return (
    <SettingContainer
      title={props.label}
      description={props.description}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
      layout="horizontal"
      disabled={disabled()}
    >
      <div class="w-full">
        <div class="flex items-center space-x-1 h-6">
          <input
            type="range"
            min={props.min}
            max={props.max}
            step={props.step ?? 0.01}
            value={props.value}
            onInput={handleChange}
            disabled={disabled()}
            class="grow h-2 rounded-lg appearance-none cursor-pointer focus:outline-none focus:ring-2 focus:ring-accent disabled:opacity-50 disabled:cursor-not-allowed"
            style={{
              background: `linear-gradient(to right, var(--color-background-ui) ${pct()}%, rgba(128, 128, 128, 0.2) ${pct()}%)`,
            }}
          />
          {showValue() && (
            <span class="text-sm font-medium text-text/90 w-12 text-end">
              {formatValue()(props.value)}
            </span>
          )}
          {props.onReset && (
            <ResetButton
              onClick={props.onReset}
              disabled={disabled() || (props.isResetting ?? false)}
            />
          )}
        </div>
      </div>
    </SettingContainer>
  );
};
