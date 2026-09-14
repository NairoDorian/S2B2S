import { useTranslation } from "@/i18n/useTranslation";
import type { SidebarSection } from "../Sidebar";
import { openHelp } from "../../stores/navigationStore";
import type { JSX } from "@solidjs/web";

interface QuickHelpProps {
  activeSection: SidebarSection;
}

/** One-line orientation per page, with a link into the matching Help section. */
const QUICK_HELP: Partial<
  Record<SidebarSection, { copyKey: string; anchor: string }>
> = {
  general: { copyKey: "quickHelp.general", anchor: "help-transcription" },
  history: { copyKey: "quickHelp.history", anchor: "help-history" },
  recall: { copyKey: "quickHelp.recall", anchor: "help-history" },
  statistics: { copyKey: "quickHelp.statistics", anchor: "help-history" },
  models: { copyKey: "quickHelp.models", anchor: "help-models" },
  multiStt: { copyKey: "quickHelp.multiStt", anchor: "help-multi-stt" },
  fileTranscription: {
    copyKey: "quickHelp.fileTranscription",
    anchor: "help-file-transcription",
  },
  liveMode: { copyKey: "quickHelp.liveMode", anchor: "help-live-mode" },
  liveFft: { copyKey: "quickHelp.liveFft", anchor: "help-live-fft" },
  overlay: { copyKey: "quickHelp.overlay", anchor: "help-overlay" },
  advanced: { copyKey: "quickHelp.advanced", anchor: "help-advanced" },
  llama: { copyKey: "quickHelp.llama", anchor: "help-llama" },
  postprocessing: {
    copyKey: "quickHelp.postprocessing",
    anchor: "help-post-processing",
  },
  debug: { copyKey: "quickHelp.debug", anchor: "help-troubleshooting" },
};

export const QuickHelp = ({ activeSection }: QuickHelpProps): JSX.Element => {
  const { t } = useTranslation();
  const help = QUICK_HELP[activeSection];

  if (!help) return null;

  return (
    <div class="max-w-3xl w-full mx-auto flex flex-wrap items-center justify-between gap-x-3 gap-y-1 rounded-lg border border-mid-gray/20 bg-mid-gray/5 px-3 py-2">
      <p class="min-w-0 flex-1 text-xs leading-relaxed text-text/70">
        {t(help.copyKey)}
      </p>
      <a
        href={`#${help.anchor}`}
        onClick={(event) => {
          event.preventDefault();
          openHelp(help.anchor);
        }}
        class="shrink-0 rounded-md px-1 text-xs font-medium text-accent underline decoration-accent/50 underline-offset-2 transition-colors hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
      >
        {t("quickHelp.learnMore")}
      </a>
    </div>
  );
};
