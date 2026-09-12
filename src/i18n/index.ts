import i18n, { type BackendModule } from "i18next";
import { initReactI18next } from "react-i18next";
import { locale } from "@tauri-apps/plugin-os";
import { LANGUAGE_METADATA } from "./languages";
import { commands } from "@/bindings";
import { APP_NAME } from "@/lib/appIdentity";
import {
  getLanguageDirection,
  updateDocumentDirection,
  updateDocumentLanguage,
} from "@/lib/utils/rtl";

// Auto-discover translation files using Vite's glob import. Deliberately not
// `eager`: each locale is its own chunk, imported the first time that language
// is used. Eager bundling put all 24 files (2.1 MB of JSON) into the chunk
// both windows parse at startup, for the one language that is ever read.
const localeModules = import.meta.glob<{ default: Record<string, unknown> }>(
  "./locales/*/translation.json",
);

// Language code -> loader of its translation file
const localeLoaders: Record<
  string,
  () => Promise<{ default: Record<string, unknown> }>
> = {};
for (const [path, load] of Object.entries(localeModules)) {
  const langCode = path.match(/\.\/locales\/(.+)\/translation\.json/)?.[1];
  if (langCode) {
    localeLoaders[langCode] = load;
  }
}

// i18next backend that resolves a language to its lazily imported chunk.
const lazyLocaleBackend: BackendModule = {
  type: "backend",
  init() {},
  read(language, _namespace, callback) {
    const load = localeLoaders[language];
    if (!load) {
      callback(
        new Error(`No translation file for locale "${language}"`),
        false,
      );
      return;
    }
    load().then(
      (module) => callback(null, module.default),
      (error: unknown) =>
        callback(
          error instanceof Error ? error : new Error(String(error)),
          false,
        ),
    );
  },
};

// Build supported languages list from discovered locales + metadata
export const SUPPORTED_LANGUAGES = Object.keys(localeLoaders)
  .map((code) => {
    const meta = LANGUAGE_METADATA[code];
    if (!meta) {
      console.warn(`Missing metadata for locale "${code}" in languages.ts`);
      return { code, name: code, nativeName: code, priority: undefined };
    }
    return {
      code,
      name: meta.name,
      nativeName: meta.nativeName,
      priority: meta.priority,
    };
  })
  .sort((a, b) => {
    // Sort by priority first (lower = higher), then alphabetically
    if (a.priority !== undefined && b.priority !== undefined) {
      return a.priority - b.priority;
    }
    if (a.priority !== undefined) return -1;
    if (b.priority !== undefined) return 1;
    return a.name.localeCompare(b.name);
  });

export type SupportedLanguageCode = string;

// Check if a language code is supported
export const getSupportedLanguage = (
  langCode: string | null | undefined,
): SupportedLanguageCode | null => {
  if (!langCode) return null;

  const normalized = langCode.toLowerCase().replace(/_/g, "-");
  const subtags = normalized.split("-");
  const language = subtags[0];
  const isHant = subtags.includes("hant");
  const isHans = subtags.includes("hans");
  const isTraditionalRegion = ["tw", "hk", "mo"].some((region) =>
    subtags.includes(region),
  );

  // Try exact match first
  let supported = SUPPORTED_LANGUAGES.find(
    (lang) => lang.code.toLowerCase() === normalized,
  );
  if (!supported) {
    let fallback = language;
    if (language === "zh" && (isHant || (!isHans && isTraditionalRegion))) {
      fallback = "zh-tw";
    } else if (language === "yue") {
      // Cantonese uses Traditional Chinese unless explicitly tagged as Hans.
      fallback = isHans ? "zh" : "zh-tw";
    }
    supported = SUPPORTED_LANGUAGES.find(
      (lang) => lang.code.toLowerCase() === fallback,
    );
  }
  return supported ? supported.code : null;
};

// Initialize i18n with English as default; the language is synced from
// settings below. Only the initial language's bundle is loaded here.
// `changeLanguage` loads its target before it switches, so the UI never
// renders raw keys in between.
const initialized = i18n
  .use(lazyLocaleBackend)
  .use(initReactI18next)
  .init({
    lng: "en",
    fallbackLng: "en",
    interpolation: {
      escapeValue: false, // React already escapes values
      // `{{app}}` is available in every string without each key having to pass
      // it. A locale that names the product — "Start with {{app}}", "{{app}}
      // needs some permissions to work properly" — therefore survives a rename
      // untouched, and 25 files never have to be edited for one new word.
      defaultVariables: { app: APP_NAME },
    },
    react: {
      useSuspense: false, // Disable suspense for SSR compatibility
    },
  });

// Sync language from app settings
export const syncLanguageFromSettings = async () => {
  try {
    const result = await commands.getAppSettings();
    const preferred =
      result.status === "ok" && result.data.app_language
        ? result.data.app_language
        : // Fall back to system locale detection if no saved preference
          await locale();
    const supported = getSupportedLanguage(preferred);
    // init() switches to the default language once that bundle has loaded.
    // Wait for it so its switch can never land after ours and so `i18nReady`
    // always implies the fallback bundle is loaded.
    await initialized;
    if (supported && supported !== i18n.language) {
      await i18n.changeLanguage(supported);
    }
  } catch (e) {
    console.warn("Failed to sync language from settings:", e);
  }
};

// Run language sync on init. Both windows wait for this before their first
// render, so the first paint is already in the user's language.
export const i18nReady: Promise<void> = syncLanguageFromSettings();

// Listen for language changes to update HTML dir and lang attributes
i18n.on("languageChanged", (lng) => {
  const dir = getLanguageDirection(lng);
  updateDocumentDirection(dir);
  updateDocumentLanguage(lng);
});

// Re-export RTL utilities for convenience
export { getLanguageDirection, isRTLLanguage } from "@/lib/utils/rtl";

export default i18n;
