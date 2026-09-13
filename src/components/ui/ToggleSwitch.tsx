import { SettingContainer } from "./SettingContainer";
import type { JSX } from "@solidjs/web";

interface ToggleSwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  isUpdating?: boolean;
  label: string;
  description: string;
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  tooltipPosition?: "top" | "bottom";
}

export const ToggleSwitch = (props: ToggleSwitchProps): JSX.Element => {
  // Accessors, not destructured locals: a component body runs once, so a
  // destructured `checked` would freeze at mount and the switch would never
  // follow a settings change (the browser only updates its own checkbox on
  // click, which is what hid this in a click-driven test).
  const disabled = () => props.disabled ?? false;
  const isUpdating = () => props.isUpdating ?? false;
  return (
    <SettingContainer
      title={props.label}
      description={props.description}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped}
      disabled={disabled()}
      tooltipPosition={props.tooltipPosition}
    >
      <label
        class={`flex items-center ${disabled() || isUpdating() ? "cursor-not-allowed" : "cursor-pointer"}`}
      >
        <input
          type="checkbox"
          value=""
          class="sr-only peer"
          aria-label={props.label}
          checked={props.checked}
          disabled={disabled() || isUpdating()}
          onInput={(e) => props.onChange(e.target.checked)}
        />
        <div class="relative w-11 h-6 bg-mid-gray/20 peer-focus:outline-none peer-focus:ring-4 peer-focus:ring-accent rounded-pill peer peer-checked:after:translate-x-full rtl:peer-checked:after:-translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:start-[2px] after:bg-white after:border-gray-300 after:border after:rounded-pill after:h-5 after:w-5 after:transition-all peer-checked:bg-background-ui peer-disabled:opacity-50"></div>
      </label>
      {isUpdating() && (
        <div class="absolute inset-0 flex items-center justify-center">
          <div class="w-4 h-4 border-2 border-accent border-t-transparent rounded-pill animate-spin"></div>
        </div>
      )}
    </SettingContainer>
  );
};
