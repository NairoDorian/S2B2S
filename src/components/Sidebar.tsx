import {
  createSignal,
  createEffect,
  onCleanup,
  For,
  Component,
  ValidComponent,
} from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import {
  BarChart3,
  Cog,
  FlaskConical,
  History,
  Info,
  CircleHelp,
  BrainCircuit,
  Sparkles,
  Cpu,
  Mic,
  FileAudio,
  Radio,
  AudioLines,
  PictureInPicture2,
  PanelLeftClose,
  PanelLeftOpen,
} from "@/components/icons/lucide";
import BrandLockup from "./icons/BrandLockup";
import BrandMark from "./icons/BrandMark";
import { readPref, writePref } from "@/lib/appIdentity";
import { useSettings } from "../hooks/useSettings";
import {
  GeneralSettings,
  AdvancedSettings,
  HistorySettings,
  StatisticsSettings,
  DebugSettings,
  AboutSettings,
  PostProcessingSettings,
  ModelsSettings,
  MultiSttSettings,
  FileTranscriptionSettings,
  LiveModeSettings,
  LiveFftSettings,
  OverlaySettings,
  HelpSettings,
  LlamaSettings,
} from "./settings";

export type SidebarSection = keyof typeof SECTIONS_CONFIG;

interface SectionConfig {
  labelKey: string;
  icon: Component<any>;
  component: ValidComponent;
  enabled: (settings: unknown) => boolean;
}

interface SidebarProps {
  activeSection: SidebarSection;
  onSectionChange: (section: SidebarSection) => void;
}

const DEFAULT_WIDTH = 208;
const MIN_WIDTH = 160;
const MAX_WIDTH = 360;
const COLLAPSED_WIDTH = 56;
const WIDTH_PREF = "sidebar.width";
const COLLAPSED_PREF = "sidebar.collapsed";

export const SECTIONS_CONFIG = {
  general: {
    labelKey: "sidebar.general",
    icon: BrandMark,
    component: GeneralSettings,
    enabled: () => true,
  },
  history: {
    labelKey: "sidebar.history",
    icon: History,
    component: HistorySettings,
    enabled: () => true,
  },
  statistics: {
    labelKey: "sidebar.statistics",
    icon: BarChart3,
    component: StatisticsSettings,
    enabled: () => true,
  },
  models: {
    labelKey: "sidebar.models",
    icon: Cpu,
    component: ModelsSettings,
    enabled: () => true,
  },
  multiStt: {
    labelKey: "sidebar.multiStt",
    icon: Mic,
    component: MultiSttSettings,
    enabled: () => true,
  },
  fileTranscription: {
    labelKey: "sidebar.fileTranscription",
    icon: FileAudio,
    component: FileTranscriptionSettings,
    enabled: () => true,
  },
  liveMode: {
    labelKey: "sidebar.liveMode",
    icon: Radio,
    component: LiveModeSettings,
    enabled: () => true,
  },
  liveFft: {
    labelKey: "sidebar.liveFft",
    icon: AudioLines,
    component: LiveFftSettings,
    enabled: () => true,
  },
  overlay: {
    labelKey: "sidebar.overlay",
    icon: PictureInPicture2,
    component: OverlaySettings,
    enabled: () => true,
  },
  llama: {
    labelKey: "sidebar.llama",
    icon: BrainCircuit,
    component: LlamaSettings,
    enabled: () => true,
  },
  advanced: {
    labelKey: "sidebar.advanced",
    icon: Cog,
    component: AdvancedSettings,
    enabled: () => true,
  },
  postprocessing: {
    labelKey: "sidebar.postProcessing",
    icon: Sparkles,
    component: PostProcessingSettings,
    enabled: (settings: unknown) =>
      (settings as { post_process_enabled?: boolean })?.post_process_enabled ??
      false,
  },
  debug: {
    labelKey: "sidebar.debug",
    icon: FlaskConical,
    component: DebugSettings,
    enabled: () => true,
  },
  help: {
    labelKey: "sidebar.help",
    icon: CircleHelp,
    component: HelpSettings,
    enabled: () => true,
  },
  about: {
    labelKey: "sidebar.about",
    icon: Info,
    component: AboutSettings,
    enabled: () => true,
  },
} as const satisfies Record<string, SectionConfig>;

