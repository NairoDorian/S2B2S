import React, { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { RefreshCcw } from "lucide-react";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { Slider } from "../../ui/Slider";
import { Textarea } from "../../ui/Textarea";
import { Button } from "../../ui/Button";
import { ResetButton } from "../../ui/ResetButton";
import { Alert } from "../../ui/Alert";
import { useSettings } from "../../../hooks/useSettings";
import { commands } from "@/bindings";
import type { BrainConfig } from "@/bindings";

import { ProviderSelect } from "../PostProcessingSettingsApi/ProviderSelect";
import { BaseUrlField } from "../PostProcessingSettingsApi/BaseUrlField";
import { ApiKeyField } from "../PostProcessingSettingsApi/ApiKeyField";
import { ModelSelect } from "../PostProcessingSettingsApi/ModelSelect";
import { useBrainProviderState } from "./useBrainProviderState";
import { useLlamaState } from "../../../hooks/useLlamaState";

const LlamaDownloadPanel: React.FC<{
  llamaState: ReturnType<typeof useLlamaState>;
}> = ({ llamaState }) => {
  return (
    <div className="p-5 rounded-lg border border-logo-primary/20 bg-gradient-to-br from-logo-primary/5 via-logo-primary/[0.02] to-transparent backdrop-blur-sm space-y-4">
      <div className="flex items-start justify-between">
        <div className="space-y-1">
          <h4 className="text-sm font-semibold text-text flex items-center gap-2">
            Local Gemma-4 Engine (Llama.cpp)
            <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-semibold bg-amber-500/10 text-amber-400 border border-amber-500/20">
              Setup Required
            </span>
          </h4>
          <p className="text-xs text-mid-gray max-w-xl">
            To run the Brain locally, S2B2S compiles and executes llama.cpp on
            your machine. We need to download the specialized Gemma-4 UD-Q4_K_XL
            model, draft model for Multi-Token Prediction, and vision projector
            (total size ~2.2 GB).
          </p>
        </div>
      </div>

      {llamaState.error && (
        <Alert variant="error" contained>
          {llamaState.error}
        </Alert>
      )}

      {llamaState.isDownloading ? (
        <div className="space-y-2">
          <div className="flex justify-between text-xs font-medium text-mid-gray">
            <span className="truncate max-w-[280px]">
              {llamaState.currentFile
                ? `Downloading ${llamaState.currentFile}...`
                : "Downloading models..."}
            </span>
            <span className="flex gap-2">
              <span>{llamaState.downloadSpeed.toFixed(1)} MB/s</span>
              <span className="text-logo-primary font-semibold">
                {llamaState.downloadProgress.toFixed(1)}%
              </span>
            </span>
          </div>
          <div className="w-full bg-black/40 rounded-full h-2 overflow-hidden border border-white/5 relative">
            <div
              className="bg-gradient-to-r from-logo-primary via-purple-500 to-indigo-500 h-full rounded-full transition-all duration-300 ease-out shadow-[0_0_8px_rgba(168,85,247,0.5)]"
              style={{ width: `${llamaState.downloadProgress}%` }}
            />
          </div>
        </div>
      ) : (
        <Button
          variant="primary"
          onClick={() => void llamaState.startDownload()}
          className="w-full justify-center py-2.5 font-medium shadow-[0_4px_12px_rgba(0,0,0,0.2)] hover:shadow-[0_4px_16px_rgba(168,85,247,0.25)] transition-all"
        >
          Download Gemma-4 Local Suite (~2.2 GB)
        </Button>
      )}
    </div>
  );
};

const LlamaStatusCard: React.FC = () => {
  return (
    <div className="p-4 rounded-lg border border-green-500/10 bg-green-500/[0.02] backdrop-blur-sm grid grid-cols-2 gap-3 text-xs">
      <div className="col-span-2 border-b border-white/5 pb-2 mb-1 flex items-center justify-between">
        <span className="font-semibold text-text flex items-center gap-1.5">
          <span className="h-2 w-2 rounded-full bg-green-500 animate-pulse" />
          Local Gemma-4 Engine
        </span>
        <span className="text-[10px] px-2 py-0.5 bg-green-500/15 text-green-400 font-bold rounded">
          ACTIVE
        </span>
      </div>
      <div>
        <span className="text-mid-gray block">Model</span>
        <span className="font-medium text-text">
          Gemma-4-E2B-it-qat (UD-Q4_K_XL)
        </span>
      </div>
      <div>
        <span className="text-mid-gray block">MTP Acceleration</span>
        <span className="font-medium text-text">Enabled</span>
      </div>
      <div>
        <span className="text-mid-gray block">Vision Component</span>
        <span className="font-medium text-text">Disabled by default</span>
      </div>
      <div>
        <span className="text-mid-gray block">Execution Engine</span>
        <span className="font-medium text-text">
          llama-server (Flash Attention)
        </span>
      </div>
    </div>
  );
};

export const BrainSettings: React.FC = () => {
  const { t } = useTranslation();
  const { settings, updateSetting, isUpdating } = useSettings();
  const state = useBrainProviderState();
  const llamaState = useLlamaState();
  const [testState, setTestState] = useState<
    "idle" | "running" | "ok" | "error"
  >("idle");
  const [testReply, setTestReply] = useState("");
  const [testMetrics, setTestMetrics] = useState<{
    tokensPerSec?: number;
    totalMs?: number;
  }>({});

  const brain = settings?.brain;

  const update = useCallback(
    (patch: Partial<BrainConfig>) => {
      if (!brain) return;
      void updateSetting("brain", { ...brain, ...patch });
    },
    [brain, updateSetting],
  );

  if (!brain) return null;

  const testBrain = async () => {
    setTestState("running");
    setTestReply("");
    setTestMetrics({});

    const startTime = performance.now();
    // Capture server metrics from brain:done event.
    // Keep listener alive until we get metrics or a reasonable timeout.
    let capturedMetrics: { tps?: number; ms?: number } = {};
    let done = false;
    const unlistenPromise = listen<{
      tokens_per_sec?: number;
      predicted_ms?: number;
    }>("brain:done", (event) => {
      const p = event.payload;
      if (typeof p === "object") {
        capturedMetrics = {
          tps: p.tokens_per_sec,
          ms: p.predicted_ms ?? undefined,
        };
        done = true;
      }
    });

    const result = await commands.brainAsk(t("settings.brain.test.prompt"));

    // Wait briefly for the brain:done event to arrive (it's emitted in Rust
    // during the command but queued after the response in Tauri's IPC).
    if (!done) {
      await new Promise((r) => setTimeout(r, 100));
    }

    void unlistenPromise.then((fn) => fn());

    if (result.status === "ok") {
      setTestReply(result.data);
      // Use server metrics, fall back to client-side timing
      if (capturedMetrics.tps != null && capturedMetrics.tps > 0) {
        setTestMetrics({
          tokensPerSec: capturedMetrics.tps,
          totalMs: capturedMetrics.ms,
        });
      } else {
        const elapsedMs = Math.round(performance.now() - startTime);
        const estimatedTokens = Math.max(1, result.data.length / 4);
        const tokensPerSec =
          elapsedMs > 0
            ? parseFloat(((estimatedTokens / elapsedMs) * 1000).toFixed(1))
            : 0;
        setTestMetrics({ tokensPerSec, totalMs: elapsedMs });
      }
      setTestState("ok");
    } else {
      setTestReply(String(result.error));
      setTestState("error");
    }
  };

  return (
    <div className="space-y-6">
      <SettingsGroup title={t("settings.brain.group")}>
        <ToggleSwitch
          checked={brain.enabled}
          onChange={(enabled) => update({ enabled })}
          isUpdating={isUpdating("brain")}
          label={t("settings.brain.enabled.label")}
          description={t("settings.brain.enabled.description")}
          grouped
        />

        <SettingContainer
          title={t("settings.postProcessing.api.provider.title")}
          description={t("settings.postProcessing.api.provider.description")}
          descriptionMode="tooltip"
          layout="horizontal"
          grouped={true}
        >
          <div className="flex items-center gap-2">
            <ProviderSelect
              options={state.providerOptions}
              value={state.selectedProviderId}
              onChange={state.handleProviderSelect}
            />
          </div>
        </SettingContainer>

        {state.selectedProviderId === "llama_cpp" ? (
          <div className="space-y-4 pt-2">
            {!llamaState.isDownloaded || llamaState.isDownloading ? (
              <LlamaDownloadPanel llamaState={llamaState} />
            ) : (
              <>
                <SettingContainer
                  title={t("settings.postProcessing.api.baseUrl.title")}
                  description={t(
                    "settings.postProcessing.api.baseUrl.description",
                  )}
                  descriptionMode="tooltip"
                  layout="horizontal"
                  grouped={true}
                >
                  <div className="flex items-center gap-2">
                    <BaseUrlField
                      value={state.baseUrl}
                      onBlur={state.handleBaseUrlChange}
                      placeholder={t(
                        "settings.postProcessing.api.baseUrl.placeholder",
                      )}
                      disabled={state.isBaseUrlUpdating}
                      className="min-w-[380px]"
                    />
                  </div>
                </SettingContainer>

                <SettingContainer
                  title="Engine Status"
                  description="Status and properties of the active local llama.cpp server."
                  descriptionMode="tooltip"
                  layout="stacked"
                  grouped={true}
                >
                  <LlamaStatusCard />
                </SettingContainer>
              </>
            )}
          </div>
        ) : state.isAppleProvider ? (
          state.appleIntelligenceUnavailable ? (
            <Alert variant="error" contained>
              {t("settings.postProcessing.api.appleIntelligence.unavailable")}
            </Alert>
          ) : null
        ) : (
          <>
            {state.isCustomProvider && (
              <SettingContainer
                title={t("settings.postProcessing.api.baseUrl.title")}
                description={t(
                  "settings.postProcessing.api.baseUrl.description",
                )}
                descriptionMode="tooltip"
                layout="horizontal"
                grouped={true}
              >
                <div className="flex items-center gap-2">
                  <BaseUrlField
                    value={state.baseUrl}
                    onBlur={state.handleBaseUrlChange}
                    placeholder={t(
                      "settings.postProcessing.api.baseUrl.placeholder",
                    )}
                    disabled={state.isBaseUrlUpdating}
                    className="min-w-[380px]"
                  />
                </div>
              </SettingContainer>
            )}

            <SettingContainer
              title={t("settings.postProcessing.api.apiKey.title")}
              description={t("settings.postProcessing.api.apiKey.description")}
              descriptionMode="tooltip"
              layout="horizontal"
              grouped={true}
            >
              <div className="flex items-center gap-2">
                <ApiKeyField
                  value={state.apiKey}
                  onBlur={state.handleApiKeyChange}
                  placeholder={t(
                    "settings.postProcessing.api.apiKey.placeholder",
                  )}
                  disabled={state.isApiKeyUpdating}
                  className="min-w-[320px]"
                />
              </div>
            </SettingContainer>

            {!state.isAppleProvider && (
              <SettingContainer
                title={t("settings.postProcessing.api.model.title")}
                description={
                  state.isCustomProvider
                    ? t("settings.postProcessing.api.model.descriptionCustom")
                    : t("settings.postProcessing.api.model.descriptionDefault")
                }
                descriptionMode="tooltip"
                layout="stacked"
                grouped={true}
              >
                <div className="flex items-center gap-2">
                  <ModelSelect
                    value={state.model}
                    options={state.modelOptions}
                    disabled={state.isModelUpdating}
                    isLoading={state.isFetchingModels}
                    placeholder={
                      state.modelOptions.length > 0
                        ? t(
                            "settings.postProcessing.api.model.placeholderWithOptions",
                          )
                        : t(
                            "settings.postProcessing.api.model.placeholderNoOptions",
                          )
                    }
                    onSelect={state.handleModelSelect}
                    onCreate={state.handleModelCreate}
                    onBlur={() => {}}
                    className="flex-1 min-w-[380px]"
                  />
                  <ResetButton
                    onClick={state.handleRefreshModels}
                    disabled={state.isFetchingModels}
                    ariaLabel={t(
                      "settings.postProcessing.api.model.refreshModels",
                    )}
                    className="flex h-10 w-10 items-center justify-center"
                  >
                    <RefreshCcw
                      className={`h-4 w-4 ${state.isFetchingModels ? "animate-spin" : ""}`}
                    />
                  </ResetButton>
                </div>
              </SettingContainer>
            )}
          </>
        )}
      </SettingsGroup>

      <SettingsGroup title={t("settings.brain.behaviorGroup")}>
        <SettingContainer
          title={t("settings.brain.systemPrompt.label")}
          description={t("settings.brain.systemPrompt.description")}
          grouped
          layout="stacked"
        >
          <Textarea
            variant="compact"
            rows={4}
            value={brain.system_prompt}
            onChange={(e) => update({ system_prompt: e.target.value })}
          />
        </SettingContainer>
        <SettingContainer
          title="Warmup Prompt"
          description="Dummy prompt sent to warm up the model when it loads into VRAM. Leave empty to skip warmup."
          grouped
          layout="stacked"
        >
          <Textarea
            variant="compact"
            rows={2}
            value={brain.warmup_prompt ?? ""}
            onChange={(e) => update({ warmup_prompt: e.target.value })}
          />
        </SettingContainer>
        <Slider
          value={brain.context_turns}
          onChange={(turns) => update({ context_turns: Math.round(turns) })}
          min={0}
          max={20}
          step={1}
          label={t("settings.brain.contextTurns.label")}
          description={t("settings.brain.contextTurns.description")}
          grouped
          showValue
          formatValue={(value) => `${Math.round(value)}`}
        />
        <ToggleSwitch
          checked={brain.read_aloud}
          onChange={(read_aloud) => update({ read_aloud })}
          label={t("settings.brain.readAloud.label")}
          description={t("settings.brain.readAloud.description")}
          grouped
        />
      </SettingsGroup>

      {state.selectedProviderId === "llama_cpp" && (
        <SettingsGroup title="Multimodal Input (Gemma 4)">
          <ToggleSwitch
            checked={brain.multimodal_audio_enabled ?? true}
            onChange={(multimodal_audio_enabled) =>
              update({ multimodal_audio_enabled })
            }
            label="Audio Input (Default: On)"
            description="Sends the raw WAV recording alongside text to the Brain model. Gemma 4 processes the audio natively for enhanced transcription accuracy. Enabled by default for the local Gemma 4 model."
            grouped
          />
          <ToggleSwitch
            checked={brain.multimodal_image_enabled ?? false}
            onChange={(multimodal_image_enabled) =>
              update({ multimodal_image_enabled })
            }
            label="Image Input"
            description="Enable image (screenshot) input support. When active, you can send a screenshot alongside text prompts for the AI to see and describe your screen."
            grouped
          />
        </SettingsGroup>
      )}

      <SettingsGroup title={t("settings.brain.testGroup")}>
        <SettingContainer
          title={t("settings.brain.test.label")}
          description={t("settings.brain.test.description")}
          grouped
          layout="stacked"
        >
          <div className="space-y-2">
            <div className="flex gap-2">
              <Button
                variant="primary-soft"
                size="sm"
                disabled={testState === "running" || !brain.enabled}
                onClick={() => void testBrain()}
              >
                {testState === "running"
                  ? t("settings.brain.test.running")
                  : t("settings.brain.test.button")}
              </Button>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => void commands.brainAbort()}
              >
                {t("settings.brain.test.abort")}
              </Button>
            </div>
            {testReply && (
              <div className="space-y-1">
                <p
                  className={`text-sm whitespace-pre-wrap ${
                    testState === "error" ? "text-red-500" : "text-mid-gray"
                  }`}
                >
                  {testReply}
                </p>
                {(testMetrics.tokensPerSec != null ||
                  testMetrics.totalMs != null) && (
                  <p className="text-[10px] text-text/30 font-mono flex gap-3">
                    {testMetrics.tokensPerSec != null && (
                      <span>{testMetrics.tokensPerSec.toFixed(1)} t/s</span>
                    )}
                    {testMetrics.totalMs != null && (
                      <span>🧠 {testMetrics.totalMs}ms</span>
                    )}
                  </p>
                )}
              </div>
            )}
          </div>
        </SettingContainer>
      </SettingsGroup>
    </div>
  );
};
