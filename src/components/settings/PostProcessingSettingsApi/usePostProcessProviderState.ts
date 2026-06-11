import { useSettings } from "../../../hooks/useSettings";
import {
  useProviderState,
  type ProviderState,
} from "../../../hooks/useProviderState";

export type { ProviderState as PostProcessProviderState };

export const usePostProcessProviderState = (): ProviderState => {
  const {
    setPostProcessProvider,
    updatePostProcessBaseUrl,
    updatePostProcessApiKey,
    updatePostProcessModel,
    fetchPostProcessModels,
    postProcessModelOptions,
  } = useSettings();

  return useProviderState({
    settingsKey: "post_process",
    setProvider: setPostProcessProvider,
    updateBaseUrl: updatePostProcessBaseUrl,
    updateApiKey: updatePostProcessApiKey,
    updateModel: updatePostProcessModel,
    fetchModels: fetchPostProcessModels,
    modelOptions: postProcessModelOptions,
  });
};
