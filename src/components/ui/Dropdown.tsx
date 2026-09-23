import { createSignal, createEffect, For } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import type { JSX } from "@solidjs/web";

export interface DropdownOption {
  value: string;
  label: string;
  description?: string;
  disabled?: boolean;
}

interface DropdownProps {
  options: DropdownOption[];
  class?: string;
  menuClassName?: string;
  selectedValue: string | null;
  onSelect: (value: string) => void;
  placeholder?: string;
  disabled?: boolean;
  onRefresh?: () => void;
}

export const Dropdown = (props: DropdownProps): JSX.Element => {
  const { t } = useTranslation();
  const [isOpen, setIsOpen] = createSignal(false);
  let dropdownRef: HTMLDivElement | null = null;

  // The compute tracks `isOpen`; the apply owns the outside-click listener
  // and returns its teardown, so closing removes it. (Reading `isOpen()` in
  // the apply is untracked: the listener would register once and never go
  // away, and every mount would warn STRICT_READ_UNTRACKED.)
  createEffect(
    () => isOpen(),
    (open) => {
      if (!open) return;
      const handleClickOutside = (event: MouseEvent) => {
        if (dropdownRef && !dropdownRef.contains(event.target as Node)) {
          setIsOpen(false);
        }
      };
      document.addEventListener("mousedown", handleClickOutside);
      return () =>
        document.removeEventListener("mousedown", handleClickOutside);
    },
  );

  const selectedOption = () =>
    props.options.find((option) => option.value === props.selectedValue);

  const handleSelect = (value: string) => {
    props.onSelect(value);
    setIsOpen(false);
  };

  const handleToggle = () => {
    if (props.disabled) return;
    if (!isOpen() && props.onRefresh) props.onRefresh();
    setIsOpen(!isOpen());
  };

  return (
    <div
      class={`relative ${props.class ?? ""}`}
      ref={(el) => {
        dropdownRef = el;
      }}
    >
      <button
        type="button"
        class={`px-2 py-[5px] text-sm font-semibold bg-mid-gray/10 border border-mid-gray/80 rounded-md min-w-[200px] w-full text-start grid grid-cols-[1fr_auto] gap-2 items-center transition-all duration-150 ${props.disabled ? "opacity-50 cursor-not-allowed" : "hover:bg-accent/10 cursor-pointer hover:border-accent"}`}
        onClick={handleToggle}
        disabled={props.disabled}
      >
        <span class="truncate">
          {selectedOption()?.label ||
            (props.placeholder ?? t("common.selectOption"))}
        </span>
        <svg
          class={`w-4 h-4 transition-transform duration-200 ${isOpen() ? "transform rotate-180" : ""}`}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width={2}
            d="M19 9l-7 7-7-7"
          />
        </svg>
      </button>
      {isOpen() && !props.disabled && (
        <div
          class={`absolute top-full mt-1 bg-background border border-mid-gray/80 rounded-md shadow-lg z-50 max-h-60 overflow-y-auto ${props.menuClassName ?? "left-0 right-0"}`}
        >
          {props.options.length === 0 ? (
            <div class="px-2 py-1 text-sm text-mid-gray">
              {t("common.noOptionsFound")}
            </div>
          ) : (
            <For each={props.options}>
              {(option) => (
                <button
                  type="button"
                  class={`w-full text-sm text-start hover:bg-accent/10 transition-colors duration-150 ${option.description ? "px-3 py-2" : "px-2 py-1"} ${props.selectedValue === option.value ? "bg-accent/20" : ""} ${option.disabled ? "opacity-50 cursor-not-allowed" : ""}`}
                  onClick={() => handleSelect(option.value)}
                  disabled={option.disabled}
                >
                  <span
                    class={`block whitespace-normal break-words ${option.description || props.selectedValue === option.value ? "font-semibold" : ""}`}
                  >
                    {option.label}
                  </span>
                  {option.description && (
                    <span class="mt-0.5 block whitespace-normal text-xs font-normal leading-snug text-mid-gray">
                      {option.description}
                    </span>
                  )}
                </button>
              )}
            </For>
          )}
        </div>
      )}
    </div>
  );
};
