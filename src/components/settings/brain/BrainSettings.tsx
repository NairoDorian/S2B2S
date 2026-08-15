import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { RefreshCcw } from "lucide-react";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { Slider } from "../../ui/Slider";
import { Select } from "../../ui/Select";
import { Textarea } from "../../ui/Textarea";
import { Button } from "../../ui/Button";
import { ResetButton } from "../../ui/ResetButton";
import { Alert } from "../../ui/Alert";
import { useSettings } from "../../../hooks/useSettings";
import { commands } from "@/bindings";
import type { BrainConfig, Gemma4QuantCatalog } from "@/bindings";

import { ProviderSelect } from "../PostProcessingSettingsApi/ProviderSelect";
import { BaseUrlField } from "../PostProcessingSettingsApi/BaseUrlField";
import { ApiKeyField } from "../PostProcessingSettingsApi/ApiKeyField";
import { ModelSelect } from "../PostProcessingSettingsApi/ModelSelect";
import { useBrainProviderState } from "./useBrainProviderState";
import { useLlamaState } from "../../../hooks/useLlamaState";

const LlamaDownloadPanel: React.FC<{
  llamaState: ReturnType<typeof useLlamaState>;
}> = ({ llamaState }) => {
  const { t } = useTranslation();
  const { settings, updateSetting } = useSettings();
  const brain = settings?.brain;
  const variant = brain?.llama_model_variant ?? "standard";
  const [catalog, setCatalog] = useState<Gemma4QuantCatalog | null>(null);
  const [catalogError, setCatalogError] = useState<string | null>(null);

  const refreshCatalog = useCallback(
    async (targetVariant?: string) => {
      setCatalogError(null);
      const v = targetVariant ?? variant;
      const result = await commands.fetchGemma4Quants(v);
      if (result.status === "ok") {
        setCatalog(result.data);
      } else {
        setCatalogError(String(result.error));
      }
    },
    [variant],
  );

  useEffect(() => {
    void refreshCatalog(variant);
  }, [refreshCatalog, variant]);

  const quantUpdate = useCallback(
    (patch: Partial<BrainConfig>) => {
      if (!brain) return;
      void updateSetting("brain", { ...brain, ...patch });
    },
    [brain, updateSetting],
  );

  const handleVariantChange = (newVariant: string) => {
    const is4BVariant = newVariant === "4b" || newVariant === "e4b";
    const isMobileVariant = newVariant === "mobile";
    const defaultQuant = isMobileVariant
      ? "Q2_K_XL"
      : is4BVariant
        ? "Q4_K_XL"
        : "Q2_K_XL";
    quantUpdate({
      llama_model_variant: newVariant,
      llama_model_quant: defaultQuant,
    });
    void refreshCatalog(newVariant);
    void llamaState.refreshStatus();
  };

  const is4B = variant === "4b" || variant === "e4b";
  const isMobile = variant === "mobile";
  const mmprojEnabled =
    (brain?.llama_mmproj_enabled ?? true) &&
    brain?.llama_mmproj_quant !== "disabled";
  const mtpEnabled =
    (brain?.llama_mtp_enabled ?? true) && brain?.llama_mtp_quant !== "disabled";

  const quantOptions = (list: Gemma4QuantCatalog["model"]) =>
    list.map((q) => ({
      value: q.id,
      label: `${q.label} (${(q.size_mb ?? 0).toFixed(0)} MB)`,
    }));

  const modelSize = is4B ? 4.0 : isMobile ? 2.1 : 2.5;
  const mmprojSize = mmprojEnabled ? 0.95 : 0.0;
  const mtpSize = mtpEnabled ? 0.06 : 0.0;
  const totalEstimatedGB = (modelSize + mmprojSize + mtpSize).toFixed(1);
  const estimatedTotalSize = `~${totalEstimatedGB} GB`;

  const modelDisplayName = is4B
    ? "Gemma 4 4B IT QAT"
    : isMobile
      ? "Gemma 4 2B Mobile"
      : "Gemma 4 2B IT QAT";

  return (
    <div className="p-5 rounded-lg border border-logo-primary/20 bg-gradient-to-br from-logo-primary/5 via-logo-primary/[0.02] to-transparent backdrop-blur-sm space-y-4">
      <div className="flex items-start justify-between">
        <div className="space-y-1">
          <h4 className="text-sm font-semibold text-text flex items-center gap-2">
            {t("llamaCpp.localGemma.title")}
            {!llamaState.isDownloaded && (
              <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-semibold bg-amber-500/10 text-amber-400 border border-amber-500/20">
                {t("llamaCpp.localGemma.setupRequired")}
              </span>
            )}
          </h4>
          <p className="text-xs text-mid-gray max-w-xl">
            {t("llamaCpp.localGemma.brainDescription", {
              size: estimatedTotalSize,
            })}
          </p>
        </div>
      </div>

      {llamaState.error && (
        <Alert variant="error" contained>
          {llamaState.error}
        </Alert>
      )}

      {/* Model Choice Controls */}
      {brain && (
        <div className="space-y-3">
          {/* Quick Toggle for 4B vs 2B */}
          <ToggleSwitch
            checked={is4B}
            onChange={(checked) =>
              handleVariantChange(checked ? "4b" : "standard")
            }
            label={t("llamaCpp.localGemma.quickToggle.label")}
            description={t("llamaCpp.localGemma.quickToggle.description")}
            grouped
          />

          {/* Model Architecture Selector Dropdown */}
          <div className="space-y-1">
            <span className="text-xs font-medium text-mid-gray">
              {t("llamaCpp.localGemma.modelArchitecture")}
            </span>
            <Select
              value={is4B ? "4b" : isMobile ? "mobile" : "standard"}
              options={[
                {
                  value: "standard",
                  label: t("llamaCpp.localGemma.modelVariant.2b"),
                },
                {
                  value: "4b",
                  label: t("llamaCpp.localGemma.modelVariant.4b"),
                },
                {
                  value: "mobile",
                  label: t("llamaCpp.localGemma.modelVariant.mobile"),
                },
              ]}
              isClearable={false}
              onChange={(value) => value && handleVariantChange(value)}
            />
          </div>

          {/* Component Toggles for mmproj and MTP */}
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-1">
            <ToggleSwitch
              checked={brain.llama_mmproj_enabled ?? true}
              onChange={(enabled) =>
                quantUpdate({
                  llama_mmproj_enabled: enabled,
                  ...(!enabled
                    ? {}
                    : {
                        llama_mmproj_quant:
                          brain.llama_mmproj_quant === "disabled"
                            ? "F16"
                            : (brain.llama_mmproj_quant ?? "F16"),
                      }),
                })
              }
              label={t("llamaCpp.localGemma.mmprojToggle.label")}
              description={t("llamaCpp.localGemma.mmprojToggle.description")}
              grouped
            />
            <ToggleSwitch
              checked={brain.llama_mtp_enabled ?? true}
              onChange={(enabled) =>
                quantUpdate({
                  llama_mtp_enabled: enabled,
                  ...(!enabled
                    ? {}
                    : {
                        llama_mtp_quant:
                          brain.llama_mtp_quant === "disabled"
                            ? "Q4_0"
                            : (brain.llama_mtp_quant ?? "Q4_0"),
                      }),
                })
              }
              label={t("llamaCpp.localGemma.mtpToggle.label")}
              description={t("llamaCpp.localGemma.mtpToggle.description")}
              grouped
            />
          </div>

          {/* Quantization pickers: model GGUF, mmproj precision, MTP draft. */}
          <div className="grid grid-cols-1 sm:grid-cols-3 gap-3 pt-1">
            <div className="space-y-1">
              <span className="text-xs font-medium text-mid-gray">
                {t("llamaCpp.localGemma.modelQuant")}
              </span>
              <Select
                value={
                  brain.llama_model_quant ?? (is4B ? "Q4_K_XL" : "Q2_K_XL")
                }
                options={
                  isMobile
                    ? [{ value: "Q2_K_XL", label: "Q2_K_XL (Mobile)" }]
                    : quantOptions(catalog?.model ?? [])
                }
                isClearable={false}
                disabled={isMobile}
                onChange={(value) =>
                  value && quantUpdate({ llama_model_quant: value })
                }
              />
            </div>
            <div className="space-y-1">
              <span className="text-xs font-medium text-mid-gray">
                {t("llamaCpp.localGemma.mmprojQuant")}
              </span>
              <Select
                value={
                  (brain.llama_mmproj_enabled ?? true)
                    ? (brain.llama_mmproj_quant ?? "F16")
                    : "disabled"
                }
                options={[
                  ...quantOptions(catalog?.mmproj ?? []),
                  {
                    value: "disabled",
                    label: t("llamaCpp.localGemma.disabledOption"),
                  },
                ]}
                isClearable={false}
                disabled={!(brain.llama_mmproj_enabled ?? true)}
                onChange={(value) => {
                  if (value === "disabled") {
                    quantUpdate({
                      llama_mmproj_enabled: false,
                      llama_mmproj_quant: "disabled",
                    });
                  } else if (value) {
                    quantUpdate({
                      llama_mmproj_enabled: true,
                      llama_mmproj_quant: value,
                    });
                  }
                }}
              />
            </div>
            <div className="space-y-1">
              <span className="text-xs font-medium text-mid-gray">
                {t("llamaCpp.localGemma.mtpQuant")}
              </span>
              <Select
                value={
                  (brain.llama_mtp_enabled ?? true)
                    ? (brain.llama_mtp_quant ?? "Q4_0")
                    : "disabled"
                }
                options={[
                  ...quantOptions(catalog?.mtp ?? []),
                  {
                    value: "disabled",
                    label: t("llamaCpp.localGemma.disabledOption"),
                  },
                ]}
                isClearable={false}
                disabled={!(brain.llama_mtp_enabled ?? true)}
                onChange={(value) => {
                  if (value === "disabled") {
                    quantUpdate({
                      llama_mtp_enabled: false,
                      llama_mtp_quant: "disabled",
                    });
                  } else if (value) {
                    quantUpdate({
                      llama_mtp_enabled: true,
                      llama_mtp_quant: value,
                    });
                  }
                }}
              />
            </div>
          </div>
        </div>
      )}

      {catalogError && (
        <p className="text-[11px] text-amber-400/80">
          {t("llamaCpp.localGemma.quantCatalogError")} {catalogError}
          <button
            className="ml-2 underline hover:text-amber-300"
            onClick={() => void refreshCatalog()}
          >
            {t("llamaCpp.localGemma.retry")}
          </button>
        </p>
      )}

      {llamaState.isDownloading ? (
        <div className="space-y-2">
          <div className="flex justify-between text-xs font-medium text-mid-gray">
            <span className="truncate max-w-[280px]">
              {llamaState.currentFile
                ? `Downloading ${llamaState.currentFile}...`
                : `Downloading ${modelDisplayName}...`}
            </span>
            <span className="flex gap-2">
              <span>
                {t("llamaCpp.downloadSpeed", {
                  speed: llamaState.downloadSpeed.toFixed(1),
                })}
              </span>
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
          {llamaState.isDownloaded
            ? t("llamaCpp.localGemma.redownloadButton")
            : t("llamaCpp.localGemma.downloadButton", {
                size: estimatedTotalSize,
              })}
        </Button>
      )}
    </div>
  );
};

