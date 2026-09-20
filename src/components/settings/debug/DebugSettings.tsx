import { useTranslation } from "@/i18n/useTranslation";
import { WordCorrectionThreshold } from "./WordCorrectionThreshold";
import { LiveLogViewer } from "./LiveLogViewer";
import { PasteDelay } from "./PasteDelay";
import { HoldThreshold } from "./HoldThreshold";
import { ReliablePasteToggle } from "./ReliablePaste";
import { RecordingBuffer } from "./RecordingBuffer";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { AlwaysOnMicrophone } from "../AlwaysOnMicrophone";
import { SoundPicker } from "../SoundPicker";
import { ClamshellMicrophoneSelector } from "../ClamshellMicrophoneSelector";
import { UpdateChecksToggle } from "../UpdateChecksToggle";
import { WhatsNewPreview } from "./WhatsNewPreview";
import { KeyboardDiagnostic } from "./KeyboardDiagnostic";
import { SessionToastHistory } from "./SessionToastHistory";
import { InterfaceScale } from "./InterfaceScale";
import {
  OnboardingPreview,
  type OnboardingPreviewStep,
} from "./OnboardingPreview";

interface DebugSettingsProps {
  onPreviewOnboarding: (step: OnboardingPreviewStep) => void;
}

export const DebugSettings = ({ onPreviewOnboarding }: DebugSettingsProps) => {
  const { t } = useTranslation();

  return (
    <div class="max-w-3xl w-full mx-auto space-y-6">
      <SessionToastHistory />
      <SettingsGroup title={t("settings.debug.title")}>
        <InterfaceScale descriptionMode="tooltip" grouped={true} />
        <WhatsNewPreview descriptionMode="tooltip" grouped={true} />
        {onPreviewOnboarding && (
          <OnboardingPreview
            onPreview={onPreviewOnboarding}
            descriptionMode="tooltip"
            grouped={true}
          />
        )}
        <UpdateChecksToggle descriptionMode="tooltip" grouped={true} />
        <SoundPicker
          label={t("settings.debug.soundTheme.label")}
          description={t("settings.debug.soundTheme.description")}
        />
        <WordCorrectionThreshold descriptionMode="tooltip" grouped={true} />
        <PasteDelay descriptionMode="tooltip" grouped={true} />
        <PasteDelay
          descriptionMode="tooltip"
          grouped={true}
          settingKey="paste_delay_after_ms"
          labelKey="settings.debug.pasteDelayAfter.title"
          descriptionKey="settings.debug.pasteDelayAfter.description"
        />
        <ReliablePasteToggle descriptionMode="tooltip" grouped={true} />
        <HoldThreshold descriptionMode="tooltip" grouped={true} />
        <RecordingBuffer descriptionMode="tooltip" grouped={true} />
        <AlwaysOnMicrophone descriptionMode="tooltip" grouped={true} />
        <ClamshellMicrophoneSelector descriptionMode="tooltip" grouped={true} />
        <KeyboardDiagnostic />
        <LiveLogViewer descriptionMode="tooltip" grouped={true} />
      </SettingsGroup>
    </div>
  );
};
