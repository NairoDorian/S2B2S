import { useCallback, useMemo, useState } from "react";
import { useSettings } from "./useSettings";
import { commands, type PostProcessProvider } from "@/bindings";
import type { ModelOption } from "../components/settings/PostProcessingSettingsApi/types";
import type { DropdownOption } from "../components/ui/Dropdown";

export type ProviderConfig = {
  settingsKey: "brain" | "post_process";
  setProvider: (providerId: string) => Promise<void>;
  updateBaseUrl: (providerId: string, baseUrl: string) => Promise<void>;
  updateApiKey: (providerId: string, apiKey: string) => Promise<void>;
  updateModel: (providerId: string, model: string) => Promise<void>;
  fetchModels: (providerId: string) => Promise<string[]>;
  modelOptions: Record<string, string[]>;
};

export type ProviderState = {
  providerOptions: DropdownOption[];
  selectedProviderId: string;
  selectedProvider: PostProcessProvider | undefined;
  isCustomProvider: boolean;
  isAppleProvider: boolean;
  appleIntelligenceUnavailable: boolean;
  baseUrl: string;
  handleBaseUrlChange: (value: string) => void;
  isBaseUrlUpdating: boolean;
  apiKey: string;
  handleApiKeyChange: (value: string) => void;
  isApiKeyUpdating: boolean;
  model: string;
  handleModelChange: (value: string) => void;
  modelOptions: ModelOption[];
  isModelUpdating: boolean;
  isFetchingModels: boolean;
  handleProviderSelect: (providerId: string) => void;
  handleModelSelect: (value: string) => void;
  handleModelCreate: (value: string) => void;
  handleRefreshModels: () => void;
};

const APPLE_PROVIDER_ID = "apple_intelligence";

export const useProviderState = (config: ProviderConfig): ProviderState => {
  const { settings, isUpdating } = useSettings();
  const prefix = config.settingsKey;

  const providers =
    prefix === "brain"
      ? settings?.brain?.providers || []
      : settings?.post_process_providers || [];

  const selectedProviderId = useMemo(() => {
    if (prefix === "brain") {
      return settings?.brain?.provider_id || providers[0]?.id || "custom";
    }
    return settings?.post_process_provider_id || providers[0]?.id || "openai";
  }, [providers, settings, prefix]);

  const selectedProvider = useMemo(() => {
    return (
      providers.find((provider) => provider.id === selectedProviderId) ||
      providers[0]
    );
  }, [providers, selectedProviderId]);

  const isAppleProvider = selectedProvider?.id === APPLE_PROVIDER_ID;
  const [appleIntelligenceUnavailable, setAppleIntelligenceUnavailable] =
    useState(false);

  const baseUrl = selectedProvider?.base_url ?? "";
  const apiKey =
    prefix === "brain"
      ? (settings?.brain?.api_keys?.[selectedProviderId] ?? "")
      : (settings?.post_process_api_keys?.[selectedProviderId] ?? "");
  const model =
    prefix === "brain"
      ? (settings?.brain?.models?.[selectedProviderId] ?? "")
      : (settings?.post_process_models?.[selectedProviderId] ?? "");

  const providerOptions = useMemo<DropdownOption[]>(() => {
    return providers.map((provider) => ({
      value: provider.id,
      label: provider.label,
    }));
  }, [providers]);

  const handleProviderSelect = useCallback(
    async (providerId: string) => {
      setAppleIntelligenceUnavailable(false);

      if (providerId === selectedProviderId) return;

      if (providerId === APPLE_PROVIDER_ID) {
        const available = await commands.checkAppleIntelligenceAvailable();
        if (!available) {
          setAppleIntelligenceUnavailable(true);
        }
      }

      await config.setProvider(providerId);

      if (providerId !== APPLE_PROVIDER_ID) {
        const provider = providers.find((p) => p.id === providerId);
        const key =
          prefix === "brain"
            ? (settings?.brain?.api_keys?.[providerId] ?? "")
            : (settings?.post_process_api_keys?.[providerId] ?? "");
        const hasBaseUrl = (provider?.base_url ?? "").trim() !== "";
        const hasApiKey = key.trim() !== "";

        if (
          provider?.id === "custom" || provider?.id === "llama_cpp"
            ? hasBaseUrl
            : hasApiKey
        ) {
          void config.fetchModels(providerId);
        }
      }
    },
    [selectedProviderId, config, providers, settings, prefix],
  );

  const handleBaseUrlChange = useCallback(
    (value: string) => {
      if (
        !selectedProvider ||
        (selectedProvider.id !== "custom" &&
          selectedProvider.id !== "llama_cpp")
      )
        return;
      const trimmed = value.trim();
      if (trimmed && trimmed !== baseUrl) {
        void config.updateBaseUrl(selectedProvider.id, trimmed);
      }
    },
    [selectedProvider, baseUrl, config],
  );

  const handleApiKeyChange = useCallback(
    (value: string) => {
      const trimmed = value.trim();
      if (trimmed !== apiKey) {
        void config.updateApiKey(selectedProviderId, trimmed);
      }
    },
    [apiKey, selectedProviderId, config],
  );

  const handleModelChange = useCallback(
    (value: string) => {
      const trimmed = value.trim();
      if (trimmed !== model) {
        void config.updateModel(selectedProviderId, trimmed);
      }
    },
    [model, selectedProviderId, config],
  );

  const handleModelSelect = useCallback(
    (value: string) => {
      void config.updateModel(selectedProviderId, value.trim());
    },
    [selectedProviderId, config],
  );

  const handleModelCreate = useCallback(
    (value: string) => {
      void config.updateModel(selectedProviderId, value);
    },
    [selectedProviderId, config],
  );

  const handleRefreshModels = useCallback(() => {
    if (isAppleProvider) return;
    void config.fetchModels(selectedProviderId);
  }, [config, isAppleProvider, selectedProviderId]);

  const availableModelsRaw = config.modelOptions[selectedProviderId] || [];

  const modelOptionsResult = useMemo<ModelOption[]>(() => {
    const seen = new Set<string>();
    const options: ModelOption[] = [];

    const upsert = (value: string | null | undefined) => {
      const trimmed = value?.trim();
      if (!trimmed || seen.has(trimmed)) return;
      seen.add(trimmed);
      options.push({ value: trimmed, label: trimmed });
    };

    for (const candidate of availableModelsRaw) {
      upsert(candidate);
    }

    upsert(model);

    return options;
  }, [availableModelsRaw, model]);

  const isBaseUrlUpdating = isUpdating(
    `${prefix}_base_url:${selectedProviderId}`,
  );
  const isApiKeyUpdating = isUpdating(
    `${prefix}_api_key:${selectedProviderId}`,
  );
  const isModelUpdating = isUpdating(`${prefix}_model:${selectedProviderId}`);
  const isFetchingModels = isUpdating(
    `${prefix}_models_fetch:${selectedProviderId}`,
  );

  const isCustomProvider = selectedProvider?.allow_base_url_edit ?? false;

  return {
    providerOptions,
    selectedProviderId,
    selectedProvider,
    isCustomProvider,
    isAppleProvider,
    appleIntelligenceUnavailable,
    baseUrl,
    handleBaseUrlChange,
    isBaseUrlUpdating,
    apiKey,
    handleApiKeyChange,
    isApiKeyUpdating,
    model,
    handleModelChange,
    modelOptions: modelOptionsResult,
    isModelUpdating,
    isFetchingModels,
    handleProviderSelect,
    handleModelSelect,
    handleModelCreate,
    handleRefreshModels,
  };
};
