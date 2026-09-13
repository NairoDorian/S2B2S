import { createSignal, createEffect } from "solid-js";
import { Input } from "../../ui/Input";
import type { JSX } from "@solidjs/web";

interface ApiKeyFieldProps {
  value: string;
  onBlur: (value: string) => void;
  disabled: boolean;
  placeholder?: string;
  className?: string;
}

export const ApiKeyField = ({
  value,
  onBlur,
  disabled,
  placeholder,
  className = "",
}: ApiKeyFieldProps): JSX.Element => {
  const [localValue, setLocalValue] = createSignal(value);

  createEffect(
    () => undefined,
    () => {
      setLocalValue(value);
    },
  );

  return (
    <Input
      type="password"
      value={localValue()}
      onInput={(e) => setLocalValue(e.target.value)}
      onBlur={() => onBlur(localValue())}
      placeholder={placeholder}
      variant="compact"
      disabled={disabled}
      class={`flex-1 min-w-[320px] ${className}`}
    />
  );
};
