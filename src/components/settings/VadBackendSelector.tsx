import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import type { VadBackend } from "@/bindings";

interface VadBackendSelectorProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

export const VadBackendSelector: React.FC<VadBackendSelectorProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const currentBackend = (getSetting("vad_backend") ?? "silero") as VadBackend;
  const isEarshot = currentBackend === "earshot";

  return (
    <ToggleSwitch
      checked={isEarshot}
      onChange={(checked) =>
        updateSetting(
          "vad_backend",
          (checked ? "earshot" : "silero") as VadBackend,
        )
      }
      isUpdating={isUpdating("vad_backend")}
      label={t("settings.advanced.vadBackend.title")}
      description={t("settings.advanced.vadBackend.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    />
  );
};
