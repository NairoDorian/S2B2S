import { createSignal, createEffect } from "solid-js";
import { Input } from "../../ui/Input";
import type { JSX } from "@solidjs/web";

interface BaseUrlFieldProps {
  value: string;
  onBlur: (value: string) => void;
  disabled: boolean;
  placeholder?: string;
  className?: string;
}

export const BaseUrlField = ({
  value,
  onBlur,
  disabled,
  placeholder,
  className = "",
}: BaseUrlFieldProps): JSX.Element => {
  const [localValue, setLocalValue] = createSignal(value);

  createEffect(
    () => undefined,
    () => {
      setLocalValue(value);
    },
  );

  const disabledMessage = disabled
    ? "Base URL is managed by the selected provider."
    : undefined;

  return (
    <Input
      type="text"
      value={localValue()}
      onInput={(e) => setLocalValue(e.target.value)}
      onBlur={() => onBlur(localValue())}
      placeholder={placeholder}
      variant="compact"
      disabled={disabled}
      class={`flex-1 min-w-[360px] ${className}`}
      title={disabledMessage}
    />
  );
};
