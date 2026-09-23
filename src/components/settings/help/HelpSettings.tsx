import { createMemo, createEffect, createSignal, For, Show } from "solid-js";
import { dynamic } from "@solidjs/web";
import { useTranslation } from "@/i18n/useTranslation";
import {
  AlertTriangle,
  Check,
  Copy,
  Cpu,
  FileAudio,
  Mic,
  Search,
  Settings2,
  Wrench,
} from "@/components/icons/lucide";
import { Button } from "../../ui/Button";
import {
  useNavigationStore,
  setSection,
  consumePendingHelpAnchor,
} from "../../../stores/navigationStore";
import { scrollAndFocusAnchor } from "../../../lib/anchorNavigation";
import {
  HELP_SECTIONS,
  SMART_HELP_ACTIONS,
  SOURCE_URL,
  type HelpEntryDefinition,
} from "./helpContent";

const SMART_HELP_ICONS = {
  settings: Settings2,
  cpu: Cpu,
  mic: Mic,
  file: FileAudio,
  wrench: Wrench,
} as const;

/** Every section and subsection, flattened, in reading order. */
const ALL_ENTRIES: readonly HelpEntryDefinition[] = HELP_SECTIONS.flatMap(
  (section) => [section, ...(section.subsections ?? [])],
);

/**
 * In-app Help: goal cards, a search box, a table of contents and one block
 * per feature, each with a button into the page that configures it. Adapted
 * from the AIVORelay fork's Help page; the copy is this fork's. Search runs
 * over the translated title and summary, so it follows the app language.
 */
const pendingHelpAnchor = () => useNavigationStore().pendingHelpAnchor;

const scrollToAnchor = (anchor: string) => {
  const heading = document.getElementById(anchor);
  if (heading) scrollAndFocusAnchor(heading);
};

