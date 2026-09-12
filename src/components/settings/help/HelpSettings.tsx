import React, { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
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
} from "lucide-react";
import { Button } from "../../ui/Button";
import { useNavigationStore } from "../../../stores/navigationStore";
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
export const HelpSettings: React.FC = () => {
  const { t } = useTranslation();
  const setSection = useNavigationStore((state) => state.setSection);
  const pendingHelpAnchor = useNavigationStore(
    (state) => state.pendingHelpAnchor,
  );
  const consumePendingHelpAnchor = useNavigationStore(
    (state) => state.consumePendingHelpAnchor,
  );
  const [query, setQuery] = useState("");
  const [copyStatus, setCopyStatus] = useState<"idle" | "copied" | "error">(
    "idle",
  );

  const scrollToAnchor = useCallback((anchor: string) => {
    const heading = document.getElementById(anchor);
    if (heading) scrollAndFocusAnchor(heading);
  }, []);

  // A QuickHelp banner or the hotkey sidebar opened Help at a section.
  useEffect(() => {
    if (!pendingHelpAnchor) return;
    const frame = window.requestAnimationFrame(() => {
      scrollToAnchor(pendingHelpAnchor);
      consumePendingHelpAnchor();
    });
    return () => window.cancelAnimationFrame(frame);
  }, [consumePendingHelpAnchor, pendingHelpAnchor, scrollToAnchor]);

  const normalizedQuery = query.trim().toLocaleLowerCase();
  const matches = useMemo(() => {
    if (!normalizedQuery) return [];
    return ALL_ENTRIES.filter((entry) => {
      const haystack =
        `${t(entry.titleKey)} ${t(entry.summaryKey)} ${t(entry.destinationLabelKey)}`.toLocaleLowerCase();
      return haystack.includes(normalizedQuery);
    });
  }, [normalizedQuery, t]);

  const aiPrompt = t("help.aiAssist.prompt", { url: SOURCE_URL });
  const copyAiPrompt = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(aiPrompt);
      setCopyStatus("copied");
    } catch {
      setCopyStatus("error");
    }
    window.setTimeout(() => setCopyStatus("idle"), 2000);
  }, [aiPrompt]);

  const renderEntry = (
    entry: HelpEntryDefinition,
    level: 2 | 3,
    index: string,
  ) => {
    const Heading = level === 2 ? "h2" : "h3";
    return (
      <section
        key={entry.id}
        className={`scroll-mt-4 rounded-lg border border-mid-gray/20 bg-background p-4 ${
          level === 3 ? "ms-4" : ""
        }`}
      >
        <Heading
          id={entry.anchor}
          tabIndex={-1}
          className={`settings-anchor font-semibold text-text ${
            level === 2 ? "text-base" : "text-sm"
          }`}
        >
          {`${index} ${t(entry.titleKey)}`}
        </Heading>
        <p className="mt-2 text-sm leading-relaxed text-text/80 whitespace-pre-line">
          {t(entry.summaryKey)}
        </p>
        {entry.warningKey && (
          <div className="mt-3 flex gap-2 rounded-md border border-warning/30 bg-warning/10 p-3 text-xs text-text/80">
            <AlertTriangle className="w-4 h-4 shrink-0 text-warning" />
            <span>{t(entry.warningKey)}</span>
          </div>
        )}
        <div className="mt-3">
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
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <header className="space-y-2">
        <h1 className="text-xl font-semibold text-text">{t("help.title")}</h1>
        <p className="text-sm text-text/70">{t("help.intro")}</p>
      </header>

      {/* Search */}
      <div className="relative">
        <Search className="pointer-events-none absolute start-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text/40" />
        <input
          type="search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={t("help.search.placeholder")}
          aria-label={t("help.search.placeholder")}
          className="w-full rounded-lg border border-mid-gray/20 bg-background ps-9 pe-3 py-2 text-sm text-text placeholder:text-text/40 focus:outline-none focus:ring-2 focus:ring-accent/50"
        />
        {normalizedQuery && (
          <div className="mt-2 rounded-lg border border-mid-gray/20 bg-background divide-y divide-mid-gray/20">
            {matches.length === 0 ? (
              <p className="px-3 py-2 text-sm text-text/60">
                {t("help.search.noResults")}
              </p>
            ) : (
              matches.map((entry) => (
                <button
                  key={entry.anchor}
                  type="button"
                  onClick={() => {
                    setQuery("");
                    scrollToAnchor(entry.anchor);
                  }}
                  className="w-full text-start px-3 py-2 text-sm hover:bg-accent/10 cursor-pointer"
                >
                  <span className="font-medium text-text">
                    {t(entry.titleKey)}
                  </span>
                  <span className="block text-xs text-text/60 truncate">
                    {t(entry.summaryKey)}
                  </span>
                </button>
              ))
            )}
          </div>
        )}
      </div>

      {/* Smart help */}
      <section className="space-y-2">
        <h2 className="text-xs font-medium text-mid-gray uppercase tracking-wide px-1">
          {t("help.smartHelp.title")}
        </h2>
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
          {SMART_HELP_ACTIONS.map((action) => {
            const Icon = SMART_HELP_ICONS[action.icon];
            return (
              <button
                key={action.id}
                type="button"
                onClick={() => scrollToAnchor(action.anchor)}
                className="flex items-start gap-3 rounded-lg border border-mid-gray/20 bg-background p-3 text-start hover:border-accent hover:bg-accent/5 transition-colors cursor-pointer"
              >
                <Icon className="w-5 h-5 shrink-0 text-accent mt-0.5" />
                <span>
                  <span className="block text-sm font-medium text-text">
                    {t(`help.smartHelp.actions.${action.id}.label`)}
                  </span>
                  <span className="block text-xs text-text/60">
                    {t(`help.smartHelp.actions.${action.id}.description`)}
                  </span>
                </span>
              </button>
            );
          })}
        </div>
      </section>

      {/* Contents */}
      <nav aria-label={t("help.contents")} className="space-y-1">
        <h2 className="text-xs font-medium text-mid-gray uppercase tracking-wide px-1">
          {t("help.contents")}
        </h2>
        <ol className="rounded-lg border border-mid-gray/20 bg-background p-3 text-sm space-y-1">
          {HELP_SECTIONS.map((section, i) => (
            <li key={section.id}>
              <button
                type="button"
                onClick={() => scrollToAnchor(section.anchor)}
                className="text-text hover:text-accent cursor-pointer"
              >
                {`${i + 1}. ${t(section.titleKey)}`}
              </button>
              {section.subsections && (
                <ol className="ms-5 mt-1 space-y-1 text-text/70">
                  {section.subsections.map((sub, j) => (
                    <li key={sub.id}>
                      <button
                        type="button"
                        onClick={() => scrollToAnchor(sub.anchor)}
                        className="hover:text-accent cursor-pointer"
                      >
                        {`${i + 1}${String.fromCharCode(97 + j)}. ${t(sub.titleKey)}`}
                      </button>
                    </li>
                  ))}
                </ol>
              )}
            </li>
          ))}
        </ol>
      </nav>

      {/* Sections */}
      <div className="space-y-3">
        {HELP_SECTIONS.map((section, i) => (
          <React.Fragment key={section.id}>
            {renderEntry(section, 2, `${i + 1}.`)}
            {section.subsections?.map((sub, j) =>
              renderEntry(sub, 3, `${i + 1}${String.fromCharCode(97 + j)}.`),
            )}
          </React.Fragment>
        ))}
      </div>

      {/* Ask an AI assistant */}
      <section className="rounded-lg border border-mid-gray/20 bg-background p-4 space-y-2">
        <h2 className="text-sm font-semibold text-text">
          {t("help.aiAssist.title")}
        </h2>
        <p className="text-xs text-text/70">{t("help.aiAssist.description")}</p>
        <pre className="whitespace-pre-wrap break-words rounded-md bg-mid-gray/10 p-3 text-xs text-text/80 select-text">
          {aiPrompt}
        </pre>
        <Button variant="secondary" size="sm" onClick={copyAiPrompt}>
          <span className="inline-flex items-center gap-1.5">
            {copyStatus === "copied" ? (
              <Check className="w-3.5 h-3.5" />
            ) : (
              <Copy className="w-3.5 h-3.5" />
            )}
            {copyStatus === "copied"
              ? t("help.aiAssist.copied")
              : copyStatus === "error"
                ? t("help.aiAssist.copyFailed")
                : t("help.aiAssist.copy")}
          </span>
        </Button>
      </section>

      <p className="text-xs text-text/50 px-1">
        <button
          type="button"
          onClick={() => setSection("about")}
          className="underline underline-offset-2 hover:text-text cursor-pointer"
        >
          {t("help.aboutLink")}
        </button>
      </p>
    </div>
  );
};
