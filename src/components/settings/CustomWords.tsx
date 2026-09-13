import { createSignal, For } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { sessionToast as toast } from "@/lib/sessionToast";
import { useSettings } from "../../hooks/useSettings";
import { Input } from "../ui/Input";
import { Button } from "../ui/Button";
import { SettingContainer } from "../ui/SettingContainer";

interface CustomWordsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

const normalizeCustomWord = (word: string) =>
  word
    .replace(/[<>"']/g, "")
    .replace(/\s+/g, " ")
    .trim();

export const CustomWords = (props: CustomWordsProps) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const [newWord, setNewWord] = createSignal("");
  // Read at call time: the word list lives in the settings store, and both
  // the duplicate check and the remove filter must see the current list.
  const customWords = () => getSetting("custom_words") || [];
  const normalizedWord = () => normalizeCustomWord(newWord());

  const handleAddWord = () => {
    const word = normalizedWord();
    const words = customWords();
    if (word && word.length <= 50) {
      if (words.includes(word)) {
        toast.error(
          t("settings.advanced.customWords.duplicate", {
            word: word,
          }),
        );
        return;
      }
      updateSetting("custom_words", [...words, word]);
      setNewWord("");
    }
  };

  const handleRemoveWord = (wordToRemove: string) => {
    updateSetting(
      "custom_words",
      customWords().filter((word) => word !== wordToRemove),
    );
  };

  const handleKeyPress = (e: KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      handleAddWord();
    }
  };

  return (
    <>
      <SettingContainer
        title={t("settings.advanced.customWords.title")}
        description={t("settings.advanced.customWords.description")}
        descriptionMode={props.descriptionMode}
        grouped={props.grouped}
      >
        <div class="flex items-center gap-2">
          <Input
            type="text"
            class="max-w-40"
            value={newWord()}
            onChange={(e) => setNewWord(e.target.value)}
            onKeyDown={handleKeyPress}
            placeholder={t("settings.advanced.customWords.placeholder")}
            variant="compact"
            disabled={isUpdating("custom_words")}
          />
          <Button
            onClick={handleAddWord}
            disabled={
              !normalizedWord() ||
              normalizedWord().length > 50 ||
              isUpdating("custom_words")
            }
            variant="primary"
            size="md"
          >
            {t("settings.advanced.customWords.add")}
          </Button>
        </div>
      </SettingContainer>
      {customWords().length > 0 && (
        <div
          class={`px-4 p-2 ${props.grouped ? "" : "rounded-lg border border-mid-gray/20"} flex flex-wrap gap-1`}
        >
          <For each={customWords()}>
            {(word) => (
              <Button
                onClick={() => handleRemoveWord(word)}
                disabled={isUpdating("custom_words")}
                variant="secondary"
                size="sm"
                class="inline-flex items-center gap-1 cursor-pointer"
                aria-label={t("settings.advanced.customWords.remove", {
                  word: word,
                })}
              >
                <span>{word}</span>
                <svg
                  class="w-3 h-3"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width={2}
                    d="M6 18L18 6M6 6l12 12"
                  />
                </svg>
              </Button>
            )}
          </For>
        </div>
      )}
    </>
  );
};
