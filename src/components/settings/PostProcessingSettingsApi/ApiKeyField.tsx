import { createSignal, createEffect, untrack } from "solid-js";
import { Input } from "../../ui/Input";
import type { JSX } from "@solidjs/web";

interface ApiKeyFieldProps {
  value: string;
  onBlur: (value: string) => void;
  disabled: boolean;
  placeholder?: string;
  className?: string;
}

export const ApiKeyField = (props: ApiKeyFieldProps): JSX.Element => {
  const [localValue, setLocalValue] = createSignal(untrack(() => props.value));

  // Re-sync the draft whenever the stored key changes — including a switch to
  // another provider while the field stays mounted, so a blur can never write
  // the previous provider's key into the new one.
  createEffect(
    () => props.value,
    (value) => {
      setLocalValue(value);
    },
  );

  return (
    <Input
      type="password"
      value={localValue()}
      onInput={(e) => setLocalValue(e.target.value)}
      onBlur={() => props.onBlur(localValue())}
      placeholder={props.placeholder}
      variant="compact"
      disabled={props.disabled}
      class={`flex-1 min-w-[320px] ${props.className ?? ""}`}
    />
  );
};
