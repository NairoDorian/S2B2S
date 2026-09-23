import { createSignal, createEffect, untrack } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { Input } from "../../ui/Input";
import type { JSX } from "@solidjs/web";

interface BaseUrlFieldProps {
  value: string;
  onBlur: (value: string) => void;
  disabled: boolean;
  placeholder?: string;
  className?: string;
}

export const BaseUrlField = (props: BaseUrlFieldProps): JSX.Element => {
  const { t } = useTranslation();
  const [localValue, setLocalValue] = createSignal(untrack(() => props.value));

  // Re-sync the draft whenever the stored URL changes, including a switch to
  // another provider while the field stays mounted.
  createEffect(
    () => props.value,
    (value) => {
      setLocalValue(value);
    },
  );

  return (
    <Input
      type="text"
      value={localValue()}
      onInput={(e) => setLocalValue(e.target.value)}
      onBlur={() => props.onBlur(localValue())}
      placeholder={props.placeholder}
      variant="compact"
      disabled={props.disabled}
      class={`flex-1 min-w-[360px] ${props.className ?? ""}`}
      title={
        props.disabled
          ? t("settings.postProcessing.api.baseUrl.managedByProvider")
          : undefined
      }
    />
  );
};