export const HelpSettings = () => {
  const { t } = useTranslation();
  const [query, setQuery] = createSignal("");
  const [copyStatus, setCopyStatus] = createSignal<"idle" | "copied" | "error">(
    "idle",
  );

  // A QuickHelp banner opened Help at a section. Tracked, so an `openHelp`
  // issued while this page is already showing is honoured too.
  createEffect(
    () => pendingHelpAnchor(),
    (anchor) => {
      if (!anchor) return;
      const frame = window.requestAnimationFrame(() => {
        scrollToAnchor(anchor);
        consumePendingHelpAnchor();
      });
      return () => window.cancelAnimationFrame(frame);
    },
  );

  const normalizedQuery = createMemo(() => query().trim().toLocaleLowerCase());
  const matches = createMemo(() => {
    if (!normalizedQuery()) return [];
    return ALL_ENTRIES.filter((entry) => {
      const haystack =
        `${t(entry.titleKey)} ${t(entry.summaryKey)} ${t(entry.destinationLabelKey)}`.toLocaleLowerCase();
      return haystack.includes(normalizedQuery());
    });
  });

  const aiPrompt = () => t("help.aiAssist.prompt", { url: SOURCE_URL });
  const copyAiPrompt = async () => {
    try {
      await navigator.clipboard.writeText(aiPrompt());
      setCopyStatus("copied");
    } catch {
      setCopyStatus("error");
    }
    window.setTimeout(() => setCopyStatus("idle"), 2000);
  };

  const renderEntry = (
    entry: HelpEntryDefinition,
    level: 2 | 3,
    index: string,
  ) => {
    // A tag name held in a variable is not a component in Solid — `<Heading>`
    // would call `createComponent("h2", …)` and throw. `dynamic()` is the
    // supported way to render an intrinsic element chosen at runtime.
    const Heading = dynamic(() => (level === 2 ? "h2" : "h3"));
    return (
      <section
        class={[
          "scroll-mt-4 rounded-lg border border-mid-gray/20 bg-background p-4",
          { "ms-4": level === 3 },
        ]}
      >
        <Heading
          id={entry.anchor}
          tabindex={-1}
          class={[
            "settings-anchor font-semibold text-text",
            {
              "text-base": level === 2,
              "text-sm": level === 3,
            },
          ]}
        >
          {`${index} ${t(entry.titleKey)}`}
        </Heading>
        <p class="mt-2 text-sm leading-relaxed text-text/80 whitespace-pre-line">
          {t(entry.summaryKey)}
        </p>
        <Show when={entry.warningKey}>
          {(warningKey) => (
            <div class="mt-3 flex gap-2 rounded-md border border-warning/30 bg-warning/10 p-3 text-xs text-text/80">
              <AlertTriangle class="w-4 h-4 shrink-0 text-warning" />
              <span>{t(warningKey())}</span>
            </div>
          )}
        </Show>
        <div class="mt-3">
          <Button
            variant="secondary"
            size="sm"
            onClick={() => setSection(entry.destination)}
          >
            {t("help.openPage", { page: t(entry.destinationLabelKey) })}
          </Button>
        </div>
      </section>
    );
  };

  return (
    <div class="max-w-3xl w-full mx-auto space-y-6">
      <header class="space-y-2">
        <h1 class="text-xl font-semibold text-text">{t("help.title")}</h1>
        <p class="text-sm text-text/70">{t("help.intro")}</p>
      </header>

      {/* Search */}
      <div class="relative">
        <Search class="pointer-events-none absolute start-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text/40" />
        <input
          type="search"
          value={query()}
          onInput={(e) => setQuery(e.currentTarget.value)}
          placeholder={t("help.search.placeholder")}
          aria-label={t("help.search.placeholder")}
          class="w-full rounded-lg border border-mid-gray/20 bg-background ps-9 pe-3 py-2 text-sm text-text placeholder:text-text/40 focus:outline-none focus:ring-2 focus:ring-accent/50"
        />
        <Show when={normalizedQuery()}>
          <div class="mt-2 rounded-lg border border-mid-gray/20 bg-background divide-y divide-mid-gray/20">
            <Show when={matches().length === 0}>
              <p class="px-3 py-2 text-sm text-text/60">
                {t("help.search.noResults")}
              </p>
            </Show>
            <Show when={matches().length > 0}>
              <For each={matches()}>
                {(entry) => (
                  <button
                    type="button"
                    onClick={() => {
                      setQuery("");
                      scrollToAnchor(entry.anchor);
                    }}
                    class="w-full text-start px-3 py-2 text-sm hover:bg-accent/10 cursor-pointer"
                  >
                    <span class="font-medium text-text">
                      {t(entry.titleKey)}
                    </span>
                    <span class="block text-xs text-text/60 truncate">
                      {t(entry.summaryKey)}
                    </span>
                  </button>
                )}
              </For>
            </Show>
          </div>
        </Show>
      </div>

      {/* Smart help */}
      <section class="space-y-2">
        <h2 class="text-xs font-medium text-mid-gray uppercase tracking-wide px-1">
          {t("help.smartHelp.title")}
        </h2>
        <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
          <For each={SMART_HELP_ACTIONS}>
            {(action) => {
              const Icon = SMART_HELP_ICONS[action.icon];
              return (
                <button
                  type="button"
                  onClick={() => scrollToAnchor(action.anchor)}
                  class="flex items-start gap-3 rounded-lg border border-mid-gray/20 bg-background p-3 text-start hover:border-accent hover:bg-accent/5 transition-colors cursor-pointer"
                >
                  <Icon class="w-5 h-5 shrink-0 text-accent mt-0.5" />
                  <span>
                    <span class="block text-sm font-medium text-text">
                      {t(`help.smartHelp.actions.${action.id}.label`)}
                    </span>
                    <span class="block text-xs text-text/60">
                      {t(`help.smartHelp.actions.${action.id}.description`)}
                    </span>
                  </span>
                </button>
              );
            }}
          </For>
        </div>
      </section>

      {/* Contents */}
      <nav aria-label={t("help.contents")} class="space-y-1">
        <h2 class="text-xs font-medium text-mid-gray uppercase tracking-wide px-1">
          {t("help.contents")}
        </h2>
        <ol class="rounded-lg border border-mid-gray/20 bg-background p-3 text-sm space-y-1">
          <For each={HELP_SECTIONS}>
            {(section, i) => (
              <li>
                <button
                  type="button"
                  onClick={() => scrollToAnchor(section.anchor)}
                  class="text-text hover:text-accent cursor-pointer"
                >
                  {`${i() + 1}. ${t(section.titleKey)}`}
                </button>
                <Show when={section.subsections}>
                  <ol class="ms-5 mt-1 space-y-1 text-text/70">
                    <For each={section.subsections}>
                      {(sub, j) => (
                        <li>
                          <button
                            type="button"
                            onClick={() => scrollToAnchor(sub.anchor)}
                            class="hover:text-accent cursor-pointer"
                          >
                            {`${i() + 1}${String.fromCharCode(97 + j())}. ${t(sub.titleKey)}`}
                          </button>
                        </li>
                      )}
                    </For>
                  </ol>
                </Show>
              </li>
            )}
          </For>
        </ol>
      </nav>

      {/* Sections */}
      <div class="space-y-3">
        <For each={HELP_SECTIONS}>
          {(section, i) => (
            <>
              {renderEntry(section, 2, `${i() + 1}.`)}
              <Show when={section.subsections}>
                <For each={section.subsections}>
                  {(sub, j) =>
                    renderEntry(
                      sub,
                      3,
                      `${i() + 1}${String.fromCharCode(97 + j())}.`,
                    )
                  }
                </For>
              </Show>
            </>
          )}
        </For>
      </div>

      {/* Ask an AI assistant */}
      <section class="rounded-lg border border-mid-gray/20 bg-background p-4 space-y-2">
        <h2 class="text-sm font-semibold text-text">
          {t("help.aiAssist.title")}
        </h2>
        <p class="text-xs text-text/70">{t("help.aiAssist.description")}</p>
        <pre class="whitespace-pre-wrap break-words rounded-md bg-mid-gray/10 p-3 text-xs text-text/80 select-text">
          {aiPrompt()}
        </pre>
        <Button variant="secondary" size="sm" onClick={copyAiPrompt}>
          <span class="inline-flex items-center gap-1.5">
            <Show when={copyStatus() === "copied"}>
              <Check class="w-3.5 h-3.5" />
            </Show>
            <Show when={copyStatus() !== "copied"}>
              <Copy class="w-3.5 h-3.5" />
            </Show>
            {copyStatus() === "copied"
              ? t("help.aiAssist.copied")
              : copyStatus() === "error"
                ? t("help.aiAssist.copyFailed")
                : t("help.aiAssist.copy")}
          </span>
        </Button>
      </section>

      <p class="text-xs text-text/50 px-1">
        <button
          type="button"
          onClick={() => setSection("about")}
          class="underline underline-offset-2 hover:text-text cursor-pointer"
        >
          {t("help.aboutLink")}
        </button>
      </p>
    </div>
  );
};
