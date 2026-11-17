import { useTranslation } from "@/i18n/useTranslation";
import { type } from "@tauri-apps/plugin-os";
import { MicrophoneSelector } from "../MicrophoneSelector";
import { ChannelSelector } from "../ChannelSelector";
import { ShortcutInput } from "../ShortcutInput";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { OutputDeviceSelector } from "../OutputDeviceSelector";
import { ShortcutActivationSetting } from "../ShortcutActivation";
import { AudioFeedback } from "../AudioFeedback";
import { useSettings } from "../../../hooks/useSettings";
import { VolumeSlider } from "../VolumeSlider";
import { MuteWhileRecording } from "../MuteWhileRecording";
import { ModelSettingsCard } from "./ModelSettingsCard";
import { SpokenLanguages } from "../SpokenLanguages";
import { ChineseScriptSetting } from "../ChineseScript";
import { isChineseLanguage } from "@/lib/constants/languages";

export const GeneralSettings = () => {
  const { t } = useTranslation();
  const { audioFeedbackEnabled, getSetting } = useSettings();
  // The script setting is for people who write Chinese: they speak it, picked
  // it as the language, or already chose a script.
  const writesChinese = () =>
    (getSetting("spoken_languages") ?? []).some(isChineseLanguage) ||
    isChineseLanguage(getSetting("selected_language") ?? "") ||
    (getSetting("chinese_script") ?? "as_transcribed") !== "as_transcribed";
  const isLinux = type() === "linux";
  return (
    <div class="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title={t("settings.general.title")}>
        <ShortcutInput shortcutId="transcribe" grouped={true} />
        <ShortcutActivationSetting descriptionMode="tooltip" grouped={true} />
        {!isLinux && <ShortcutInput shortcutId="cancel" grouped={true} />}
      </SettingsGroup>
      <ModelSettingsCard />
      <SettingsGroup title={t("settings.general.spokenLanguages.groupTitle")}>
        <SpokenLanguages descriptionMode="tooltip" grouped={true} />
        {writesChinese() && (
          <ChineseScriptSetting descriptionMode="tooltip" grouped={true} />
        )}
      </SettingsGroup>
      <SettingsGroup title={t("settings.sound.title")}>
        <MicrophoneSelector descriptionMode="tooltip" grouped={true} />
        <ChannelSelector descriptionMode="tooltip" grouped={true} />
        <MuteWhileRecording descriptionMode="tooltip" grouped={true} />
        <AudioFeedback descriptionMode="tooltip" grouped={true} />
        <OutputDeviceSelector
          descriptionMode="tooltip"
          grouped={true}
          disabled={!audioFeedbackEnabled()}
        />
        <VolumeSlider disabled={!audioFeedbackEnabled()} />
      </SettingsGroup>
    </div>
  );
};
