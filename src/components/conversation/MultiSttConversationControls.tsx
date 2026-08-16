import React, { useCallback, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { ListChecks } from "lucide-react";
import { Dropdown } from "../ui/Dropdown";
import { commands, type ModelInfo } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { useModelStore } from "../../stores/modelStore";

/**
 * Compact multi-STT controls embedded in the Conversation view.
 *
 * Lets the user run a 2nd/3rd STT model (plus optional Gemma 4 STT with
 * mmproj audio input) in conversation mode without leaving the view.
 * The extra models are preloaded into RAM/VRAM and the local llama.cpp
 * server is warmed with mmproj whenever multimodal fusion is enabled.
 */
export const MultiSttConversationControls: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const { models, currentModel } = useModelStore();

  const enabled = (getSetting("multi_stt_enabled") as boolean) ?? false;
  const model2 = (getSetting("multi_stt_model_2") as string | null) ?? null;
  const model3 = (getSetting("multi_stt_model_3") as string | null) ?? null;
  const brainMode =
    (getSetting("multi_stt_brain_mode") as string | null) ?? "text_only";

  const downloadedModels = models.filter(
    (m: ModelInfo) => m.is_downloaded || m.is_custom,
  );

  // Same slot logic as the Multi-STT settings panel: each slot keeps its own
  // selection visible and excludes the primary model and the other slot.
  const modelOptionsForSlot2 = downloadedModels
    .filter((m: ModelInfo) => m.id !== currentModel && m.id !== model3)
    .map((m: ModelInfo) => ({ value: m.id, label: m.name }));
  const modelOptionsForSlot3 = downloadedModels
    .filter((m: ModelInfo) => m.id !== currentModel && m.id !== model2)
    .map((m: ModelInfo) => ({ value: m.id, label: m.name }));

  const noneOption = { value: "", label: t("multiStt.models.notSelected") };
  const slot2Options = [noneOption, ...modelOptionsForSlot2];
  const slot3Options = [noneOption, ...modelOptionsForSlot3];

  const preloadExtras = useCallback(() => {
    void commands
      .preloadMultiSttModels()
      .catch((err) =>
        console.error("Failed to preload multi-STT models:", err),
      );
  }, []);

  const unloadExtras = useCallback(() => {
    void commands
      .unloadAllExtraModels()
      .catch((err) => console.error("Failed to unload multi-STT models:", err));
  }, []);

  const warmBrainWithMmproj = useCallback((mmproj: boolean) => {
    void commands
      .warmBrainServer(mmproj)
      .catch((err) => console.error("Failed to warm brain server:", err));
  }, []);

  const handleToggleEnabled = async (value: boolean) => {
    await updateSetting("multi_stt_enabled", value);
    if (value) {
      preloadExtras();
      const needsMmproj = brainMode !== "text_only";
      if (needsMmproj) {
        warmBrainWithMmproj(true);
      }
    } else {
      unloadExtras();
    }
  };

  const handleModelSelect = async (slot: 2 | 3, value: string | null) => {
    const key = slot === 2 ? "multi_stt_model_2" : "multi_stt_model_3";
    const otherKey = slot === 2 ? "multi_stt_model_3" : "multi_stt_model_2";
    const previous = getSetting(key) as string | null;
    const otherSlot = getSetting(otherKey) as string | null;

    await updateSetting(key, value);
    if (enabled && value) {
      preloadExtras();
    }
    // Free the VRAM of the model this slot replaced — unless it is still
    // configured in the other slot.
    if (previous && previous !== value && previous !== otherSlot) {
      void commands
        .unloadExtraModel(previous)
        .catch((err) =>
          console.error("Failed to unload replaced multi-STT model:", err),
        );
    }
  };

  const handleBrainModeChange = async (
    value: "text_only" | "separate_asr" | "audio_in_merge",
  ) => {
    await updateSetting("multi_stt_brain_mode", value);
    if (enabled && value !== "text_only") {
      warmBrainWithMmproj(true);
    }
  };

  // When the component mounts with multi-STT already enabled, make sure the
  // extra models and (if needed) the mmproj server are ready.
  useEffect(() => {
    if (enabled) {
      preloadExtras();
      if (brainMode !== "text_only") {
        warmBrainWithMmproj(true);
      }
    }
  }, []);

  return (
    <div className="px-2 space-y-2">
      <div className="flex items-center gap-2 flex-wrap">
        <label
          className="inline-flex items-center cursor-pointer text-xs text-mid-gray hover:text-foreground select-none"
          title={t("multiStt.enabled.description")}
        >
          <input
            type="checkbox"
            className="sr-only peer"
            checked={enabled}
            onChange={(e) => void handleToggleEnabled(e.target.checked)}
          />
          <div className="relative w-9 h-5 bg-mid-gray/20 rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:start-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-logo-primary peer-disabled:opacity-50 mr-1.5"></div>
          <ListChecks size={13} className="mr-1 inline-block" />
          <span>{t("multiStt.enabled.label")}</span>
        </label>

        {enabled && (
          <div className="flex items-center gap-2 flex-wrap">
            <Dropdown
              selectedValue={model2 ?? ""}
              options={slot2Options}
              onSelect={(value) => void handleModelSelect(2, value || null)}
              placeholder={t("multiStt.models.notSelected")}
              className="min-w-[180px]"
            />
            <Dropdown
              selectedValue={model3 ?? ""}
              options={slot3Options}
              onSelect={(value) => void handleModelSelect(3, value || null)}
              placeholder={t("multiStt.models.notSelected")}
              className="min-w-[180px]"
            />

            <Dropdown
              selectedValue={brainMode}
              options={[
                {
                  value: "text_only",
                  label: t("multiStt.brainMode.options.textOnly"),
                },
                {
                  value: "separate_asr",
                  label: t("multiStt.brainMode.options.separateAsr"),
                },
                {
                  value: "audio_in_merge",
                  label: t("multiStt.brainMode.options.audioInMerge"),
                },
              ]}
              onSelect={(value) =>
                void handleBrainModeChange(
                  value as "text_only" | "separate_asr" | "audio_in_merge",
                )
              }
              className="min-w-[150px]"
            />
          </div>
        )}
      </div>
    </div>
  );
};
