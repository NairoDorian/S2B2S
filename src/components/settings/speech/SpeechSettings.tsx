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
import { ExternalLink, Terminal, Upload } from "lucide-react";

const ENGINES: TtsEngine[] = [
  "piper",
  "kokoro",
  "kitten",
  "pocket",
  "qwen3",
  "sapi",
  "openai",
  "elevenlabs",
  "cartesia",
];

const ENGINE_BADGES: Record<
  string,
  ("offline" | "free" | "cloud" | "paid" | "freemium")[]
> = {
  piper: ["offline", "free"],
  kokoro: ["offline", "free"],
  kitten: ["offline", "free"],
  pocket: ["offline", "free"],
  qwen3: ["offline", "free"],
  sapi: ["offline", "free"],
  openai: ["cloud", "paid"],
  elevenlabs: ["cloud", "freemium"],
  cartesia: ["cloud", "freemium"],
};

const BADGE_COLORS: Record<string, string> = {
  offline: "bg-blue-500/15 text-blue-400 ring-blue-500/30",
  free: "bg-green-500/15 text-green-400 ring-green-500/30",
  cloud: "bg-violet-500/15 text-violet-400 ring-violet-500/30",
  paid: "bg-amber-500/15 text-amber-400 ring-amber-500/30",
  freemium: "bg-yellow-500/15 text-yellow-400 ring-yellow-500/30",
};

// Engines whose backend actually applies `speed`: piper/kokoro via length_scale,
// sapi via SetRate, openai/elevenlabs via the API. Kitten & Pocket have no speed
// control in the underlying model, and Cartesia isn't plumbed — so the slider is
// hidden for those to avoid a dead control. Keep in sync with the TtsBackend impls.
const SPEED_CAPABLE_ENGINES = new Set<TtsEngine>([
  "piper",
  "kokoro",
  "sapi",
  "openai",
  "elevenlabs",
]);
const engineSupportsSpeed = (engine?: TtsEngine | null): boolean =>
  !!engine && SPEED_CAPABLE_ENGINES.has(engine);

