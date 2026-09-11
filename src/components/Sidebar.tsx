import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
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
  PanelLeftClose,
  PanelLeftOpen,
} from "lucide-react";
import HandyTextLogo from "./icons/HandyTextLogo";
import HandyHand from "./icons/HandyHand";
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
  HelpSettings,
  LlamaSettings,
} from "./settings";

export type SidebarSection = keyof typeof SECTIONS_CONFIG;

interface IconProps {
  width?: number | string;
  height?: number | string;
  size?: number | string;
  className?: string;
  [key: string]: any;
}

interface SectionConfig {
  labelKey: string;
  icon: React.ComponentType<IconProps>;
  component: React.ComponentType;
  enabled: (settings: any) => boolean;
}

export const SECTIONS_CONFIG = {
  general: {
    labelKey: "sidebar.general",
    icon: HandyHand,
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
    enabled: (settings) => settings?.post_process_enabled ?? false,
  },
  debug: {
    labelKey: "sidebar.debug",
    icon: FlaskConical,
    component: DebugSettings,
    // Always listed: the diagnostics (toast history, logs, live VAD/keyboard
    // checks) are useful outside debug mode too.
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

interface SidebarProps {
  activeSection: SidebarSection;
  onSectionChange: (section: SidebarSection) => void;
}

const DEFAULT_WIDTH = 208;
const MIN_WIDTH = 160;
const MAX_WIDTH = 360;
/** Icon-only width: 24 px icon + padding + the active edge. */
const COLLAPSED_WIDTH = 56;
const WIDTH_KEY = "handy.sidebar.width";
const COLLAPSED_KEY = "handy.sidebar.collapsed";

const readStorage = (key: string): string | null => {
  try {
    return window.localStorage.getItem(key);
  } catch {
    return null;
  }
};
const writeStorage = (key: string, value: string) => {
  try {
    window.localStorage.setItem(key, value);
  } catch {
    // UI preference only.
  }
};

/**
 * Left navigation. Resizable by dragging its right edge (160–360 px),
 * collapsible to an icon rail, and the entry list scrolls when the window
 * is shorter than the list. Width and collapsed state persist per machine.
 */
export const Sidebar: React.FC<SidebarProps> = ({
  activeSection,
  onSectionChange,
}) => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const [collapsed, setCollapsed] = useState(
    () => readStorage(COLLAPSED_KEY) === "true",
  );
  const [width, setWidth] = useState(() => {
    const saved = Number(readStorage(WIDTH_KEY));
    return Number.isFinite(saved) && saved >= MIN_WIDTH
      ? Math.min(MAX_WIDTH, saved)
      : DEFAULT_WIDTH;
  });
  const [resizing, setResizing] = useState(false);
  const dragStartX = useRef(0);
  const dragStartWidth = useRef(DEFAULT_WIDTH);

  const availableSections = Object.entries(SECTIONS_CONFIG)
    .filter(([, config]) => config.enabled(settings))
    .map(([id, config]) => ({ id: id as SidebarSection, ...config }));

  const toggleCollapsed = useCallback(() => {
    setCollapsed((prev) => {
      writeStorage(COLLAPSED_KEY, String(!prev));
      return !prev;
    });
  }, []);

  const onResizeStart = useCallback(
    (e: React.MouseEvent) => {
      if (collapsed) return;
      setResizing(true);
      dragStartX.current = e.clientX;
      dragStartWidth.current = width;
      e.preventDefault();
    },
    [collapsed, width],
  );

  useEffect(() => {
    if (!resizing) return;
    const onMove = (e: MouseEvent) => {
      const next = dragStartWidth.current + (e.clientX - dragStartX.current);
      setWidth(Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, next)));
    };
    const onUp = () => {
      setResizing(false);
      setWidth((w) => {
        writeStorage(WIDTH_KEY, String(w));
        return w;
      });
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, [resizing]);

  const effectiveWidth = collapsed ? COLLAPSED_WIDTH : width;
  const collapseLabel = collapsed ? t("sidebar.expand") : t("sidebar.collapse");

  return (
    <div
      className="relative flex flex-col h-full shrink-0 border-e border-mid-gray/20 select-none"
      style={{
        width: effectiveWidth,
        transition: resizing ? "none" : "width 150ms ease-out",
      }}
    >
      <div className="flex items-center justify-center shrink-0 px-2 h-16 border-b border-mid-gray/20">
        {collapsed ? (
          <HandyHand width={28} height={28} />
        ) : (
          <HandyTextLogo width={Math.min(120, width - 40)} />
        )}
      </div>

      {/* Scrolls when the window is shorter than the list. */}
      <nav className="flex-1 min-h-0 overflow-y-auto overflow-x-hidden flex flex-col gap-1 py-2 px-2">
        {availableSections.map((section) => {
          const Icon = section.icon;
          const isActive = activeSection === section.id;
          const label = t(section.labelKey);
          return (
            <button
              type="button"
              key={section.id}
              title={label}
              aria-current={isActive ? "page" : undefined}
              className={`flex gap-2 items-center p-2 w-full border-s-2 cursor-pointer transition-colors text-start ${
                collapsed ? "justify-center" : ""
              } ${
                isActive
                  ? "border-logo-primary bg-logo-primary/10 text-logo-primary"
                  : "border-transparent hover:bg-mid-gray/15 hover:opacity-100 opacity-80"
              }`}
              onClick={() => onSectionChange(section.id)}
            >
              <Icon width={24} height={24} className="shrink-0" />
              {!collapsed && (
                <span className="text-sm font-medium truncate">{label}</span>
              )}
            </button>
          );
        })}
      </nav>

      <button
        type="button"
        onClick={toggleCollapsed}
        title={collapseLabel}
        aria-label={collapseLabel}
        className="shrink-0 flex items-center justify-center gap-2 h-9 border-t border-mid-gray/20 text-text/50 hover:text-text hover:bg-mid-gray/15 cursor-pointer"
      >
        {collapsed ? (
          <PanelLeftOpen className="w-4 h-4" />
        ) : (
          <>
            <PanelLeftClose className="w-4 h-4" />
            <span className="text-xs">{t("sidebar.collapse")}</span>
          </>
        )}
      </button>

      {/* Resize handle on the right edge */}
      {!collapsed && (
        <div
          role="separator"
          aria-orientation="vertical"
          aria-label={t("sidebar.resize")}
          onMouseDown={onResizeStart}
          className={`absolute top-0 -end-0.5 w-1.5 h-full cursor-ew-resize hover:bg-logo-primary/40 transition-colors ${
            resizing ? "bg-logo-primary/60" : ""
          }`}
        />
      )}
    </div>
  );
};
