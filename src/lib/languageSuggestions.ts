import { MODEL_CAPABILITY_LANGUAGES } from "@/lib/constants/languages";

/** How many languages a suggestion offers before English rides along. */
const MAX_SUGGESTED = 3;

/** Codes the OS spells differently from the picker. */
const PICKER_ALIASES = new Map([
  ["nb", "no"], // Norwegian Bokmål → Norwegian
  ["fil", "tl"], // Filipino → Tagalog
  ["iw", "he"], // the legacy Hebrew code
  ["jv", "jw"], // the picker keeps Whisper's Javanese code
]);

const OFFERED = new Set(MODEL_CAPABILITY_LANGUAGES.map((l) => l.value));

/**
 * The picker's code for a BCP-47 tag from the OS: its primary subtag, with
 * the picker's spellings (`nb-NO` → `no`, `zh-Hant-TW` → `zh`,
 * `yue-Hant-HK` → `yue`).
 */
export const pickerLanguage = (tag: string): string => {
  const base = tag.split(/[-_]/)[0].toLowerCase();
  return PICKER_ALIASES.get(base) ?? base;
};

/**
 * What "Languages I speak" suggests from the OS's preferred languages (and
 * any other tags passed after them, such as the browser locale): up to three
 * languages the picker offers, in order, without repeats, plus English.
 * Upstream's rule (`system_languages::suggested_languages`). It is only ever
 * offered: the list is the engine's language allowlist, so the user confirms
 * it.
 */
export const suggestedSpokenLanguages = (tags: string[]): string[] => {
  const languages: string[] = [];
  for (const language of tags.map(pickerLanguage)) {
    if (languages.length === MAX_SUGGESTED) break;
    if (OFFERED.has(language) && !languages.includes(language)) {
      languages.push(language);
    }
  }
  if (!languages.includes("en")) languages.push("en");
  return languages;
};
