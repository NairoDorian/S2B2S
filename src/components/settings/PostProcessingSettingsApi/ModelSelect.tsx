import { useTranslation } from "@/i18n/useTranslation";
import type { ModelOption } from "./types";
import { Select } from "../../ui/Select";
import type { JSX } from "@solidjs/web";

type ModelSelectProps = {
  value: string;
  options: ModelOption[];
  disabled?: boolean;
  placeholder?: string;
  isLoading?: boolean;
  onSelect: (value: string) => void;
  onCreate: (value: string) => void;
  onBlur?: () => void;
  className?: string;
};

export const ModelSelect = (props: ModelSelectProps): JSX.Element => {
  const { t } = useTranslation();

  const handleCreate = (inputValue: string) => {
    const trimmed = inputValue.trim();
    if (!trimmed) return;
    props.onCreate(trimmed);
  };

  return (
    <Select
      class={`text-sm ${props.className ?? "flex-1 min-w-[360px]"}`}
      value={props.value || null}
      options={props.options}
      onChange={(selected) => props.onSelect(selected ?? "")}
      onCreateOption={handleCreate}
      onBlur={() => props.onBlur?.()}
      placeholder={props.placeholder}
      disabled={props.disabled}
      isLoading={props.isLoading}
      isCreatable
      formatCreateLabel={(input) =>
        t("settings.postProcessing.api.model.useCustom", { value: input })
      }
      ariaLabel={t("settings.postProcessing.api.model.title")}
    />
  );
};
