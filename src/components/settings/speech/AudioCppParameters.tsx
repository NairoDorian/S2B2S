import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Dropdown,
  Input,
  SettingContainer,
  SettingsGroup,
  Slider,
  ToggleSwitch,
} from "@/components/ui";
import {
  ChevronDown,
  ChevronRight,
  Sliders,
  Cpu,
  Sparkles,
} from "lucide-react";
import type { TtsConfig } from "@/bindings";
import { commands } from "@/bindings";

const AUDIOCPP_LANGUAGES: Array<{ value: string; label: string }> = [
  { value: "auto", label: "Auto (Detect / Mirror)" },
  { value: "en", label: "English (en)" },
  { value: "es", label: "Español / Spanish (es)" },
  { value: "fr", label: "Français / French (fr)" },
  { value: "de", label: "Deutsch / German (de)" },
  { value: "it", label: "Italiano / Italian (it)" },
  { value: "ja", label: "日本語 / Japanese (ja)" },
  { value: "ko", label: "한국어 / Korean (ko)" },
  { value: "zh", label: "中文 / Chinese (zh)" },
  { value: "ru", label: "Русский / Russian (ru)" },
  { value: "pt", label: "Português / Portuguese (pt)" },
  { value: "nl", label: "Nederlands / Dutch (nl)" },
  { value: "pl", label: "Polski / Polish (pl)" },
  { value: "ar", label: "العربية / Arabic (ar)" },
  { value: "hi", label: "हिन्दी / Hindi (hi)" },
  { value: "tr", label: "Türkçe / Turkish (tr)" },
  { value: "vi", label: "Tiếng Việt / Vietnamese (vi)" },
  { value: "uk", label: "Українська / Ukrainian (uk)" },
  { value: "sv", label: "Svenska / Swedish (sv)" },
  { value: "da", label: "Dansk / Danish (da)" },
  { value: "cs", label: "Čeština / Czech (cs)" },
  { value: "bg", label: "Български / Bulgarian (bg)" },
  { value: "el", label: "Ελληνικά / Greek (el)" },
  { value: "fi", label: "Suomi / Finnish (fi)" },
  { value: "hr", label: "Hrvatski / Croatian (hr)" },
  { value: "hu", label: "Magyar / Hungarian (hu)" },
  { value: "id", label: "Bahasa Indonesia / Indonesian (id)" },
  { value: "ro", label: "Română / Romanian (ro)" },
  { value: "sk", label: "Slovenčina / Slovak (sk)" },
  { value: "sl", label: "Slovenščina / Slovenian (sl)" },
  { value: "lt", label: "Lietuvių / Lithuanian (lt)" },
  { value: "lv", label: "Latviešu / Latvian (lv)" },
  { value: "et", label: "Eesti / Estonian (et)" },
  { value: "no", label: "Norsk / Norwegian (no)" },
  { value: "sw", label: "Kiswahili / Swahili (sw)" },
  { value: "ms", label: "Bahasa Melayu / Malay (ms)" },
];

interface AudioCppParametersProps {
  tts: TtsConfig;
  update: (patch: Partial<TtsConfig>) => void;
}

