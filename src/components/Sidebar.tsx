import React from "react";
import { useTranslation } from "react-i18next";
import {
  Brain,
  Cog,
  FlaskConical,
  History,
  Info,
  MessagesSquare,
  Sparkles,
  Cpu,
  Volume2,
  Terminal,
  Monitor,
  Zap,
} from "lucide-react";
import S2B2SIcon from "./icons/S2B2SIcon";
import appIcon from "../assets/icon.png";
import { useSettings } from "../hooks/useSettings";
import {
  GeneralSettings,
  AdvancedSettings,
  HistorySettings,
  DebugSettings,
  AboutSettings,
  PostProcessingSettings,
  ModelsSettings,
  SpeechSettings,
  BrainSettings,
  LlamaCppSettings,
  OverlayWindowSettings,
  WgpuTrailSettings,
} from "./settings";
import { ConversationView } from "./conversation/ConversationView";

export type SidebarSection = keyof typeof SECTIONS_CONFIG;

interface IconProps {
  width?: number | string;
  height?: number | string;
  size?: number | string;
  className?: string;
  [key: string]: unknown;
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
    icon: S2B2SIcon,
    component: GeneralSettings,
    enabled: () => true,
  },
  history: {
    labelKey: "sidebar.history",
    icon: History,
    component: HistorySettings,
    enabled: () => true,
  },
  models: {
    labelKey: "sidebar.models",
    icon: Cpu,
    component: ModelsSettings,
    enabled: () => true,
  },
  advanced: {
    labelKey: "sidebar.advanced",
    icon: Cog,
    component: AdvancedSettings,
    enabled: () => true,
  },
  conversation: {
    labelKey: "sidebar.conversation",
    icon: MessagesSquare,
    component: ConversationView,
    enabled: () => true,
  },
  speech: {
    labelKey: "sidebar.speech",
    icon: Volume2,
    component: SpeechSettings,
    enabled: () => true,
  },
  brain: {
    labelKey: "sidebar.brain",
    icon: Brain,
    component: BrainSettings,
    enabled: () => true,
  },
  llamaCpp: {
    labelKey: "sidebar.llamaCpp",
    icon: Terminal,
    component: LlamaCppSettings,
    enabled: () => true,
  },
  overlayWindow: {
    labelKey: "sidebar.overlayWindow",
    icon: Monitor,
    component: OverlayWindowSettings,
    enabled: () => true,
  },
  wgpuTrail: {
    labelKey: "sidebar.wgpuTrail",
    icon: Zap,
    component: WgpuTrailSettings,
    // Hidden from the normal UI until the native wgpu overlay renderer exists — the
    // toggle currently persists config the backend can't render. Still reachable in
    // debug mode for development.
    enabled: (settings) => settings?.debug_mode ?? false,
  },
  postprocessing: {
    labelKey: "sidebar.postProcessing",
    icon: Sparkles,
    component: PostProcessingSettings,
    enabled: () => true,
  },
  debug: {
    labelKey: "sidebar.debug",
    icon: FlaskConical,
    component: DebugSettings,
    enabled: (settings) => settings?.debug_mode ?? false,
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

export const Sidebar: React.FC<SidebarProps> = ({
  activeSection,
  onSectionChange,
}) => {
  const { t } = useTranslation();
  const { settings } = useSettings();

  const availableSections = Object.entries(SECTIONS_CONFIG)
    .filter(([_, config]) => config.enabled(settings))
    .map(([id, config]) => ({ id: id as SidebarSection, ...config }));

  return (
    <div className="flex flex-col w-40 h-full border-e border-mid-gray/20 items-center px-2 overflow-hidden">
      <img src={appIcon} alt="S2B2S" className="w-10 h-10 m-4 shrink-0" />
      <div className="flex flex-col w-full items-center gap-1 pt-2 border-t border-mid-gray/20 flex-1 overflow-y-auto">
        {availableSections.map((section) => {
          const Icon = section.icon;
          const isActive = activeSection === section.id;

          return (
            <div
              key={section.id}
              className={`flex gap-2 items-center p-2 w-full rounded-lg cursor-pointer transition-colors ${
                isActive
                  ? "bg-logo-primary/80"
                  : "hover:bg-mid-gray/20 hover:opacity-100 opacity-85"
              }`}
              onClick={() => onSectionChange(section.id)}
            >
              <Icon width={24} height={24} className="shrink-0" />
              <p
                className="text-sm font-medium truncate"
                title={t(section.labelKey)}
              >
                {t(section.labelKey)}
              </p>
            </div>
          );
        })}
      </div>
    </div>
  );
};
