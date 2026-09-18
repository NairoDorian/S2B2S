/* oxlint-disable jsx-a11y/prefer-tag-over-role, jsx-a11y/click-events-have-key-events, jsx-a11y/interactive-supports-focus */
import {
  createSignal,
  createEffect,
  createMemo,
  createUniqueId,
  For,
} from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import type { JSX } from "@solidjs/web";

export type SelectOption = {
  value: string;
  label: string;
  isDisabled?: boolean;
};

type BaseProps = {
  value: string | null;
  options: SelectOption[];
  placeholder?: string;
  disabled?: boolean;
  isLoading?: boolean;
  isClearable?: boolean;
  onChange: (value: string | null) => void;
  onBlur?: () => void;
  class?: string;
  ariaLabel?: string;
};

type CreatableProps = {
  isCreatable: true;
  onCreateOption: (value: string) => void;
  formatCreateLabel: (input: string) => string;
};

type NonCreatableProps = {
  isCreatable?: false;
  onCreateOption?: never;
  formatCreateLabel?: never;
};

export type SelectProps = BaseProps & (CreatableProps | NonCreatableProps);

interface MenuItem {
  key: string;
  label: string;
  value: string | null;
  disabled: boolean;
  isCreate: boolean;
}

const CREATE_ITEM_KEY = "\u0000create";
const MAX_MENU_HEIGHT = 300;
const MIN_MENU_HEIGHT = 140;

const matchesQuery = (option: SelectOption, query: string) =>
  `${option.label} ${option.value}`.toLowerCase().includes(query);

const Chevron = () => (
  <svg
    class="h-5 w-5"
    viewBox="0 0 20 20"
    aria-hidden="true"
    fill="currentColor"
  >
    <path d="M4.516 7.548c0.436-0.446 1.043-0.481 1.576 0l3.908 3.747 3.908-3.747c0.533-0.481 1.141-0.446 1.574 0 0.436 0.445 0.408 1.197 0 1.615-0.406 0.418-4.695 4.502-4.695 4.502-0.217 0.223-0.502 0.335-0.787 0.335s-0.57-0.112-0.789-0.335c0 0-4.287-4.084-4.695-4.502s-0.436-1.17 0-1.615z" />
  </svg>
);

const Cross = () => (
  <svg
    class="h-5 w-5"
    viewBox="0 0 20 20"
    aria-hidden="true"
    fill="currentColor"
  >
    <path d="M14.348 14.849c-0.469 0.469-1.229 0.469-1.697 0l-2.651-3.030-2.651 3.029c-0.469 0.469-1.229 0.469-1.697 0-0.469-0.469-0.469-1.229 0-1.697l2.758-3.15-2.759-3.152c-0.469-0.469-0.469-1.228 0-1.697s1.228-0.469 1.697 0l2.652 3.031 2.651-3.031c0.469-0.469 1.228-0.469 1.697 0s0.469 1.229 0 1.697l-2.758 3.152 2.758 3.15c0.469 0.469 0.469 1.229 0 1.698z" />
  </svg>
);

