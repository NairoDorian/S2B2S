import React, { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { Textarea } from "../../ui/Textarea";
import { useSettings } from "../../../hooks/useSettings";
import { ShortcutInput } from "../ShortcutInput";

/**
 * AI Replace: select text anywhere, hit the shortcut, and the Brain rewrites
 * the selection in place according to the configured instruction.
 */
export const AiReplaceSettings: React.FC = () => {
  const { t } = useTranslation();
  const { settings, updateSetting, isUpdating } = useSettings();

  const instruction = settings?.ai_replace_instruction ?? "";

  useEffect(() => {
    const unlistenStarted = listen("ai-replace:started", () => {
      toast.info(t("settings.postProcessing.aiReplace.toastStarted"), {
        duration: 2000,
      });
    });
    const unlistenDone = listen<string>("ai-replace:done", (event) => {
      toast.success(
        t("settings.postProcessing.aiReplace.toastDone", {
          length: event.payload.length,
        }),
      );
    });
    const unlistenError = listen<string>("ai-replace:error", (event) => {
      toast.error(
        t("settings.postProcessing.aiReplace.toastError", {
          error: event.payload,
        }),
      );
    });
    return () => {
      void unlistenStarted.then((fn) => fn());
      void unlistenDone.then((fn) => fn());
      void unlistenError.then((fn) => fn());
    };
  }, [t]);

  return (
    <SettingsGroup title={t("settings.postProcessing.aiReplace.group")}>
      <ShortcutInput
        shortcutId="ai_replace"
        descriptionMode="tooltip"
        grouped={true}
      />
      <SettingContainer
        title={t("settings.postProcessing.aiReplace.instruction.label")}
        description={t(
          "settings.postProcessing.aiReplace.instruction.description",
        )}
        grouped
        layout="stacked"
      >
        <Textarea
          variant="compact"
          rows={3}
          value={instruction}
          disabled={isUpdating("ai_replace_instruction")}
          onChange={(e) =>
            void updateSetting("ai_replace_instruction", e.target.value)
          }
        />
        <p className="text-[11px] text-text/40 font-mono pt-1">
          {"${selected_text} ${active_app} ${clipboard} ${time_local}"}
        </p>
      </SettingContainer>
      <p className="px-4 pb-3 text-[11px] text-text/40 leading-relaxed">
        {t("settings.postProcessing.aiReplace.usesBrain")}
      </p>
    </SettingsGroup>
  );
};
