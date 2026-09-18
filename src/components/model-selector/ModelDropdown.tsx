import { For } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import type { ModelInfo } from "@/bindings";
import {
  getTranslatedModelName,
  getTranslatedModelDescription,
} from "../../lib/utils/modelTranslation";
import type { JSX } from "@solidjs/web";

interface ModelDropdownProps {
  models: ModelInfo[];
  currentModelId: string;
  onModelSelect: (modelId: string) => void;
}

const ModelDropdown = (props: ModelDropdownProps): JSX.Element => {
  const { models, currentModelId, onModelSelect } = props;
  const { t } = useTranslation();
  const downloadedModels = models.filter((m) => m.is_downloaded);

  const handleModelClick = (modelId: string) => {
    onModelSelect(modelId);
  };

  return (
    <div class="absolute bottom-full start-0 mb-2 w-64 max-h-[60vh] overflow-y-auto bg-background border border-mid-gray/20 rounded-lg shadow-lg py-2 z-50">
      {downloadedModels.length > 0 ? (
        <div>
          <For each={downloadedModels}>
            {(model) => (
              <button
                type="button"
                onClick={() => handleModelClick(model.id)}
                class={`w-full px-3 py-2 text-start hover:bg-mid-gray/10 transition-colors cursor-pointer focus:outline-none border-0 bg-transparent block ${currentModelId === model.id ? "bg-accent/10 text-accent" : ""}`}
              >
                <div class="flex items-center justify-between">
                  <div>
                    <div class="text-sm text-text/80">
                      {getTranslatedModelName(model, t)}
                      {model.is_custom && (
                        <span class="ms-1.5 text-[10px] font-medium text-text/40 uppercase">
                          {t("modelSelector.custom")}
                        </span>
                      )}
                      {model.supports_streaming && (
                        <span class="ms-1.5 text-[10px] font-medium text-accent/70 uppercase">
                          {t("modelSelector.streaming")}
                        </span>
                      )}
                    </div>
                    <div class="text-xs text-text/40 italic pe-4">
                      {getTranslatedModelDescription(model, t)}
                    </div>
                  </div>
                  {currentModelId === model.id && (
                    <div class="text-xs text-accent">
                      {t("modelSelector.active")}
                    </div>
                  )}
                </div>
              </button>
            )}
          </For>
        </div>
      ) : (
        <div class="px-3 py-2 text-sm text-text/60">
          {t("modelSelector.noModelsAvailable")}
        </div>
      )}
    </div>
  );
};

export default ModelDropdown;
