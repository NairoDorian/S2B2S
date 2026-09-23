import { Dropdown, type DropdownOption } from "../../ui/Dropdown";
import type { JSX } from "@solidjs/web";

interface ProviderSelectProps {
  options: DropdownOption[];
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}

export const ProviderSelect = (props: ProviderSelectProps): JSX.Element => {
  return (
    <Dropdown
      options={props.options}
      selectedValue={props.value}
      onSelect={(value) => props.onChange(value)}
      disabled={props.disabled}
      class="flex-1"
    />
  );
};