export function Sidebar(props: SidebarProps) {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const [collapsed, setCollapsed] = createSignal(
    () => readPref(COLLAPSED_PREF) === "true",
  );
  const [width, setWidth] = createSignal(() => {
    const saved = Number(readPref(WIDTH_PREF));
    return Number.isFinite(saved) && saved >= MIN_WIDTH
      ? Math.min(MAX_WIDTH, saved)
      : DEFAULT_WIDTH;
  });
  const [resizing, setResizing] = createSignal(false);
  let dragStartX = 0;
  let dragStartWidth = DEFAULT_WIDTH;

  // A function, not a value: `settings` is an accessor now, and a section's
  // visibility (Post Process is gated on `post_process_enabled`) has to follow
  // it. Reading `settings()` inside keeps this reactive at the `For` call site.
  const availableSections = () =>
    Object.entries(SECTIONS_CONFIG)
      .filter(([, config]) => config.enabled(settings()))
      .map(([id, config]) => ({ id: id as SidebarSection, ...config }));

  const toggleCollapsed = () => {
    setCollapsed((prev) => {
      writePref(COLLAPSED_PREF, String(!prev));
      return !prev;
    });
  };

  const onResizeStart = (e: MouseEvent) => {
    if (collapsed()) return;
    setResizing(true);
    dragStartX = e.clientX;
    dragStartWidth = width();
    e.preventDefault();
  };

  createEffect(
    () => undefined,
    () => {
      if (!resizing()) return;
      const onMove = (e: MouseEvent) => {
        const next = dragStartWidth + (e.clientX - dragStartX);
        setWidth(Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, next)));
      };
      const onUp = () => {
        setResizing(false);
        setWidth((w) => {
          writePref(WIDTH_PREF, String(w));
          return w;
        });
      };
      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
      onCleanup(() => {
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
      });
    },
  );

  const effectiveWidth = () => (collapsed() ? COLLAPSED_WIDTH : width());
  const collapseLabel = () =>
    collapsed() ? t("sidebar.expand") : t("sidebar.collapse");

  return (
    <div
      class="relative flex flex-col h-full shrink-0 border-e border-mid-gray/20 select-none"
      style={{
        width: `${effectiveWidth()}px`,
        transition: resizing() ? "none" : "width 150ms ease-out",
      }}
    >
      <div class="flex items-center justify-center shrink-0 px-2 h-16 border-b border-mid-gray/20">
        {collapsed() ? (
          <BrandMark size={28} />
        ) : (
          <BrandLockup size={26} maxWidth={Math.max(60, width() - 40)} />
        )}
      </div>

      <nav class="flex-1 min-h-0 overflow-y-auto overflow-x-hidden flex flex-col gap-1 py-2 px-2">
        <For each={availableSections()}>
          {(section) => {
            const Icon = section.icon;
            const isActive = () => props.activeSection === section.id;
            return (
              <button
                type="button"
                aria-current={isActive() ? "page" : undefined}
                class={`flex gap-2 items-center p-2 w-full border-s-2 cursor-pointer transition-colors text-start ${
                  collapsed() ? "justify-center" : ""
                } ${
                  isActive()
                    ? "border-accent bg-accent/10 text-accent"
                    : "border-transparent hover:bg-mid-gray/15 hover:opacity-100 opacity-80"
                }`}
                onClick={() => props.onSectionChange(section.id)}
              >
                <Icon width={24} height={24} class="shrink-0" />
                {!collapsed() && (
                  <span class="text-sm font-medium truncate">
                    {t(section.labelKey)}
                  </span>
                )}
              </button>
            );
          }}
        </For>
      </nav>

      <button
        type="button"
        onClick={toggleCollapsed}
        title={collapseLabel()}
        aria-label={collapseLabel()}
        class="shrink-0 flex items-center justify-center gap-2 h-9 border-t border-mid-gray/20 text-text/50 hover:text-text hover:bg-mid-gray/15 cursor-pointer"
      >
        {collapsed() ? (
          <PanelLeftOpen class="w-4 h-4" />
        ) : (
          <>
            <PanelLeftClose class="w-4 h-4" />
            <span class="text-xs">{t("sidebar.collapse")}</span>
          </>
        )}
      </button>

      {!collapsed() && (
        <div
          role="separator"
          aria-orientation="vertical"
          aria-label={t("sidebar.resize")}
          onMouseDown={onResizeStart}
          class={`absolute top-0 -end-0.5 w-1.5 h-full cursor-ew-resize hover:bg-accent/40 transition-colors ${
            resizing() ? "bg-accent/60" : ""
          }`}
        />
      )}
    </div>
  );
}
