import { createSignal, createMemo, type Accessor } from "solid-js";
import { useSettings } from "../../../hooks/useSettings";
import { commands, type PostProcessProvider } from "@/bindings";
import type { ModelOption } from "./types";
import type { DropdownOption } from "../../ui/Dropdown";

type PostProcessProviderState = {
  providerOptions: Accessor<DropdownOption[]>;
  selectedProviderId: Accessor<string>;
  selectedProvider: Accessor<PostProcessProvider | undefined>;
  isCustomProvider: Accessor<boolean>;
  isAppleProvider: Accessor<boolean>;
  appleIntelligenceUnavailable: Accessor<boolean>;
  baseUrl: Accessor<string>;
  handleBaseUrlChange: (value: string) => void;
  isBaseUrlUpdating: Accessor<boolean>;
  apiKey: Accessor<string>;
  handleApiKeyChange: (value: string) => void;
  isApiKeyUpdating: Accessor<boolean>;
  model: Accessor<string>;
  handleModelChange: (value: string) => void;
  modelOptions: Accessor<ModelOption[]>;
  isModelUpdating: Accessor<boolean>;
  isFetchingModels: Accessor<boolean>;
  handleProviderSelect: (providerId: string) => void;
  handleModelSelect: (value: string) => void;
  handleModelCreate: (value: string) => void;
  handleRefreshModels: () => void;
};

const APPLE_PROVIDER_ID = "apple_intelligence";

export const usePostProcessProviderState = (): PostProcessProviderState => {
  const {
    settings,
    isUpdating,
    setPostProcessProvider,
    updatePostProcessBaseUrl,
    updatePostProcessApiKey,
    updatePostProcessModel,
    fetchPostProcessModels,
    postProcessModelOptions,
  } = useSettings();

  const providers = createMemo(() => settings()?.post_process_providers ?? []);

  const selectedProviderId = createMemo(() => {
    return (
      settings()?.post_process_provider_id || providers()[0]?.id || "openai"
    );
  });

  const selectedProvider = createMemo(() => {
    return (
      providers().find((provider) => provider.id === selectedProviderId()) ||
      providers()[0]
    );
  });

  const isAppleProvider = () => selectedProvider()?.id === APPLE_PROVIDER_ID;
  const [appleIntelligenceUnavailable, setAppleIntelligenceUnavailable] =
    createSignal(false);

  const baseUrl = () => selectedProvider()?.base_url ?? "";
  const apiKey = () =>
    settings()?.post_process_api_keys?.[selectedProviderId()] ?? "";
  const model = () =>
    settings()?.post_process_models?.[selectedProviderId()] ?? "";

  const providerOptions = createMemo<DropdownOption[]>(() => {
    return providers().map((provider) => ({
      value: provider.id,
      label: provider.label,
    }));
  });

  const handleProviderSelect = async (providerId: string) => {
    setAppleIntelligenceUnavailable(false);

    if (providerId === selectedProviderId()) return;

    if (providerId === APPLE_PROVIDER_ID) {
      const available = await commands.checkAppleIntelligenceAvailable();
      if (!available) {
        setAppleIntelligenceUnavailable(true);
      }
    }

    await setPostProcessProvider(providerId);

    if (providerId !== APPLE_PROVIDER_ID) {
      const provider = providers().find((p) => p.id === providerId);
      const key = settings()?.post_process_api_keys?.[providerId] ?? "";
      const hasBaseUrl = (provider?.base_url ?? "").trim() !== "";
      const hasApiKey = key.trim() !== "";

      if (provider?.id === "custom" ? hasBaseUrl : hasApiKey) {
        void fetchPostProcessModels(providerId);
      }
    }
  };

  const handleBaseUrlChange = (value: string) => {
    const provider = selectedProvider();
    if (!provider || provider.id !== "custom") {
      return;
    }
    const trimmed = value.trim();
    if (trimmed && trimmed !== baseUrl()) {
      void updatePostProcessBaseUrl(provider.id, trimmed);
    }
  };

  const handleApiKeyChange = (value: string) => {
    const trimmed = value.trim();
    if (trimmed !== apiKey()) {
      void updatePostProcessApiKey(selectedProviderId(), trimmed);
    }
  };

  const handleModelChange = (value: string) => {
    const trimmed = value.trim();
    if (trimmed !== model()) {
      void updatePostProcessModel(selectedProviderId(), trimmed);
    }
  };

  const handleModelSelect = (value: string) => {
    void updatePostProcessModel(selectedProviderId(), value.trim());
  };

  const handleModelCreate = (value: string) => {
    void updatePostProcessModel(selectedProviderId(), value);
  };

  const handleRefreshModels = () => {
    if (isAppleProvider()) return;
    void fetchPostProcessModels(selectedProviderId());
  };

  const modelOptions = createMemo<ModelOption[]>(() => {
    const seen = new Set<string>();
    const options: ModelOption[] = [];

    const upsert = (value: string | null | undefined) => {
      const trimmed = value?.trim();
      if (!trimmed || seen.has(trimmed)) return;
      seen.add(trimmed);
      options.push({ value: trimmed, label: trimmed });
    };

    for (const candidate of postProcessModelOptions()[selectedProviderId()] ??
      []) {
      upsert(candidate);
    }

    upsert(model());

    return options;
  });

  const isBaseUrlUpdating = () =>
    isUpdating(`post_process_base_url:${selectedProviderId()}`);
  const isApiKeyUpdating = () =>
    isUpdating(`post_process_api_key:${selectedProviderId()}`);
  const isModelUpdating = () =>
    isUpdating(`post_process_model:${selectedProviderId()}`);
  const isFetchingModels = () =>
    isUpdating(`post_process_models_fetch:${selectedProviderId()}`);

  const isCustomProvider = () => selectedProvider()?.id === "custom";

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
