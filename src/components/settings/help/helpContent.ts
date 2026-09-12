import { REPO_URL } from "@/lib/appIdentity";
import type { SidebarSection } from "../../Sidebar";

/**
 * The Help page is structured data: every section has an anchor (linked
 * from QuickHelp banners and Smart Help cards), i18n keys for its title and
 * summary, and the settings page it sends the reader to. Copy lives under
 * `help.sections.*` in the locale files.
 */
export interface HelpEntryDefinition {
  id: string;
  anchor: string;
  titleKey: string;
  summaryKey: string;
  /** Optional caution rendered in an amber block under the summary. */
  warningKey?: string;
  destination: SidebarSection;
  destinationLabelKey: string;
}

export interface HelpSectionDefinition extends HelpEntryDefinition {
  subsections?: readonly HelpEntryDefinition[];
}

const entry = (
  id: string,
  anchor: string,
  key: string,
  destination: SidebarSection,
  destinationLabelKey: string,
  warning = false,
): HelpEntryDefinition => ({
  id,
  anchor,
  titleKey: `help.sections.${key}.title`,
  summaryKey: `help.sections.${key}.summary`,
  warningKey: warning ? `help.sections.${key}.warning` : undefined,
  destination,
  destinationLabelKey,
});

export const HELP_SECTIONS: readonly HelpSectionDefinition[] = [
  {
    ...entry(
      "transcription",
      "help-transcription",
      "transcription",
      "general",
      "sidebar.general",
    ),
    subsections: [
      entry("models", "help-models", "models", "models", "sidebar.models"),
      entry(
        "advanced",
        "help-advanced",
        "advanced",
        "advanced",
        "sidebar.advanced",
      ),
      entry("vad", "help-vad", "vad", "advanced", "sidebar.advanced"),
    ],
  },
  entry(
    "postProcessing",
    "help-post-processing",
    "postProcessing",
    "postprocessing",
    "sidebar.postProcessing",
    true,
  ),
  entry("llama", "help-llama", "llama", "llama", "sidebar.llama"),
  entry(
    "multiStt",
    "help-multi-stt",
    "multiStt",
    "multiStt",
    "sidebar.multiStt",
  ),
  entry(
    "fileTranscription",
    "help-file-transcription",
    "fileTranscription",
    "fileTranscription",
    "sidebar.fileTranscription",
  ),
  entry(
    "liveMode",
    "help-live-mode",
    "liveMode",
    "liveMode",
    "sidebar.liveMode",
  ),
  entry("liveFft", "help-live-fft", "liveFft", "liveFft", "sidebar.liveFft"),
  entry("overlay", "help-overlay", "overlay", "overlay", "sidebar.overlay"),
  entry("history", "help-history", "history", "history", "sidebar.history"),
  entry(
    "troubleshooting",
    "help-troubleshooting",
    "troubleshooting",
    "debug",
    "sidebar.debug",
  ),
] as const;

/** Goal-oriented entry points shown at the top of the page. */
export const SMART_HELP_ACTIONS = [
  { id: "setup", anchor: "help-transcription", icon: "settings" },
  { id: "model", anchor: "help-models", icon: "cpu" },
  { id: "tune", anchor: "help-vad", icon: "mic" },
  { id: "files", anchor: "help-file-transcription", icon: "file" },
  { id: "broken", anchor: "help-troubleshooting", icon: "wrench" },
] as const;

/** Where users can point an AI assistant for source-level answers. */
export const SOURCE_URL = REPO_URL;
