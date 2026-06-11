import { useSettings } from "../../../hooks/useSettings";
import {
  useProviderState,
  type ProviderState,
} from "../../../hooks/useProviderState";

export type { ProviderState as BrainProviderState };

export const useBrainProviderState = (): ProviderState => {
  const {
    setBrainProvider,
    updateBrainBaseUrl,
    updateBrainApiKey,
    updateBrainModel,
    fetchBrainModels,
    brainModelOptions,
  } = useSettings();

  return useProviderState({
    settingsKey: "brain",
    setProvider: setBrainProvider,
    updateBaseUrl: updateBrainBaseUrl,
    updateApiKey: updateBrainApiKey,
    updateModel: updateBrainModel,
    fetchModels: fetchBrainModels,
    modelOptions: brainModelOptions,
  });
};
