import { createSignal, createEffect } from "solid-js";
import { useTranslation, TranslatedMarkup } from "@/i18n/useTranslation";
import { Alert } from "../../ui/Alert";
import {
  Dropdown,
  SettingContainer,
  SettingsGroup,
  Textarea,
} from "@/components/ui";
import { Button } from "../../ui/Button";
import { ResetButton } from "../../ui/ResetButton";
import { Input } from "../../ui/Input";
import { RefreshCcw } from "@/components/icons/lucide";
import { commands } from "@/bindings";
import { sessionToast as toast } from "@/lib/sessionToast";
import { ProviderSelect } from "../PostProcessingSettingsApi/ProviderSelect";
import { BaseUrlField } from "../PostProcessingSettingsApi/BaseUrlField";
import { ApiKeyField } from "../PostProcessingSettingsApi/ApiKeyField";
import { ModelSelect } from "../PostProcessingSettingsApi/ModelSelect";
import { usePostProcessProviderState } from "../PostProcessingSettingsApi/usePostProcessProviderState";
import { ShortcutInput } from "../ShortcutInput";
import { useSettings } from "../../../hooks/useSettings";
import type { JSX } from "@solidjs/web";

const PostProcessingSettingsApiComponent = (): JSX.Element => {
  const { t } = useTranslation();
  const state = usePostProcessProviderState();

  return (
    <>
      <SettingContainer
        title={t("settings.postProcessing.api.provider.title")}
        description={t("settings.postProcessing.api.provider.description")}
        descriptionMode="tooltip"
        layout="horizontal"
        grouped={true}
      >
        <div class="flex items-center gap-2">
          <ProviderSelect
            options={state.providerOptions()}
            value={state.selectedProviderId()}
            onChange={state.handleProviderSelect}
          />
        </div>
      </SettingContainer>

      {state.isAppleProvider() ? (
        state.appleIntelligenceUnavailable() ? (
          <Alert variant="error" contained>
            {t("settings.postProcessing.api.appleIntelligence.unavailable")}
          </Alert>
        ) : null
      ) : (
        <>
          {state.isCustomProvider() && (
            <SettingContainer
              title={t("settings.postProcessing.api.baseUrl.title")}
              description={t("settings.postProcessing.api.baseUrl.description")}
              descriptionMode="tooltip"
              layout="horizontal"
              grouped={true}
            >
              <div class="flex items-center gap-2">
                <BaseUrlField
                  value={state.baseUrl()}
                  onBlur={state.handleBaseUrlChange}
                  placeholder={t(
                    "settings.postProcessing.api.baseUrl.placeholder",
                  )}
                  disabled={state.isBaseUrlUpdating()}
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
            <div class="flex items-center gap-2">
              <ApiKeyField
                value={state.apiKey()}
                onBlur={state.handleApiKeyChange}
                placeholder={t(
                  "settings.postProcessing.api.apiKey.placeholder",
                )}
                disabled={state.isApiKeyUpdating()}
                className="min-w-[320px]"
              />
            </div>
          </SettingContainer>
        </>
      )}

      {!state.isAppleProvider() && (
        <SettingContainer
          title={t("settings.postProcessing.api.model.title")}
          description={
            state.isCustomProvider()
              ? t("settings.postProcessing.api.model.descriptionCustom")
              : t("settings.postProcessing.api.model.descriptionDefault")
          }
          descriptionMode="tooltip"
          layout="stacked"
          grouped={true}
        >
          <div class="flex items-center gap-2">
            <ModelSelect
              value={state.model()}
              options={state.modelOptions()}
              disabled={state.isModelUpdating()}
              isLoading={state.isFetchingModels()}
              placeholder={
                state.modelOptions().length > 0
                  ? t(
                      "settings.postProcessing.api.model.placeholderWithOptions",
                    )
                  : t("settings.postProcessing.api.model.placeholderNoOptions")
              }
              onSelect={state.handleModelSelect}
              onCreate={state.handleModelCreate}
              className="flex-1 min-w-[380px]"
            />
            <ResetButton
              onClick={state.handleRefreshModels}
              disabled={state.isFetchingModels()}
              ariaLabel={t("settings.postProcessing.api.model.refreshModels")}
              class="flex h-10 w-10 items-center justify-center"
            >
              <RefreshCcw
                class={`h-4 w-4 ${state.isFetchingModels() ? "animate-spin" : ""}`}
              />
            </ResetButton>
          </div>
        </SettingContainer>
      )}
    </>
  );
};

const PostProcessingSettingsPromptsComponent = (): JSX.Element => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating, refreshSettings } =
    useSettings();
  const [isCreating, setIsCreating] = createSignal(false);
  const [draftName, setDraftName] = createSignal("");
  const [draftText, setDraftText] = createSignal("");

  const prompts = () => getSetting("post_process_prompts") || [];
  const selectedPromptId = () =>
    getSetting("post_process_selected_prompt_id") || "";
  const selectedPrompt = () =>
    prompts().find((prompt) => prompt.id === selectedPromptId()) || null;

  createEffect(
    () => selectedPrompt(),
    (prompt) => {
      if (isCreating()) return;

      if (prompt) {
        setDraftName(prompt.name);
        setDraftText(prompt.prompt);
      } else {
        setDraftName("");
        setDraftText("");
      }
    },
  );

  const handlePromptSelect = (promptId: string | null) => {
    if (!promptId) return;
    updateSetting("post_process_selected_prompt_id", promptId);
    setIsCreating(false);
  };

  const handleCreatePrompt = async () => {
    if (!draftName().trim() || !draftText().trim()) return;

    try {
      const result = await commands.addPostProcessPrompt(
        draftName().trim(),
        draftText().trim(),
      );
      if (result.status === "error") {
        console.error("Failed to create prompt:", result.error);
        toast.error(result.error);
        return;
      }
      await refreshSettings();
      updateSetting("post_process_selected_prompt_id", result.data.id);
      setIsCreating(false);
    } catch (error) {
      console.error("Failed to create prompt:", error);
    }
  };

  const handleUpdatePrompt = async () => {
    if (!selectedPromptId() || !draftName().trim() || !draftText().trim())
      return;

    // A backend `Err` is returned as `{ status: "error" }`, not thrown.
    try {
      const result = await commands.updatePostProcessPrompt(
        selectedPromptId(),
        draftName().trim(),
        draftText().trim(),
      );
      if (result.status === "error") {
        console.error("Failed to update prompt:", result.error);
        toast.error(result.error);
        return;
      }
      await refreshSettings();
    } catch (error) {
      console.error("Failed to update prompt:", error);
    }
  };

  const handleDeletePrompt = async (promptId: string) => {
    if (!promptId) return;

    try {
      const result = await commands.deletePostProcessPrompt(promptId);
      if (result.status === "error") {
        console.error("Failed to delete prompt:", result.error);
        toast.error(result.error);
        return;
      }
      await refreshSettings();
      setIsCreating(false);
    } catch (error) {
      console.error("Failed to delete prompt:", error);
    }
  };

  const handleCancelCreate = () => {
    setIsCreating(false);
    const prompt = selectedPrompt();
    if (prompt) {
      setDraftName(prompt.name);
      setDraftText(prompt.prompt);
    } else {
      setDraftName("");
      setDraftText("");
    }
  };

  const handleStartCreate = () => {
    setIsCreating(true);
    setDraftName("");
    setDraftText("");
  };

  const hasPrompts = () => prompts().length > 0;
  const isDirty = () => {
    const prompt = selectedPrompt();
    return (
      !!prompt &&
      (draftName().trim() !== prompt.name ||
        draftText().trim() !== prompt.prompt.trim())
    );
  };

  return (
    <SettingContainer
      title={t("settings.postProcessing.prompts.selectedPrompt.title")}
      description={t(
        "settings.postProcessing.prompts.selectedPrompt.description",
      )}
      descriptionMode="tooltip"
      layout="stacked"
      grouped={true}
    >
      <div class="space-y-3">
        <div class="flex gap-2 min-w-0">
          <Dropdown
            selectedValue={selectedPromptId() || null}
            options={prompts().map((p) => ({
              value: p.id,
              label: p.name,
            }))}
            onSelect={(value) => handlePromptSelect(value)}
            placeholder={
              prompts().length === 0
                ? t("settings.postProcessing.prompts.noPrompts")
                : t("settings.postProcessing.prompts.selectPrompt")
            }
            disabled={
              isUpdating("post_process_selected_prompt_id") || isCreating()
            }
            class="flex-1 min-w-0"
          />
          <Button
            onClick={handleStartCreate}
            variant="primary"
            size="md"
            disabled={isCreating()}
            class="shrink-0"
          >
            {t("settings.postProcessing.prompts.createNew")}
          </Button>
        </div>

        {!isCreating() && hasPrompts() && selectedPrompt() && (
          <div class="space-y-3">
            <div class="space-y-2 flex flex-col">
              <label class="text-sm font-semibold">
                {t("settings.postProcessing.prompts.promptLabel")}
              </label>
              <Input
                type="text"
                value={draftName()}
                onInput={(e) => setDraftName(e.target.value)}
                placeholder={t(
                  "settings.postProcessing.prompts.promptLabelPlaceholder",
                )}
                variant="compact"
              />
            </div>

            <div class="space-y-2 flex flex-col">
              <label class="text-sm font-semibold">
                {t("settings.postProcessing.prompts.promptInstructions")}
              </label>
              <Textarea
                value={draftText()}
                onInput={(e) => setDraftText(e.target.value)}
                placeholder={t(
                  "settings.postProcessing.prompts.promptInstructionsPlaceholder",
                )}
              />
              <p class="text-xs text-mid-gray/70">
                <TranslatedMarkup
                  text={t("settings.postProcessing.prompts.promptTip")}
                />
              </p>
            </div>

            <div class="flex gap-2 pt-2">
              <Button
                onClick={handleUpdatePrompt}
                variant="primary"
                size="md"
                disabled={
                  !draftName().trim() || !draftText().trim() || !isDirty()
                }
              >
                {t("settings.postProcessing.prompts.updatePrompt")}
              </Button>
              <Button
                onClick={() => handleDeletePrompt(selectedPromptId())}
                variant="secondary"
                size="md"
                disabled={!selectedPromptId() || prompts().length <= 1}
              >
                {t("settings.postProcessing.prompts.deletePrompt")}
              </Button>
            </div>
          </div>
        )}

        {!isCreating() && !selectedPrompt() && (
          <div class="p-3 bg-mid-gray/5 rounded-md border border-mid-gray/20">
            <p class="text-sm text-mid-gray">
              {hasPrompts()
                ? t("settings.postProcessing.prompts.selectToEdit")
                : t("settings.postProcessing.prompts.createFirst")}
            </p>
          </div>
        )}

        {isCreating() && (
          <div class="space-y-3">
            <div class="space-y-2 flex flex-col">
              <label class="text-sm font-semibold text-text">
                {t("settings.postProcessing.prompts.promptLabel")}
              </label>
              <Input
                type="text"
                value={draftName()}
                onInput={(e) => setDraftName(e.target.value)}
                placeholder={t(
                  "settings.postProcessing.prompts.promptLabelPlaceholder",
                )}
                variant="compact"
              />
            </div>

            <div class="space-y-2 flex flex-col">
              <label class="text-sm font-semibold">
                {t("settings.postProcessing.prompts.promptInstructions")}
              </label>
              <Textarea
                value={draftText()}
                onInput={(e) => setDraftText(e.target.value)}
                placeholder={t(
                  "settings.postProcessing.prompts.promptInstructionsPlaceholder",
                )}
              />
              <p class="text-xs text-mid-gray/70">
                <TranslatedMarkup
                  text={t("settings.postProcessing.prompts.promptTip")}
                />
              </p>
            </div>

            <div class="flex gap-2 pt-2">
              <Button
                onClick={handleCreatePrompt}
                variant="primary"
                size="md"
                disabled={!draftName().trim() || !draftText().trim()}
              >
                {t("settings.postProcessing.prompts.createPrompt")}
              </Button>
              <Button
                onClick={handleCancelCreate}
                variant="secondary"
                size="md"
              >
                {t("settings.postProcessing.prompts.cancel")}
              </Button>
            </div>
          </div>
        )}
      </div>
    </SettingContainer>
  );
};

export const PostProcessingSettings = (): JSX.Element => {
  const { t } = useTranslation();

  return (
    <div class="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title={t("settings.postProcessing.hotkey.title")}>
        <ShortcutInput
          shortcutId="transcribe_with_post_process"
          descriptionMode="tooltip"
          grouped={true}
        />
      </SettingsGroup>

      <SettingsGroup title={t("settings.postProcessing.api.title")}>
        <PostProcessingSettingsApiComponent />
      </SettingsGroup>

      <SettingsGroup title={t("settings.postProcessing.prompts.title")}>
        <PostProcessingSettingsPromptsComponent />
      </SettingsGroup>
    </div>
  );
};
