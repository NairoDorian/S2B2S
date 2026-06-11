import React, { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
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

export const BrainSettings: React.FC = () => {
  const { t } = useTranslation();
  const { settings, updateSetting, isUpdating } = useSettings();
  const state = useBrainProviderState();
  const [testState, setTestState] = useState<
    "idle" | "running" | "ok" | "error"
  >("idle");
  const [testReply, setTestReply] = useState("");

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
    const result = await commands.brainAsk(t("settings.brain.test.prompt"));
    if (result.status === "ok") {
      setTestReply(result.data);
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

        {state.isAppleProvider ? (
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
          </>
        )}

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
                ariaLabel={t("settings.postProcessing.api.model.refreshModels")}
                className="flex h-10 w-10 items-center justify-center"
              >
                <RefreshCcw
                  className={`h-4 w-4 ${state.isFetchingModels ? "animate-spin" : ""}`}
                />
              </ResetButton>
            </div>
          </SettingContainer>
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
              <p
                className={`text-sm whitespace-pre-wrap ${
                  testState === "error" ? "text-red-500" : "text-mid-gray"
                }`}
              >
                {testReply}
              </p>
            )}
          </div>
        </SettingContainer>
      </SettingsGroup>
    </div>
  );
};
