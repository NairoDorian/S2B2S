import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { Slider } from "../../ui/Slider";
import { Dropdown } from "../../ui/Dropdown";
import { Input } from "../../ui/Input";
import { Button } from "../../ui/Button";
import { useSettings } from "../../../hooks/useSettings";
import { commands } from "@/bindings";
import type { TtsConfig, TtsEngine, Voice } from "@/bindings";

const ENGINES: TtsEngine[] = ["piper", "openai", "elevenlabs", "cartesia"];

export const SpeechSettings: React.FC = () => {
  const { t } = useTranslation();
  const { settings, updateSetting, isUpdating } = useSettings();
  const [voices, setVoices] = useState<Voice[]>([]);
  const [speaking, setSpeaking] = useState(false);

  const tts = settings?.tts;

  const update = useCallback(
    (patch: Partial<TtsConfig>) => {
      if (!tts) return;
      void updateSetting("tts", { ...tts, ...patch });
    },
    [tts, updateSetting],
  );

  const refreshVoices = useCallback(async () => {
    const result = await commands.ttsGetVoices();
    if (result.status === "ok") {
      setVoices(result.data);
    }
  }, []);

  useEffect(() => {
    void refreshVoices();
  }, [refreshVoices, tts?.engine, tts?.piper.data_dir]);

  if (!tts) return null;

  // `sanitization` is optional in the generated bindings (serde default).
  const sanitization = tts.sanitization ?? {
    enabled: true,
    markdown: true,
    tts_normalization: true,
  };

  const speakSample = async () => {
    setSpeaking(true);
    try {
      await commands.ttsSpeak(t("settings.speech.sampleText"));
    } finally {
      setSpeaking(false);
    }
  };

  return (
    <div className="space-y-6">
      <SettingsGroup title={t("settings.speech.outputGroup")}>
        <ToggleSwitch
          checked={tts.enabled}
          onChange={(enabled) => update({ enabled })}
          isUpdating={isUpdating("tts")}
          label={t("settings.speech.enabled.label")}
          description={t("settings.speech.enabled.description")}
          grouped
        />
        <SettingContainer
          title={t("settings.speech.engine.label")}
          description={t("settings.speech.engine.description")}
          grouped
        >
          <Dropdown
            options={ENGINES.map((engine) => ({
              value: engine,
              label: t(`settings.speech.engine.options.${engine}`),
            }))}
            selectedValue={tts.engine}
            onSelect={(value) => update({ engine: value as TtsEngine })}
          />
        </SettingContainer>
        <SettingContainer
          title={t("settings.speech.voice.label")}
          description={t("settings.speech.voice.description")}
          grouped
        >
          <Dropdown
            options={voices.map((voice) => ({
              value: voice.id,
              label: voice.name,
            }))}
            selectedValue={tts.voice || null}
            onSelect={(voice) => update({ voice })}
            placeholder={t("settings.speech.voice.placeholder")}
            onRefresh={() => void refreshVoices()}
          />
        </SettingContainer>
        <Slider
          value={tts.speed ?? 1}
          onChange={(speed) => update({ speed })}
          min={0.5}
          max={2}
          step={0.05}
          label={t("settings.speech.speed.label")}
          description={t("settings.speech.speed.description")}
          grouped
          showValue
          formatValue={(value) => `${value.toFixed(2)}x`}
        />
        <Slider
          value={tts.volume}
          onChange={(volume) => update({ volume: Math.round(volume) })}
          min={0}
          max={100}
          step={1}
          label={t("settings.speech.volume.label")}
          description={t("settings.speech.volume.description")}
          grouped
          showValue
          formatValue={(value) => `${Math.round(value)}%`}
        />
        <ToggleSwitch
          checked={tts.double_copy_enabled ?? false}
          onChange={(double_copy_enabled) => update({ double_copy_enabled })}
          label={t("settings.speech.doubleCopy.label")}
          description={t("settings.speech.doubleCopy.description")}
          grouped
        />
        <ToggleSwitch
          checked={tts.play_startup_greeting ?? true}
          onChange={(play_startup_greeting) => update({ play_startup_greeting })}
          label="Play Startup Greeting Audio"
          description="Speak the warmup sentence aloud when the voice model finishes loading"
          grouped
        />
        <SettingContainer
          title={t("settings.speech.test.label")}
          description={t("settings.speech.test.description")}
          grouped
        >
          <div className="flex gap-2">
            <Button
              variant="primary-soft"
              size="sm"
              disabled={speaking || !tts.enabled}
              onClick={() => void speakSample()}
            >
              {t("settings.speech.test.speak")}
            </Button>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => void commands.ttsStop()}
            >
              {t("settings.speech.test.stop")}
            </Button>
          </div>
        </SettingContainer>
      </SettingsGroup>

      <SettingsGroup title={t("settings.speech.piperGroup")}>
        <ToggleSwitch
          checked={tts.piper.cuda}
          onChange={(cuda) => update({ piper: { ...tts.piper, cuda } })}
          label={t("settings.speech.piperCuda.label")}
          description={t("settings.speech.piperCuda.description")}
          grouped
        />
        <SettingContainer
          title={t("settings.speech.unload.label")}
          description={t("settings.speech.unload.description")}
          grouped
        >
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void commands.ttsUnloadEngine()}
          >
            {t("settings.speech.unload.button")}
          </Button>
        </SettingContainer>
      </SettingsGroup>

      <SettingsGroup title={t("settings.speech.sanitizeGroup")}>
        <ToggleSwitch
          checked={sanitization.enabled}
          onChange={(enabled) =>
            update({ sanitization: { ...sanitization, enabled } })
          }
          label={t("settings.speech.sanitizeEnabled.label")}
          description={t("settings.speech.sanitizeEnabled.description")}
          grouped
        />
        <ToggleSwitch
          checked={sanitization.markdown}
          onChange={(markdown) =>
            update({ sanitization: { ...sanitization, markdown } })
          }
          disabled={!sanitization.enabled}
          label={t("settings.speech.sanitizeMarkdown.label")}
          description={t("settings.speech.sanitizeMarkdown.description")}
          grouped
        />
        <ToggleSwitch
          checked={sanitization.tts_normalization}
          onChange={(tts_normalization) =>
            update({
              sanitization: { ...sanitization, tts_normalization },
            })
          }
          disabled={!sanitization.enabled}
          label={t("settings.speech.sanitizeNormalize.label")}
          description={t("settings.speech.sanitizeNormalize.description")}
          grouped
        />
      </SettingsGroup>
    </div>
  );
};
