import type { TFunction } from "i18next";
import type { LanguageStatus } from "@/bindings";

/**
 * The one-line explanation of what Auto does with a model and the spoken
 * languages, or `null` when there is nothing to explain (Auto works, or a
 * language is picked). Shared by the General page's line under "Languages I
 * speak" and each Multi-STT slot, so both say the same thing.
 */
export function languageStatusText(
  status: LanguageStatus | null,
  t: TFunction,
  label: (code: string) => string,
): string | null {
  if (!status) return null;
  switch (status.kind) {
    case "detector_missing":
      return t("settings.general.spokenLanguages.status.detectorMissing", {
        language: label(status.fallback),
      });
    case "auto_unavailable": {
      const language = label(status.language);
      const reason = status.reason;
      if (reason.kind === "undetectable")
        return t("settings.general.spokenLanguages.status.undetectable", {
          language,
          languages: reason.languages.map(label).join(", "),
        });
      if (reason.kind === "no_match")
        return t("settings.general.spokenLanguages.status.noMatch", {
          language,
        });
      return t("settings.general.spokenLanguages.status.oneLanguage", {
        language,
      });
    }
    default:
      return null;
  }
}