export const AudioCppParameters: React.FC<AudioCppParametersProps> = ({
  tts,
  update,
}) => {
  const { t } = useTranslation();
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [showHardware, setShowHardware] = useState(false);

  const audiocpp = tts.audiocpp ?? {
    model: "supertonic",
    quantization: "default",
    voice: "default",
    language: "auto",
    backend: "cuda",
    device_id: 0,
    threads: 4,
    temperature: 0.7,
    top_p: 0.95,
    top_k: 50,
    repetition_penalty: 1.05,
    guidance_scale: 3.0,
    num_inference_steps: 10,
    seed: -1,
    instructions: "",
  };

  const updateAudioCpp = (patch: Partial<typeof audiocpp>) => {
    update({
      audiocpp: {
        ...audiocpp,
        ...patch,
      },
    });
  };

  return (
    <>
      <SettingsGroup title={t("settings.speech.audiocppSettings.title")}>
        {/* Language Selection */}
        <SettingContainer
          title={t("settings.speech.audiocppLanguage.label")}
          description={t("settings.speech.audiocppLanguage.description")}
          grouped
        >
          <Dropdown
            options={AUDIOCPP_LANGUAGES.map((lang) => ({
              value: lang.value,
              label:
                lang.value === "auto"
                  ? t("settings.speech.audiocppLanguage.optionAuto")
                  : lang.label,
            }))}
            selectedValue={audiocpp.language ?? "auto"}
            onSelect={(value) => {
              updateAudioCpp({ language: value });
            }}
          />
        </SettingContainer>

        {/* Style / Emotion Instruction Prompt */}
        <SettingContainer
          title={t("settings.speech.audiocppInstructions.label")}
          description={t("settings.speech.audiocppInstructions.description")}
          grouped
        >
          <Input
            type="text"
            className="w-full"
            placeholder={t("settings.speech.audiocppInstructions.placeholder")}
            value={audiocpp.instructions ?? ""}
            onChange={(e) => {
              updateAudioCpp({ instructions: e.target.value });
            }}
          />
        </SettingContainer>
      </SettingsGroup>

      {/* Advanced Generation Parameters */}
      <SettingsGroup
        title={
          <button
            type="button"
            className="flex items-center gap-2 text-sm font-semibold hover:text-logo-primary transition-colors cursor-pointer"
            onClick={() => setShowAdvanced(!showAdvanced)}
          >
            {showAdvanced ? (
              <ChevronDown className="w-4 h-4 text-logo-primary" />
            ) : (
              <ChevronRight className="w-4 h-4 text-mid-gray" />
            )}
            <Sliders className="w-4 h-4 text-logo-primary" />
            <span>{t("settings.speech.audiocppAdvanced.title")}</span>
          </button>
        }
        description={t("settings.speech.audiocppAdvanced.description")}
      >
        {showAdvanced && (
          <div className="space-y-1">
            {/* Temperature */}
            <Slider
              label={t("settings.speech.audiocppTemperature.label")}
              description={t("settings.speech.audiocppTemperature.description")}
              min={0.0}
              max={1.5}
              step={0.05}
              value={audiocpp.temperature ?? 0.7}
              onChange={(temperature) => updateAudioCpp({ temperature })}
              onReset={() => updateAudioCpp({ temperature: 0.7 })}
              formatValue={(v) => v.toFixed(2)}
              grouped
            />

            {/* Top-P */}
            <Slider
              label={t("settings.speech.audiocppTopP.label")}
              description={t("settings.speech.audiocppTopP.description")}
              min={0.1}
              max={1.0}
              step={0.05}
              value={audiocpp.top_p ?? 0.95}
              onChange={(top_p) => updateAudioCpp({ top_p })}
              onReset={() => updateAudioCpp({ top_p: 0.95 })}
              formatValue={(v) => v.toFixed(2)}
              grouped
            />

            {/* Top-K */}
            <Slider
              label={t("settings.speech.audiocppTopK.label")}
              description={t("settings.speech.audiocppTopK.description")}
              min={0}
              max={100}
              step={1}
              value={audiocpp.top_k ?? 50}
              onChange={(top_k) => updateAudioCpp({ top_k })}
              onReset={() => updateAudioCpp({ top_k: 50 })}
              formatValue={(v) => (v === 0 ? "Disabled" : v.toString())}
              grouped
            />

            {/* Repetition Penalty */}
            <Slider
              label={t("settings.speech.audiocppRepetitionPenalty.label")}
              description={t(
                "settings.speech.audiocppRepetitionPenalty.description",
              )}
              min={1.0}
              max={2.0}
              step={0.05}
              value={audiocpp.repetition_penalty ?? 1.05}
              onChange={(repetition_penalty) =>
                updateAudioCpp({ repetition_penalty })
              }
              onReset={() => updateAudioCpp({ repetition_penalty: 1.05 })}
              formatValue={(v) => v.toFixed(2)}
              grouped
            />

            {/* Guidance Scale (Diffusion / Flow Matching / Emotion) */}
            <Slider
              label={t("settings.speech.audiocppGuidanceScale.label")}
              description={t(
                "settings.speech.audiocppGuidanceScale.description",
              )}
              min={1.0}
              max={10.0}
              step={0.5}
              value={audiocpp.guidance_scale ?? 3.0}
              onChange={(guidance_scale) => updateAudioCpp({ guidance_scale })}
              onReset={() => updateAudioCpp({ guidance_scale: 3.0 })}
              formatValue={(v) => v.toFixed(1)}
              grouped
            />

            {/* Inference Steps */}
            <Slider
              label={t("settings.speech.audiocppInferenceSteps.label")}
              description={t(
                "settings.speech.audiocppInferenceSteps.description",
              )}
              min={5}
              max={50}
              step={1}
              value={audiocpp.num_inference_steps ?? 10}
              onChange={(num_inference_steps) =>
                updateAudioCpp({ num_inference_steps })
              }
              onReset={() => updateAudioCpp({ num_inference_steps: 10 })}
              formatValue={(v) => v.toString()}
              grouped
            />

            {/* Deterministic Seed */}
            <SettingContainer
              title={t("settings.speech.audiocppSeed.label")}
              description={t("settings.speech.audiocppSeed.description")}
              grouped
            >
              <div className="flex items-center gap-2">
                <Input
                  type="number"
                  className="w-36"
                  placeholder="-1 (Random)"
                  value={audiocpp.seed ?? -1}
                  onChange={(e) => {
                    const val = parseInt(e.target.value, 10);
                    updateAudioCpp({ seed: isNaN(val) ? -1 : val });
                  }}
                />
                <button
                  type="button"
                  className="px-2.5 py-1 text-xs rounded border border-mid-gray/40 hover:border-logo-primary hover:bg-logo-primary/10 transition-colors"
                  onClick={() => updateAudioCpp({ seed: -1 })}
                >
                  {t("settings.speech.audiocppSeed.randomBtn")}
                </button>
              </div>
            </SettingContainer>
          </div>
        )}
      </SettingsGroup>

      {/* Hardware & Compute Execution */}
      <SettingsGroup
        title={
          <button
            type="button"
            className="flex items-center gap-2 text-sm font-semibold hover:text-logo-primary transition-colors cursor-pointer"
            onClick={() => setShowHardware(!showHardware)}
          >
            {showHardware ? (
              <ChevronDown className="w-4 h-4 text-logo-primary" />
            ) : (
              <ChevronRight className="w-4 h-4 text-mid-gray" />
            )}
            <Cpu className="w-4 h-4 text-logo-primary" />
            <span>{t("settings.speech.audiocppHardware.title")}</span>
          </button>
        }
        description={t("settings.speech.audiocppHardware.description")}
      >
        {showHardware && (
          <div className="space-y-1">
            {/* Backend Selection */}
            <SettingContainer
              title={t("settings.speech.audiocppBackend.label")}
              description={t("settings.speech.audiocppBackend.description")}
              grouped
            >
              <Dropdown
                options={[
                  {
                    value: "cuda",
                    label: t("settings.speech.audiocppBackend.cuda"),
                  },
                  {
                    value: "vulkan",
                    label: t("settings.speech.audiocppBackend.vulkan"),
                  },
                  {
                    value: "cpu",
                    label: t("settings.speech.audiocppBackend.cpu"),
                  },
                ]}
                selectedValue={audiocpp.backend ?? "cuda"}
                onSelect={async (value) => {
                  updateAudioCpp({ backend: value });
                  await commands.ttsUnloadEngine();
                }}
              />
            </SettingContainer>

            {/* CPU Threads */}
            <Slider
              label={t("settings.speech.audiocppThreads.label")}
              description={t("settings.speech.audiocppThreads.description")}
              min={1}
              max={32}
              step={1}
              value={audiocpp.threads ?? 4}
              onChange={async (threads) => {
                updateAudioCpp({ threads });
                await commands.ttsUnloadEngine();
              }}
              onReset={() => updateAudioCpp({ threads: 4 })}
              formatValue={(v) => v.toString()}
              grouped
            />

            {/* GPU Device ID */}
            <SettingContainer
              title={t("settings.speech.audiocppDeviceId.label")}
              description={t("settings.speech.audiocppDeviceId.description")}
              grouped
            >
              <Input
                type="number"
                min={0}
                max={8}
                className="w-24"
                value={audiocpp.device_id ?? 0}
                onChange={async (e) => {
                  const val = parseInt(e.target.value, 10);
                  updateAudioCpp({ device_id: isNaN(val) ? 0 : val });
                  await commands.ttsUnloadEngine();
                }}
              />
            </SettingContainer>
          </div>
        )}
      </SettingsGroup>
    </>
  );
};
