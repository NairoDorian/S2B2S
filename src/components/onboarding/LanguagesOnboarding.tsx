import { createEffect, createSignal, For } from "solid-js";
import { commands } from "@/bindings";
import { useTranslation } from "@/i18n/useTranslation";
import { getLanguageLabel } from "@/lib/constants/languages";
import { suggestedSpokenLanguages } from "@/lib/languageSuggestions";
import { Button } from "../ui/Button";
import BrandLockup from "../icons/BrandLockup";
import { useSettings } from "../../hooks/useSettings";

interface LanguagesOnboardingProps {
  onComplete: () => void;
  /** Debug preview: shows the step, writes nothing. */
  preview?: boolean;
}

/**
 * The first-run "languages I speak" step (upstream `0e5e9ca`'s languages
 * step, kept to its point): the OS's preferred languages, as the picker
 * spells them, start ticked. Continue saves the ticked ones as
 * `spoken_languages`; Skip saves nothing. Nothing is ever filled in without
 * this confirmation, because the list becomes the engine's language
 * allowlist.
 */
const LanguagesOnboarding = (props: LanguagesOnboardingProps) => {
  const { t } = useTranslation();
  const { updateSetting } = useSettings();
  const [suggested, setSuggested] = createSignal<string[]>([]);
  const [chosen, setChosen] = createSignal<string[]>([]);
  const [saving, setSaving] = createSignal(false);

  createEffect(
    () => undefined,
    () => {
      let cancelled = false;
      void commands.getSystemLanguages().then((result) => {
        if (cancelled) return;
        const tags = result.status === "ok" ? result.data : [];
        const languages = suggestedSpokenLanguages([
          ...tags,
          navigator.language,
        ]);
        setSuggested(languages);
        setChosen(languages);
      });
      return () => {
        cancelled = true;
      };
    },
  );

  // Ticking keeps the suggestion's order, which is the preference order.
  const toggle = (code: string) =>
    setChosen((list) =>
      list.includes(code)
        ? list.filter((c) => c !== code)
        : suggested().filter((c) => c === code || list.includes(c)),
    );

  const finish = async (save: boolean) => {
    if (props.preview) return;
    if (save && chosen().length > 0) {
      setSaving(true);
      try {
        await updateSetting("spoken_languages", chosen());
      } finally {
        setSaving(false);
      }
    }
    props.onComplete();
  };

  return (
    <div class="h-screen w-full flex flex-col p-6 gap-6 items-center justify-center">
      <BrandLockup size={44} />
      <div class="max-w-md text-center space-y-2">
        <h1 class="text-lg font-semibold text-text">
          {t("onboarding.languages.title")}
        </h1>
        <p class="text-sm text-text/60">
          {t("onboarding.languages.description")}
        </p>
      </div>
      <fieldset class="flex flex-wrap justify-center gap-2 max-w-md border-0 p-0 m-0">
        <legend class="sr-only">{t("onboarding.languages.title")}</legend>
        <For each={suggested()}>
          {(code) => (
            <button
              type="button"
              aria-pressed={chosen().includes(code) ? "true" : "false"}
              onClick={() => toggle(code)}
              class={`px-3 py-1.5 rounded-md border text-sm transition-colors ${
                chosen().includes(code)
                  ? "border-accent bg-accent/15 text-text"
                  : "border-mid-gray/30 text-text/50 hover:border-mid-gray/60"
              }`}
            >
              {getLanguageLabel(code) ?? code}
            </button>
          )}
        </For>
      </fieldset>
      <p class="text-xs text-text/45 max-w-md text-center">
        {t("onboarding.languages.later")}
      </p>
      <div class="flex gap-2">
        <Button variant="secondary" onClick={() => void finish(false)}>
          {t("onboarding.languages.skip")}
        </Button>
        <Button
          onClick={() => void finish(true)}
          disabled={saving() || chosen().length === 0}
        >
          {t("onboarding.languages.continue")}
        </Button>
      </div>
    </div>
  );
};

export default LanguagesOnboarding;
