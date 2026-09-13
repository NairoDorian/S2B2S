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
  onBlur: () => void;
  className?: string;
};

export const ModelSelect = ({
  value,
  options,
  disabled,
  placeholder,
  isLoading,
  onSelect,
  onCreate,
  onBlur,
  className = "flex-1 min-w-[360px]",
}: ModelSelectProps): JSX.Element => {
  const { t } = useTranslation();

  const handleCreate = (inputValue: string) => {
    const trimmed = inputValue.trim();
    if (!trimmed) return;
    onCreate(trimmed);
  };

  const computedClassName = `text-sm ${className}`;

  return (
    <Select
      class={computedClassName}
      value={value || null}
      options={options}
      onChange={(selected) => onSelect(selected ?? "")}
      onCreateOption={handleCreate}
      onBlur={onBlur}
      placeholder={placeholder}
      disabled={disabled}
      isLoading={isLoading}
      isCreatable
      formatCreateLabel={(input) => `Use "${input}"`}
      ariaLabel={t("settings.postProcessing.api.model.title")}
    />
  );
};
