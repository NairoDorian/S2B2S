import { Dropdown, type DropdownOption } from "../../ui/Dropdown";
import type { JSX } from "@solidjs/web";

interface ProviderSelectProps {
  options: DropdownOption[];
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}

export const ProviderSelect = ({
  options,
  value,
  onChange,
  disabled,
}: ProviderSelectProps): JSX.Element => {
  return (
    <Dropdown
      options={options}
      selectedValue={value}
      onSelect={onChange}
      disabled={disabled}
      class="flex-1"
    />
  );
};
