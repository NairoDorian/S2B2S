import { createEffect, createMemo, createSignal, For, Show } from "solid-js";
import type { JSX } from "@solidjs/web";
import { useTranslation } from "@/i18n/useTranslation";
import { SettingContainer } from "../ui/SettingContainer";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import {
  getLanguageLabel,
  MODEL_CAPABILITY_LANGUAGES,
} from "../../lib/constants/languages";
import { isComposingKeyEvent } from "@/lib/utils/keyboard";

interface SpokenLanguagesProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * "Languages I speak": the list `spoken_languages` holds, as removable chips
 * plus a searchable menu to add one. While the language is Auto, a model with
 * an allowlist is limited to these (the backend's run plan decides where it
 * applies); the order is the user's preference order.
 */
export const SpokenLanguages = (props: SpokenLanguagesProps): JSX.Element => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const [isOpen, setIsOpen] = createSignal(false);
  const [query, setQuery] = createSignal("");
  let menuRef: HTMLDivElement | undefined;
  let searchRef: HTMLInputElement | undefined;
  let addRef: HTMLButtonElement | undefined;

  // Accessors: the list changes under the mounted component.
  const spoken = () => getSetting("spoken_languages") ?? [];
  const busy = () => isUpdating("spoken_languages");
  const label = (code: string) => getLanguageLabel(code) ?? code;

  const addable = createMemo(() => {
    const chosen = new Set(spoken());
    const needle = query().trim().toLowerCase();
    return MODEL_CAPABILITY_LANGUAGES.filter(
      (language) =>
        !chosen.has(language.value) &&
        (needle === "" ||
          language.label.toLowerCase().includes(needle) ||
          language.value.toLowerCase() === needle),
    );
  });

  const close = (refocus: boolean) => {
    setIsOpen(false);
    setQuery("");
    if (refocus) addRef?.focus();
  };

  // The open menu owns its outside-click and Escape listeners; closing
  // removes them (the apply is untracked, so the compute reads `isOpen`).
  createEffect(
    () => isOpen(),
    (open) => {
      if (!open) return;
      searchRef?.focus();
      const onMouseDown = (event: MouseEvent) => {
        if (menuRef && !menuRef.contains(event.target as Node)) close(false);
      };
      document.addEventListener("mousedown", onMouseDown);
      return () => document.removeEventListener("mousedown", onMouseDown);
    },
  );

  const write = (next: string[]) => updateSetting("spoken_languages", next);

  const add = (code: string) => {
    void write([...spoken(), code]);
    setQuery("");
    searchRef?.focus();
  };

  const remove = (code: string) =>
    void write(spoken().filter((language) => language !== code));

  const onSearchKeyDown = (event: KeyboardEvent) => {
    if (isComposingKeyEvent(event)) return;
    if (event.key === "Enter") {
      event.preventDefault();
      const first = addable()[0];
      if (first) add(first.value);
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      if (query() !== "") setQuery("");
      else close(true);
    }
  };

  return (
    <>
      <SettingContainer
        title={t("settings.general.spokenLanguages.title")}
        description={t("settings.general.spokenLanguages.description")}
        descriptionMode={props.descriptionMode ?? "tooltip"}
        grouped={props.grouped ?? false}
      >
        <div class="flex flex-wrap items-center justify-end gap-1 max-w-[340px]">
          <For each={spoken()} keyed={(code) => code}>
            {(code) => (
              <span class="inline-flex items-center gap-1 ps-2 pe-1 py-0.5 text-xs font-semibold border border-mid-gray/60 bg-mid-gray/10 rounded">
                {label(code())}
                <button
                  type="button"
                  class="px-1 text-mid-gray hover:text-accent cursor-pointer disabled:opacity-50"
                  aria-label={t("languagePicker.remove", {
                    language: label(code()),
                  })}
                  disabled={busy()}
                  onClick={() => remove(code())}
                >
                  {"×"}
                </button>
              </span>
            )}
          </For>
          <div
            class="relative"
            ref={(el) => {
              menuRef = el;
            }}
          >
            <button
              type="button"
              ref={(el) => {
                addRef = el;
              }}
              class="px-2 py-0.5 text-xs font-semibold border border-mid-gray/80 bg-mid-gray/10 rounded hover:bg-accent/10 hover:border-accent cursor-pointer disabled:opacity-50"
              aria-expanded={isOpen() ? "true" : "false"}
              disabled={busy()}
              onClick={() => (isOpen() ? close(false) : setIsOpen(true))}
            >
              {spoken().length === 0
                ? t("settings.general.spokenLanguages.add")
                : t("languagePicker.addAnother")}
            </button>
            <Show when={isOpen()}>
              <div class="absolute top-full end-0 mt-1 w-56 bg-background border border-mid-gray/80 rounded shadow-lg z-50 overflow-hidden">
                <div class="p-2 border-b border-mid-gray/80">
                  <input
                    ref={(el) => {
                      searchRef = el;
                    }}
                    type="text"
                    autocomplete="off"
                    spellcheck={false}
                    aria-label={t("languagePicker.search")}
                    placeholder={t("languagePicker.search")}
                    value={query()}
                    onInput={(event) => setQuery(event.currentTarget.value)}
                    onKeyDown={onSearchKeyDown}
                    class="w-full px-2 py-1 text-sm bg-mid-gray/10 border border-mid-gray/40 rounded focus:outline-none focus:ring-1 focus:ring-accent focus:border-accent"
                  />
                </div>
                <div class="max-h-48 overflow-y-auto">
                  <Show
                    when={addable().length > 0}
                    fallback={
                      <div class="px-2 py-2 text-sm text-mid-gray text-center">
                        {t("settings.general.language.noResults")}
                      </div>
                    }
                  >
                    <For each={addable()} keyed={(language) => language.value}>
                      {(language) => (
                        <button
                          type="button"
                          class="w-full px-2 py-1 text-sm text-start hover:bg-accent/10 transition-colors duration-150 cursor-pointer"
                          onClick={() => add(language().value)}
                        >
                          {language().label}
                        </button>
                      )}
                    </For>
                  </Show>
                </div>
              </div>
            </Show>
          </div>
        </div>
      </SettingContainer>
      <Show when={spoken().length > 0}>
        <ToggleSwitch
          checked={getSetting("spoken_languages_any_letters") ?? false}
          onChange={(enabled) =>
            updateSetting("spoken_languages_any_letters", enabled)
          }
          isUpdating={isUpdating("spoken_languages_any_letters")}
          label={t("settings.general.spokenLanguages.anyLetters.label")}
          description={t(
            "settings.general.spokenLanguages.anyLetters.description",
          )}
          descriptionMode={props.descriptionMode ?? "tooltip"}
          grouped={props.grouped ?? false}
        />
      </Show>
    </>
  );
};
