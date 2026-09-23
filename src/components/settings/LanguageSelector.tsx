import { createSignal, createEffect, createMemo, For } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { SettingContainer } from "../ui/SettingContainer";
import { ResetButton } from "../ui/ResetButton";
import { useSettings } from "../../hooks/useSettings";
import {
  effectiveLanguage,
  getLanguageLabel,
  SELECTABLE_LANGUAGES,
  supportsLanguageCode,
} from "../../lib/constants/languages";
import type { JSX } from "@solidjs/web";

interface LanguageSelectorProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  supportedLanguages?: string[];
  supportsLanguageDetection?: boolean;
}

export const LanguageSelector = (props: LanguageSelectorProps): JSX.Element => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, resetSetting, isUpdating } = useSettings();
  const [isOpen, setIsOpen] = createSignal(false);
  const [searchQuery, setSearchQuery] = createSignal("");
  let dropdownRef: HTMLDivElement | null = null;
  let searchInputRef: HTMLInputElement | null = null;

  // Accessors, not consts: the selector stays mounted across a model switch
  // (new supported languages) and across a language pick.
  const supportsLanguageDetection = () =>
    props.supportsLanguageDetection ?? true;
  const selectedLanguage = () =>
    effectiveLanguage(
      getSetting("selected_language") || "auto",
      props.supportedLanguages ?? [],
      supportsLanguageDetection(),
    );

  createEffect(
    () => undefined,
    () => {
      const handleClickOutside = (event: MouseEvent) => {
        if (dropdownRef && !dropdownRef.contains(event.target as Node)) {
          setIsOpen(false);
          setSearchQuery("");
        }
      };

      document.addEventListener("mousedown", handleClickOutside);
      return () => {
        document.removeEventListener("mousedown", handleClickOutside);
      };
    },
  );

  createEffect(
    () => isOpen(),
    (open) => {
      if (open && searchInputRef) {
        searchInputRef.focus();
      }
    },
  );

  const availableLanguages = createMemo(() => {
    const supportedLanguages = props.supportedLanguages;
    if (!supportedLanguages || supportedLanguages.length === 0)
      return SELECTABLE_LANGUAGES;
    return SELECTABLE_LANGUAGES.filter((lang) =>
      lang.value === "auto"
        ? supportsLanguageDetection()
        : supportsLanguageCode(supportedLanguages, lang.value),
    );
  });

  const filteredLanguages = createMemo(() =>
    availableLanguages().filter((language) =>
      language.label.toLowerCase().includes(searchQuery().toLowerCase()),
    ),
  );

  const selectedLanguageName = () =>
    getLanguageLabel(selectedLanguage()) || t("settings.general.language.auto");

  const handleLanguageSelect = async (languageCode: string) => {
    await updateSetting("selected_language", languageCode);
    setIsOpen(false);
    setSearchQuery("");
  };

  const handleReset = async () => {
    await resetSetting("selected_language");
  };

  const handleToggle = () => {
    if (isUpdating("selected_language")) return;
    setIsOpen(!isOpen());
  };

  const handleSearchChange = (event: Event) => {
    setSearchQuery((event.target as HTMLInputElement).value);
  };

  const handleKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Enter" && filteredLanguages().length > 0) {
      handleLanguageSelect(filteredLanguages()[0].value);
    } else if (event.key === "Escape") {
      setIsOpen(false);
      setSearchQuery("");
    }
  };

  return (
    <SettingContainer
      title={t("settings.general.language.title")}
      description={t("settings.general.language.description")}
      descriptionMode={props.descriptionMode ?? "tooltip"}
      grouped={props.grouped ?? false}
    >
      <div class="flex items-center space-x-1">
        <div
          class="relative"
          ref={(ref) => {
            dropdownRef = ref;
          }}
        >
          <button
            type="button"
            class={`px-2 py-1 text-sm font-semibold bg-mid-gray/10 border border-mid-gray/80 rounded min-w-[200px] text-start flex items-center justify-between transition-all duration-150 ${
              isUpdating("selected_language")
                ? "opacity-50 cursor-not-allowed"
                : "hover:bg-accent/10 cursor-pointer hover:border-accent"
            }`}
            onClick={handleToggle}
            disabled={isUpdating("selected_language")}
          >
            <span class="truncate">{selectedLanguageName()}</span>
            <svg
              class={`w-4 h-4 ms-2 transition-transform duration-200 ${
                isOpen() ? "transform rotate-180" : ""
              }`}
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

          {isOpen() && !isUpdating("selected_language") && (
            <div class="absolute top-full left-0 right-0 mt-1 bg-background border border-mid-gray/80 rounded shadow-lg z-50 max-h-60 overflow-hidden">
              <div class="p-2 border-b border-mid-gray/80">
                <input
                  ref={(ref) => {
                    searchInputRef = ref;
                  }}
                  type="text"
                  value={searchQuery()}
                  onInput={handleSearchChange}
                  onKeyDown={handleKeyDown}
                  placeholder={t("settings.general.language.searchPlaceholder")}
                  class="w-full px-2 py-1 text-sm bg-mid-gray/10 border border-mid-gray/40 rounded focus:outline-none focus:ring-1 focus:ring-accent focus:border-accent"
                />
              </div>

              <div class="max-h-48 overflow-y-auto">
                {filteredLanguages().length === 0 ? (
                  <div class="px-2 py-2 text-sm text-mid-gray text-center">
                    {t("settings.general.language.noResults")}
                  </div>
                ) : (
                  <For
                    each={filteredLanguages()}
                    keyed={(language) => language.value}
                  >
                    {(language) => (
                      <button
                        type="button"
                        class={`w-full px-2 py-1 text-sm text-start hover:bg-accent/10 transition-colors duration-150 ${
                          selectedLanguage() === language().value
                            ? "bg-accent/20 text-accent font-semibold"
                            : ""
                        }`}
                        onClick={() => handleLanguageSelect(language().value)}
                      >
                        <span class="sr-only">
                          {t("settings.general.language.title")}
                        </span>
                        <div class="flex items-center justify-between">
                          <span class="truncate">{language().label}</span>
                        </div>
                      </button>
                    )}
                  </For>
                )}
              </div>
            </div>
          )}
        </div>
        <ResetButton
          onClick={handleReset}
          disabled={isUpdating("selected_language")}
        />
      </div>
      {isUpdating("selected_language") && (
        <div class="absolute inset-0 bg-mid-gray/10 rounded flex items-center justify-center">
          <div class="w-4 h-4 border-2 border-accent border-t-transparent rounded-full animate-spin"></div>
        </div>
      )}
    </SettingContainer>
  );
};