export const SpeechSettings: React.FC = () => {
  const { t } = useTranslation();
  const { settings, updateSetting, isUpdating } = useSettings();
  const [voices, setVoices] = useState<Voice[]>([]);
  const [speaking, setSpeaking] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{
    success: boolean;
    message: string;
  } | null>(null);
  const [importingVoice, setImportingVoice] = useState(false);

  const tts = settings?.tts;

  const update = useCallback(
    (patch: Partial<TtsConfig>) => {
      if (!tts) return;
      void updateSetting("tts", { ...tts, ...patch });
    },
    [tts, updateSetting],
  );

  const [greetingVoices, setGreetingVoices] = useState<Voice[]>([]);

  const refreshVoices = useCallback(async () => {
    const result = await commands.ttsGetVoices(null);
    if (result.status === "ok") {
      setVoices(result.data);
    }
  }, []);

  const refreshGreetingVoices = useCallback(async () => {
    if (!tts) return;
    const greeting = tts.greeting ?? {
      text: "Hello, how can I help?",
      speed: 1.0,
      voice: "",
      engine: "piper" as TtsEngine,
      noise_scale: 0.667,
      noise_w_scale: 0.8,
    };
    const result = await commands.ttsGetVoices(greeting.engine ?? null);
    if (result.status === "ok") {
      setGreetingVoices(result.data);
    }
  }, [tts?.greeting?.engine]);

  useEffect(() => {
    void refreshVoices();
  }, [refreshVoices, tts?.engine, tts?.piper.data_dir]);

  useEffect(() => {
    void refreshGreetingVoices();
  }, [refreshGreetingVoices, tts?.greeting?.engine, tts?.piper.data_dir]);

  if (!tts) return null;

  const sanitization = tts.sanitization ?? {
    enabled: true,
    markdown: true,
    tts_normalization: true,
  };

  const greeting = tts.greeting ?? {
    text: "Hello, how can I help?",
    speed: 1.0,
    voice: "",
    engine: "piper" as TtsEngine,
    noise_scale: 0.667,
    noise_w_scale: 0.8,
  };

  const handleTestEngine = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      const result = await commands.ttsSpeak(
        "Hello, this is a test of the S2B2S speech engine. If you can hear this clearly, the engine is working correctly.",
      );
      if (result.status === "ok") {
        setTestResult({
          success: true,
          message: "Engine test completed successfully.",
        });
      } else {
        setTestResult({
          success: false,
          message: result.error || "Test failed",
        });
      }
    } catch (err) {
      setTestResult({ success: false, message: String(err) });
    } finally {
      setTesting(false);
    }
  };

  const handleCloneVoice = async () => {
    setImportingVoice(true);
    try {
      // Use Tauri dialog to pick a WAV file
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        filters: [{ name: "WAV Audio", extensions: ["wav"] }],
        multiple: false,
      });
      if (selected && typeof selected === "string") {
        const result = await commands.pocketImportClonedVoice(selected);
        if (result.status === "ok") {
          await refreshVoices();
          update({ voice: result.data.id });
        }
      }
    } catch (err) {
      console.error("Failed to import cloned voice:", err);
    } finally {
      setImportingVoice(false);
    }
  };

  const getEngineLink = (engine: string): string | null => {
    try {
      return t(`settings.speech.engine.links.${engine}`, "") || null;
    } catch {
      return null;
    }
  };
  const getEngineLinkLabel = (engine: string): string | null => {
    try {
      return t(`settings.speech.engine.links.${engine}Label`, "");
    } catch {
      return "";
    }
  };

  const isLocalEngine = (engine: string) =>
    ["piper", "kokoro", "kitten", "pocket", "qwen3", "sapi"].includes(engine);

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
            onSelect={(value) => {
              update({ engine: value as TtsEngine });
              setTestResult(null);
            }}
          />
        </SettingContainer>

        {/* Engine description + badges + link */}
        <div className="px-4 pb-1 flex flex-col gap-1.5">
          <div className="flex items-center gap-1.5 flex-wrap">
            {(ENGINE_BADGES[tts.engine] || []).map((badge) => (
              <span
                key={badge}
                className={`rounded-full px-2 py-0.5 text-[10px] font-medium ring-1 ${BADGE_COLORS[badge] || ""}`}
              >
                {t(`settings.speech.engine.badges.${badge}`, badge)}
              </span>
            ))}
          </div>
          <p className="text-[11px] text-text/40 leading-relaxed">
            {t(`settings.speech.engine.descriptions.${tts.engine}`, "")}
          </p>
          {getEngineLink(tts.engine) && (
            <a
              href={getEngineLink(tts.engine)!}
              target="_blank"
              rel="noopener noreferrer"
              className="text-logo-primary hover:underline flex items-center gap-1 text-[11px] w-fit"
            >
              <ExternalLink className="w-3 h-3" />
              {getEngineLinkLabel(tts.engine)}
            </a>
          )}
        </div>

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
        {engineSupportsSpeed(tts.engine) && (
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
        )}
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
          checked={tts.tts_shorten_first_chunk ?? true}
          onChange={(tts_shorten_first_chunk) =>
            update({ tts_shorten_first_chunk })
          }
          label={t("settings.speech.fastFirstSentence.label")}
          description={t("settings.speech.fastFirstSentence.description")}
          grouped
        />
      </SettingsGroup>

      {/* Test Engine */}
      <SettingsGroup title={t("settings.speech.testEngine.label")}>
        <SettingContainer
          title=""
          description={t("settings.speech.testEngine.description")}
          grouped
        >
          <div className="flex flex-col gap-2">
            <div className="flex gap-2">
              <Button
                variant="secondary"
                size="sm"
                disabled={!tts.enabled || testing}
                onClick={handleTestEngine}
              >
                {testing
                  ? t("settings.speech.testEngine.testing")
                  : t("settings.speech.testEngine.button")}
              </Button>
            </div>
            {testResult && (
              <div
                className={`p-2 rounded text-xs border ${testResult.success ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/20" : "bg-red-500/10 text-red-400 border-red-500/20"}`}
              >
                {testResult.success
                  ? t("settings.speech.testEngine.engineWorking")
                  : t("settings.speech.testEngine.engineFailed")}
                {testResult.message && (
                  <div className="text-[10px] text-text/40 mt-0.5">
                    {testResult.message}
                  </div>
                )}
              </div>
            )}
          </div>
        </SettingContainer>
      </SettingsGroup>

      {/* Command Preview (local engines only) */}
      {isLocalEngine(tts.engine) && (
        <SettingsGroup title={t("settings.speech.commandPreview.title")}>
          <div className="border border-mid-gray/20 rounded-md">
            <div className="flex items-center gap-2 px-3 py-2 text-xs text-text/50">
              <Terminal size={13} className="shrink-0" />
              <span className="font-medium">
                {t("settings.speech.commandPreview.title")}
              </span>
            </div>
            <div className="border-t border-mid-gray/10 px-3 py-2.5">
              <pre className="bg-mid-gray/10 text-text/60 overflow-x-auto rounded px-3 py-2 font-mono text-[11px] leading-relaxed break-all whitespace-pre-wrap">
                {`python ${tts.engine}_server.py --port <auto> --host 127.0.0.1`}
              </pre>
              <p className="text-text/30 mt-1.5 text-[10px]">
                {t("settings.speech.commandPreview.placeholder")}
              </p>
            </div>
          </div>
        </SettingsGroup>
      )}

      {/* Pocket Voice Cloning */}
      {tts.engine === "pocket" && (
        <SettingsGroup title="Clone Voice (Pocket TTS)">
          <SettingContainer
            title="Import reference audio"
            description="Select a 5-20 second WAV file of someone speaking clearly. Pocket TTS will clone that voice."
            grouped
          >
            <Button
              variant="secondary"
              size="sm"
              disabled={importingVoice}
              onClick={handleCloneVoice}
            >
              <Upload size={14} className="mr-1" />
              {importingVoice ? "Importing..." : "Select WAV File"}
            </Button>
          </SettingContainer>
          {voices.some((v) => v.language === "cloned") && (
            <div className="px-4 pb-3 text-[11px] text-text/40">
              {t("speech.clonedVoicesHint")}
            </div>
          )}
        </SettingsGroup>
      )}

      {/* Qwen3 Voice Cloning */}
      {tts.engine === "qwen3" && (
        <SettingsGroup title="Clone Voice (Qwen3-TTS)">
          <SettingContainer
            title="Import reference audio"
            description="Select a 5-20 second WAV file of someone speaking clearly. Qwen3-TTS will clone that voice."
            grouped
          >
            <Button
              variant="secondary"
              size="sm"
              disabled={importingVoice}
              onClick={async () => {
                setImportingVoice(true);
                try {
                  const { open } = await import("@tauri-apps/plugin-dialog");
                  const selected = await open({
                    filters: [{ name: "WAV Audio", extensions: ["wav"] }],
                    multiple: false,
                  });
                  if (selected && typeof selected === "string") {
                    const result =
                      await commands.qwen3ImportClonedVoice(selected);
                    if (result.status === "ok") {
                      await refreshVoices();
                      update({ voice: result.data.id });
                    }
                  }
                } catch (err) {
                  console.error("Failed to import cloned voice:", err);
                } finally {
                  setImportingVoice(false);
                }
              }}
            >
              <Upload size={14} className="mr-1" />
              {importingVoice ? "Importing..." : "Select WAV File"}
            </Button>
          </SettingContainer>
          {voices.some((v) => v.language === "cloned") && (
            <div className="px-4 pb-3 text-[11px] text-text/40">
              {t("speech.clonedVoicesHint")}
            </div>
          )}
        </SettingsGroup>
      )}

      <SettingsGroup title={t("settings.speech.greetingGroup")}>
        <ToggleSwitch
          checked={tts.play_startup_greeting ?? true}
          onChange={(play_startup_greeting) =>
            update({ play_startup_greeting })
          }
          label={t("settings.speech.playGreetingToggle.label")}
          description={t("settings.speech.playGreetingToggle.description")}
          grouped
        />
        {tts.play_startup_greeting && (
          <>
            <SettingContainer
              title={t("settings.speech.greetingText.label")}
              description={t("settings.speech.greetingText.description")}
              grouped
            >
              <Input
                value={greeting.text}
                onChange={(e) =>
                  update({ greeting: { ...greeting, text: e.target.value } })
                }
                placeholder={t("settings.speech.greetingText.placeholder")}
              />
            </SettingContainer>
            <SettingContainer
              title={t("settings.speech.greetingEngine.label")}
              description={t("settings.speech.greetingEngine.description")}
              grouped
            >
              <Dropdown
                options={ENGINES.map((engine) => ({
                  value: engine,
                  label: t(`settings.speech.engine.options.${engine}`),
                }))}
                selectedValue={greeting.engine ?? null}
                onSelect={(value) => {
                  const newEngine = value as TtsEngine;
                  update({
                    greeting: { ...greeting, engine: newEngine, voice: "" },
                  });
                }}
              />
            </SettingContainer>
            <SettingContainer
              title={t("settings.speech.greetingVoice.label")}
              description={t("settings.speech.greetingVoice.description")}
              grouped
            >
              <Dropdown
                options={greetingVoices.map((voice) => ({
                  value: voice.id,
                  label: voice.name,
                }))}
                selectedValue={greeting.voice || null}
                onSelect={(voice) =>
                  update({ greeting: { ...greeting, voice } })
                }
                placeholder={t("settings.speech.voice.placeholder")}
                onRefresh={() => void refreshGreetingVoices()}
              />
            </SettingContainer>
            {engineSupportsSpeed(greeting.engine) && (
              <Slider
                value={greeting.speed ?? 1}
                onChange={(speed) =>
                  update({ greeting: { ...greeting, speed } })
                }
                min={0.5}
                max={2}
                step={0.05}
                label={t("settings.speech.greetingSpeed.label")}
                description={t("settings.speech.greetingSpeed.description")}
                grouped
                showValue
                formatValue={(value) => `${value.toFixed(2)}x`}
              />
            )}
            <Slider
              value={greeting.noise_scale ?? 0.667}
              onChange={(noise_scale) =>
                update({ greeting: { ...greeting, noise_scale } })
              }
              min={0}
              max={1.5}
              step={0.01}
              label={t("settings.speech.greetingNoiseScale.label")}
              description={t("settings.speech.greetingNoiseScale.description")}
              grouped
              showValue
              formatValue={(value) => `${value.toFixed(3)}`}
              onReset={() =>
                update({ greeting: { ...greeting, noise_scale: 0.667 } })
              }
            />
            <Slider
              value={greeting.noise_w_scale ?? 0.8}
              onChange={(noise_w_scale) =>
                update({ greeting: { ...greeting, noise_w_scale } })
              }
              min={0}
              max={1.5}
              step={0.01}
              label={t("settings.speech.greetingNoiseWScale.label")}
              description={t("settings.speech.greetingNoiseWScale.description")}
              grouped
              showValue
              formatValue={(value) => `${value.toFixed(3)}`}
              onReset={() =>
                update({ greeting: { ...greeting, noise_w_scale: 0.8 } })
              }
            />
          </>
        )}
        <SettingContainer
          title={t("settings.speech.repeatGreeting.label")}
          description={t("settings.speech.repeatGreeting.description")}
          grouped
        >
          <div className="flex gap-2">
            <Button
              variant="secondary"
              size="sm"
              disabled={!tts.enabled || speaking}
              onClick={async () => {
                setSpeaking(true);
                try {
                  await commands.ttsPlayGreeting();
                } finally {
                  setSpeaking(false);
                }
              }}
            >
              {t("settings.speech.playGreeting")}
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