const LlamaStatusCard: React.FC = () => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const variant = settings?.brain?.llama_model_variant ?? "standard";
  const quant = settings?.brain?.llama_model_quant ?? "Q2_K_XL";
  const mmprojEnabled =
    (settings?.brain?.llama_mmproj_enabled ?? true) &&
    settings?.brain?.llama_mmproj_quant !== "disabled";
  const mtpEnabled =
    (settings?.brain?.llama_mtp_enabled ?? true) &&
    settings?.brain?.llama_mtp_quant !== "disabled";

  const modelLabel =
    variant === "4b" || variant === "e4b"
      ? `Gemma 4 4B (UD-${quant})`
      : variant === "mobile"
        ? `Gemma 4 2B Mobile (UD-${quant})`
        : `Gemma 4 2B (UD-${quant})`;

  return (
    <div className="p-4 rounded-lg border border-green-500/10 bg-green-500/[0.02] backdrop-blur-sm grid grid-cols-2 gap-3 text-xs">
      <div className="col-span-2 border-b border-white/5 pb-2 mb-1 flex items-center justify-between">
        <span className="font-semibold text-text flex items-center gap-1.5">
          <span className="h-2 w-2 rounded-full bg-green-500 animate-pulse" />
          {t("llamaCpp.localGemma.title")}
        </span>
        <span className="text-[10px] px-2 py-0.5 bg-green-500/15 text-green-400 font-bold rounded">
          ACTIVE
        </span>
      </div>
      <div>
        <span className="text-mid-gray block">
          {t("llamaCpp.localGemma.status.model")}
        </span>
        <span className="font-medium text-text">{modelLabel}</span>
      </div>
      <div>
        <span className="text-mid-gray block">
          {t("llamaCpp.localGemma.status.mtpAcceleration")}
        </span>
        <span className="font-medium text-text">
          {mtpEnabled
            ? `${t("llamaCpp.localGemma.status.mtpEnabled")} (${settings?.brain?.llama_mtp_quant ?? "Q4_0"})`
            : t("llamaCpp.localGemma.disabledOption")}
        </span>
      </div>
      <div>
        <span className="text-mid-gray block">
          {t("llamaCpp.localGemma.status.visionComponent")}
        </span>
        <span className="font-medium text-text">
          {mmprojEnabled
            ? `Enabled (${settings?.brain?.llama_mmproj_quant ?? "F16"})`
            : t("llamaCpp.localGemma.status.visionDisabled")}
        </span>
      </div>
      <div>
        <span className="text-mid-gray block">
          {t("llamaCpp.localGemma.status.executionEngine")}
        </span>
        <span className="font-medium text-text">
          {t("llamaCpp.localGemma.status.executionEngineValue")}
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
            <LlamaDownloadPanel llamaState={llamaState} />

            <SettingContainer
              title={t("settings.postProcessing.api.baseUrl.title")}
              description={t("settings.postProcessing.api.baseUrl.description")}
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
              title={t("settings.brain.engineStatus.title")}
              description={t("settings.brain.engineStatus.description")}
              descriptionMode="tooltip"
              layout="stacked"
              grouped={true}
            >
              <LlamaStatusCard />
            </SettingContainer>
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
          title={t("settings.brain.warmupPrompt.label")}
          description={t("settings.brain.warmupPrompt.description")}
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
          checked={brain.compaction_enabled ?? true}
          onChange={(compaction_enabled) => update({ compaction_enabled })}
          label={t("settings.brain.compaction.label")}
          description={t("settings.brain.compaction.description")}
          grouped
        />
        <ToggleSwitch
          checked={brain.tools_enabled ?? false}
          onChange={(tools_enabled) => update({ tools_enabled })}
          label={t("settings.brain.tools.label")}
          description={t("settings.brain.tools.description")}
          grouped
        />
        <ToggleSwitch
          checked={brain.read_aloud}
          onChange={(read_aloud) => update({ read_aloud })}
          label={t("settings.brain.readAloud.label")}
          description={t("settings.brain.readAloud.description")}
          grouped
        />
        <SettingContainer
          title={t("settings.brain.endpointPreset.label")}
          description={t("settings.brain.endpointPreset.description")}
          descriptionMode="tooltip"
          layout="horizontal"
          grouped
        >
          <Select
            value={brain.endpoint_preset ?? "balanced"}
            options={[
              {
                value: "snappy",
                label: t("settings.brain.endpointPreset.snappy"),
              },
              {
                value: "balanced",
                label: t("settings.brain.endpointPreset.balanced"),
              },
              {
                value: "patient",
                label: t("settings.brain.endpointPreset.patient"),
              },
            ]}
            isClearable={false}
            onChange={(value) => value && update({ endpoint_preset: value })}
            className="min-w-[240px]"
          />
        </SettingContainer>
        <ToggleSwitch
          checked={brain.headphone_mode ?? false}
          onChange={(headphone_mode) => update({ headphone_mode })}
          label={t("settings.brain.headphoneMode.label")}
          description={t("settings.brain.headphoneMode.description")}
          grouped
        />
        <ToggleSwitch
          checked={brain.auto_listen ?? false}
          onChange={(auto_listen) => update({ auto_listen })}
          label={t("settings.brain.autoListen.label")}
          description={t("settings.brain.autoListen.description")}
          grouped
        />
        <SettingContainer
          title={t("settings.brain.replyLanguage.label")}
          description={t("settings.brain.replyLanguage.description")}
          descriptionMode="tooltip"
          layout="horizontal"
          grouped
        >
          <Select
            value={brain.reply_language ?? "auto"}
            options={[
              { value: "auto", label: t("settings.brain.replyLanguage.auto") },
              { value: "en", label: "English" },
              { value: "es", label: "Español" },
              { value: "fr", label: "Français" },
              { value: "de", label: "Deutsch" },
              { value: "it", label: "Italiano" },
              { value: "pt", label: "Português" },
              { value: "ru", label: "Русский" },
              { value: "ja", label: "日本語" },
              { value: "zh", label: "中文" },
            ]}
            isCreatable
            formatCreateLabel={(input) => `Use "${input}"`}
            onCreateOption={(value) => update({ reply_language: value })}
            onChange={(value) => value && update({ reply_language: value })}
            className="min-w-[240px]"
          />
        </SettingContainer>
        <SettingContainer
          title={t("settings.brain.speakableOutputPrompt.label")}
          description={t("settings.brain.speakableOutputPrompt.description")}
          grouped
          layout="stacked"
        >
          <Textarea
            variant="compact"
            rows={3}
            value={brain.speakable_output_prompt ?? ""}
            onChange={(e) =>
              update({ speakable_output_prompt: e.target.value })
            }
          />
        </SettingContainer>
      </SettingsGroup>

      {state.selectedProviderId === "llama_cpp" && (
        <SettingsGroup title={t("settings.brain.multimodal.group")}>
          <ToggleSwitch
            checked={brain.reasoning_enabled ?? false}
            onChange={(reasoning_enabled) => update({ reasoning_enabled })}
            label={t("settings.brain.reasoning.label")}
            description={t("settings.brain.reasoning.description")}
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
                      <span>
                        {t("conversation.metrics.tokensPerSec", {
                          tps: testMetrics.tokensPerSec.toFixed(1),
                        })}
                      </span>
                    )}
                    {testMetrics.totalMs != null && (
                      <span>
                        {t("conversation.metrics.totalMs", {
                          ms: testMetrics.totalMs,
                        })}
                      </span>
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
