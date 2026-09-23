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
  const { t } = useTranslation();
  const downloadedModels = () => props.models.filter((m) => m.is_downloaded);

  // The row that is both current and R2T2 gets the app's active box around its
  // name, matching the status bar's trigger (see `ModelStatusButton`). Keyed off
  // the latency kind the backend serialises, not the model id, so a renamed or
  // re-quantised R2T2 package still gets it.
  const isActiveChunkMs = (model: ModelInfo): boolean =>
    props.currentModelId === model.id &&
    model.native_streaming_latency_kind === "r2t2_chunk_ms";

  return (
    <div class="absolute bottom-full start-0 mb-2 w-64 max-h-[60vh] overflow-y-auto bg-background border border-mid-gray/20 rounded-lg shadow-lg py-2 z-50">
      {downloadedModels().length > 0 ? (
        <div>
          <For each={downloadedModels()}>
            {(model) => (
              <button
                type="button"
                onClick={() => props.onModelSelect(model.id)}
                class={`w-full px-3 py-2 text-start hover:bg-mid-gray/10 transition-colors cursor-pointer focus:outline-none border-0 bg-transparent block ${props.currentModelId === model.id ? "bg-accent/10 text-accent" : ""}`}
              >
                <div class="flex items-center justify-between">
                  <div>
                    <div
                      class={`text-sm text-text/80 ${
                        isActiveChunkMs(model)
                          ? "inline-block rounded border-2 border-accent bg-accent/10 px-1.5 py-0.5 text-accent"
                          : ""
                      }`}
                    >
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
                  {props.currentModelId === model.id && (
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
