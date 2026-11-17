import { createSignal, createEffect, For } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import type { JSX } from "@solidjs/web";
import { isComposingKeyEvent } from "@/lib/utils/keyboard";

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
  /**
   * Called on the open transition, before the menu is shown. The menu stays
   * mounted while closed, so a caller whose option list is fetched (devices,
   * backends) refetches here rather than on mount.
   */
  onRefresh?: () => void;
}

export const Dropdown = (props: DropdownProps): JSX.Element => {
  const { t } = useTranslation();
  const [isOpen, setIsOpen] = createSignal(false);
  let dropdownRef: HTMLDivElement | null = null;
  let triggerRef: HTMLButtonElement | null = null;

  // The compute tracks `isOpen`; the apply owns the outside-click and Escape
  // listeners and returns their teardown, so closing removes them. (Reading
  // `isOpen()` in the apply is untracked: the listener would register once
  // and never go away, and every mount would warn STRICT_READ_UNTRACKED.)
  createEffect(
    () => isOpen(),
    (open) => {
      if (!open) return;
      const handleClickOutside = (event: MouseEvent) => {
        if (dropdownRef && !dropdownRef.contains(event.target as Node)) {
          setIsOpen(false);
        }
      };
      // Escape closes the open menu and hands focus back to its trigger, so
      // a keyboard user is not left on an item that just disappeared. It is
      // captured on the document so this menu consumes it before an
      // enclosing dialog's own Escape handler closes the dialog too.
      const handleEscape = (event: KeyboardEvent) => {
        if (event.key !== "Escape" || props.disabled) return;
        if (isComposingKeyEvent(event)) return;
        event.preventDefault();
        event.stopPropagation();
        setIsOpen(false);
        triggerRef?.focus();
      };
      document.addEventListener("mousedown", handleClickOutside);
      document.addEventListener("keydown", handleEscape, true);
      return () => {
        document.removeEventListener("mousedown", handleClickOutside);
        document.removeEventListener("keydown", handleEscape, true);
      };
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
      class={`app-menu relative ${props.class ?? ""}`}
      ref={(el) => {
        dropdownRef = el;
      }}
    >
      <button
        ref={(el) => {
          triggerRef = el;
        }}
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