export const Select = (props: SelectProps): JSX.Element => {
  const creatable = props.isCreatable === true ? props : undefined;
  const isClearable = () => props.isClearable ?? true;
  const className = () => props.class ?? "";
  const { t } = useTranslation();

  const [isOpen, setIsOpen] = createSignal(false);
  const [inputValue, setInputValue] = createSignal("");
  const [focusedIndex, setFocusedIndex] = createSignal(-1);
  const [focused, setFocused] = createSignal(false);
  const [placement, setPlacement] = createSignal<"bottom" | "top">("bottom");
  const [availableHeight, setAvailableHeight] = createSignal(MAX_MENU_HEIGHT);

  let rootRef: HTMLDivElement | null = null;
  let inputRef: HTMLInputElement | null = null;
  let listRef: HTMLDivElement | null = null;

  const listboxId = createUniqueId();
  const optionId = (index: number) => `${listboxId}-option-${index}`;

  const selectedOption = createMemo((): SelectOption | null => {
    const current = props.value;
    if (!current) return null;
    return (
      props.options.find((option) => option.value === current) ?? {
        value: current,
        label: current,
      }
    );
  });

  const items = createMemo((): MenuItem[] => {
    const query = inputValue().trim().toLowerCase();
    const matched: MenuItem[] = props.options
      .filter((option) => query === "" || matchesQuery(option, query))
      .map((option) => ({
        key: option.value,
        label: option.label,
        value: option.value,
        disabled: option.isDisabled === true,
        isCreate: false,
      }));

    const formatCreateLabel = creatable?.formatCreateLabel;
    if (!formatCreateLabel || props.isLoading) return matched;

    const candidate = inputValue().toLowerCase();
    const taken = new Set(
      props.options.flatMap((option) => [
        option.value.toLowerCase(),
        option.label.toLowerCase(),
      ]),
    );
    const current = props.value;
    if (current) taken.add(current.toLowerCase());
    if (inputValue() === "" || taken.has(candidate)) return matched;

    return [
      ...matched,
      {
        key: CREATE_ITEM_KEY,
        label: formatCreateLabel(inputValue()),
        value: null,
        disabled: false,
        isCreate: true,
      },
    ];
  });

  createEffect(
    () => undefined,
    () => {
      setFocusedIndex(items().findIndex((item) => !item.disabled));
    },
  );

  createEffect(
    () => undefined,
    () => {
      if (!isOpen() || focusedIndex() < 0) return;
      listRef
        ?.querySelector(`[data-index="${focusedIndex()}"]`)
        ?.scrollIntoView({ block: "nearest" });
    },
  );

  const closeMenu = () => {
    setIsOpen(false);
    setInputValue("");
    setFocusedIndex(-1);
  };

  const stepFocus = (from: number, step: number): number => {
    const count = items().length;
    if (count === 0) return -1;
    let index = from < 0 ? (step > 0 ? -1 : 0) : from;
    for (let i = 0; i < count; i++) {
      index = (index + step + count) % count;
      if (!items()[index]?.disabled) return index;
    }
    return -1;
  };

  const openMenu = (where: "first" | "last") => {
    if (props.disabled) return;
    const rect = rootRef?.getBoundingClientRect();
    if (rect) {
      const below = window.innerHeight - rect.bottom;
      const flip = below < MIN_MENU_HEIGHT && rect.top > below;
      setPlacement(flip ? "top" : "bottom");
      setAvailableHeight(
        Math.min(
          MAX_MENU_HEIGHT,
          Math.max(MIN_MENU_HEIGHT, flip ? rect.top : below),
        ),
      );
    }
    setIsOpen(true);
    setFocusedIndex(
      where === "first" ? stepFocus(-1, 1) : stepFocus(items().length, -1),
    );
  };

  const selectItem = (item: MenuItem | undefined) => {
    if (!item || item.disabled) return;
    closeMenu();
    if (item.isCreate) {
      creatable?.onCreateOption(inputValue());
      return;
    }
    props.onChange(item.value);
  };

  const clearValue = () => {
    props.onChange(null);
    inputRef?.focus();
  };

  const handleKeyDown = (event: KeyboardEvent) => {
    if (props.disabled) return;
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        if (isOpen()) setFocusedIndex((index) => stepFocus(index, 1));
        else openMenu("first");
        break;
      case "ArrowUp":
        event.preventDefault();
        if (isOpen()) setFocusedIndex((index) => stepFocus(index, -1));
        else openMenu("last");
        break;
      case "Home":
        if (!isOpen()) return;
        event.preventDefault();
        setFocusedIndex(stepFocus(-1, 1));
        break;
      case "End":
        if (!isOpen()) return;
        event.preventDefault();
        setFocusedIndex(stepFocus(items().length, -1));
        break;
      case "Enter":
        if (event.isComposing) return;
        if (!isOpen() || focusedIndex() < 0) return;
        event.preventDefault();
        selectItem(items()[focusedIndex()]);
        break;
      case "Tab":
        if (event.shiftKey || !isOpen() || focusedIndex() < 0) return;
        selectItem(items()[focusedIndex()]);
        break;
      case "Escape":
        if (!isOpen()) return;
        event.preventDefault();
        closeMenu();
        break;
      case " ":
        if (inputValue() !== "") return;
        if (!isOpen()) {
          event.preventDefault();
          openMenu("first");
        } else if (focusedIndex() >= 0) {
          event.preventDefault();
          selectItem(items()[focusedIndex()]);
        }
        break;
      case "Backspace":
      case "Delete":
        if (inputValue() !== "" || !isClearable() || !selectedOption()) return;
        event.preventDefault();
        clearValue();
        break;
      default:
        break;
    }
  };

  const handleBlur = (
    event: FocusEvent & { currentTarget: HTMLDivElement },
  ) => {
    const next = event.relatedTarget;
    if (next instanceof Node && event.currentTarget.contains(next)) return;
    setFocused(false);
    closeMenu();
    props.onBlur?.();
  };

  const handleControlMouseDown = (event: MouseEvent) => {
    if (props.disabled) return;
    if (event.target !== inputRef) event.preventDefault();
    inputRef?.focus();
    if (!isOpen()) openMenu("first");
  };

  const showClear =
    isClearable() &&
    selectedOption() !== null &&
    !props.disabled &&
    !props.isLoading;

  const controlClass = `flex min-h-10 w-full items-center rounded-md border text-sm transition-all duration-150 ${props.disabled ? "cursor-not-allowed border-mid-gray/80 bg-mid-gray/10" : `cursor-text ${focused() || isOpen() ? "border-accent bg-accent/20 ring-1 ring-accent" : "border-mid-gray/80 bg-mid-gray/10 hover:border-accent hover:bg-accent/12"}`}`;
  const indicatorClass =
    "flex p-2 transition-colors duration-150 disabled:cursor-not-allowed";

  return (
    <div
      ref={(el) => {
        rootRef = el;
      }}
      class={`relative ${className()}`}
      onBlur={handleBlur}
    >
      <div
        role="presentation"
        class={controlClass}
        onMouseDown={handleControlMouseDown}
      >
        <div class="relative min-w-0 flex-1 px-2.5 py-1.5">
          {inputValue() === "" && (
            <span
              class={`pointer-events-none absolute inset-x-2.5 inset-y-1.5 truncate ${selectedOption() ? "text-text" : "text-mid-gray/65"}`}
            >
              {selectedOption()?.label ?? props.placeholder}
            </span>
          )}
          <input
            ref={(el) => {
              inputRef = el;
            }}
            class="relative w-full bg-transparent text-text outline-none"
            type="text"
            role="combobox"
            aria-expanded={isOpen() ? "true" : "false"}
            aria-haspopup="listbox"
            aria-autocomplete="list"
            aria-controls={isOpen() ? listboxId : undefined}
            aria-activedescendant={
              isOpen() && focusedIndex() >= 0
                ? optionId(focusedIndex())
                : undefined
            }
            aria-label={props.ariaLabel}
            autocomplete="off"
            autocorrect="off"
            spellcheck="false"
            disabled={props.disabled}
            value={inputValue()}
            placeholder=""
            onInput={(event) => {
              setInputValue(event.target.value);
              setIsOpen(true);
            }}
            onKeyDown={handleKeyDown}
            onFocus={() => {
              setFocused(true);
            }}
          />
        </div>

        {showClear && (
          <button
            type="button"
            aria-hidden="true"
            tabindex={-1}
            class={`${indicatorClass} text-mid-gray/80 hover:text-accent`}
            onMouseDown={(event) => {
              event.preventDefault();
              event.stopPropagation();
              clearValue();
            }}
          >
            <Cross />
          </button>
        )}

        <span
          aria-hidden="true"
          class="my-0.5 w-0.5 self-stretch bg-[hsl(0,0%,80%)]"
        />

        <button
          type="button"
          aria-hidden="true"
          tabindex={-1}
          class={`${indicatorClass} ${focused() || isOpen() ? "text-accent" : "text-mid-gray/80 hover:text-accent"}`}
          onMouseDown={(event) => {
            event.preventDefault();
            event.stopPropagation();
            if (props.disabled) return;
            inputRef?.focus();
            if (isOpen()) closeMenu();
            else openMenu("first");
          }}
        >
          <Chevron />
        </button>
      </div>

      {isOpen() && (
        <div
          role="presentation"
          onMouseDown={(event) => event.preventDefault()}
          class={`absolute inset-x-0 z-30 rounded border border-mid-gray/30 bg-background text-text shadow-[0_10px_30px_rgba(15,15,15,0.2)] ${placement() === "top" ? "bottom-full mb-1" : "top-full mt-1"}`}
        >
          <div
            ref={(el) => {
              listRef = el;
            }}
            id={listboxId}
            role="listbox"
            class="overflow-y-auto py-1"
            style={{ "max-height": `${availableHeight()}px` }}
          >
            {props.isLoading ? (
              <div class="px-2 py-2 text-center text-mid-gray">
                {t("common.loading")}
              </div>
            ) : items().length === 0 ? (
              <div class="px-2 py-2 text-center text-mid-gray">
                {t("common.noOptionsFound")}
              </div>
            ) : (
              <For each={items()}>
                {(item, i) => (
                  <div
                    id={optionId(i())}
                    data-index={i()}
                    role="option"
                    aria-selected={i() === focusedIndex() ? "true" : "false"}
                    aria-disabled={item.disabled ? "true" : undefined}
                    class={`px-3 py-2 ${item.disabled ? "cursor-not-allowed opacity-50" : "cursor-pointer"} ${i() === focusedIndex() && !item.disabled ? "bg-accent/12" : item.value !== null && item.value === props.value ? "bg-accent/20" : ""}`}
                    onMouseEnter={() => setFocusedIndex(i())}
                    onClick={() => selectItem(item)}
                  >
                    {item.label}
                  </div>
                )}
              </For>
            )}
          </div>
        </div>
      )}
    </div>
  );
};
