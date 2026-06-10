import { useCallback, useMemo, useState } from "react";
import { useSettings } from "../../../hooks/useSettings";
import { commands, type PostProcessProvider } from "@/bindings";
import type { ModelOption } from "../PostProcessingSettingsApi/types";
import type { DropdownOption } from "../../ui/Dropdown";

type BrainProviderState = {
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

export const useBrainProviderState = (): BrainProviderState => {
  const {
    settings,
    isUpdating,
    setBrainProvider,
    updateBrainBaseUrl,
    updateBrainApiKey,
    updateBrainModel,
    fetchBrainModels,
    brainModelOptions,
  } = useSettings();

  const providers = settings?.brain?.providers || [];

  const selectedProviderId = useMemo(() => {
    return settings?.brain?.provider_id || providers[0]?.id || "custom";
  }, [providers, settings?.brain?.provider_id]);

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
  const apiKey = settings?.brain?.api_keys?.[selectedProviderId] ?? "";
  const model = settings?.brain?.models?.[selectedProviderId] ?? "";

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

      await setBrainProvider(providerId);

      if (providerId !== APPLE_PROVIDER_ID) {
        const provider = providers.find((p) => p.id === providerId);
        const key = settings?.brain?.api_keys?.[providerId] ?? "";
        const hasBaseUrl = (provider?.base_url ?? "").trim() !== "";
        const hasApiKey = key.trim() !== "";

        if (provider?.id === "custom" ? hasBaseUrl : hasApiKey) {
          void fetchBrainModels(providerId);
        }
      }
    },
    [
      selectedProviderId,
      setBrainProvider,
      fetchBrainModels,
      providers,
      settings,
    ],
  );

  const handleBaseUrlChange = useCallback(
    (value: string) => {
      if (!selectedProvider || selectedProvider.id !== "custom") {
        return;
      }
      const trimmed = value.trim();
      if (trimmed && trimmed !== baseUrl) {
        void updateBrainBaseUrl(selectedProvider.id, trimmed);
      }
    },
    [selectedProvider, baseUrl, updateBrainBaseUrl],
  );

  const handleApiKeyChange = useCallback(
    (value: string) => {
      const trimmed = value.trim();
      if (trimmed !== apiKey) {
        void updateBrainApiKey(selectedProviderId, trimmed);
      }
    },
    [apiKey, selectedProviderId, updateBrainApiKey],
  );

  const handleModelChange = useCallback(
    (value: string) => {
      const trimmed = value.trim();
      if (trimmed !== model) {
        void updateBrainModel(selectedProviderId, trimmed);
      }
    },
    [model, selectedProviderId, updateBrainModel],
  );

  const handleModelSelect = useCallback(
    (value: string) => {
      void updateBrainModel(selectedProviderId, value.trim());
    },
    [selectedProviderId, updateBrainModel],
  );

  const handleModelCreate = useCallback(
    (value: string) => {
      void updateBrainModel(selectedProviderId, value);
    },
    [selectedProviderId, updateBrainModel],
  );

  const handleRefreshModels = useCallback(() => {
    if (isAppleProvider) return;
    void fetchBrainModels(selectedProviderId);
  }, [fetchBrainModels, isAppleProvider, selectedProviderId]);

  const availableModelsRaw = brainModelOptions[selectedProviderId] || [];

  const modelOptions = useMemo<ModelOption[]>(() => {
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
    `brain_base_url:${selectedProviderId}`,
  );
  const isApiKeyUpdating = isUpdating(
    `brain_api_key:${selectedProviderId}`,
  );
  const isModelUpdating = isUpdating(
    `brain_model:${selectedProviderId}`,
  );
  const isFetchingModels = isUpdating(
    `brain_models_fetch:${selectedProviderId}`,
  );

  const isCustomProvider = selectedProvider?.id === "custom";

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
    modelOptions,
    isModelUpdating,
    isFetchingModels,
    handleProviderSelect,
    handleModelSelect,
    handleModelCreate,
    handleRefreshModels,
  };
};
