import React from "react";
import { useTranslation } from "react-i18next";
import { SettingsGroup } from "@/components/ui";
import { useSettings } from "@/hooks/useSettings";
import { ShowOverlay } from "../ShowOverlay";
import { SpeechStats } from "../SpeechStats";
import { OverlayScopeGroup } from "./OverlayScopeGroup";

/**
 * Everything about the recording overlay in one place: whether and where it
 * shows and how live text arrives (Appearance), the speech statistics it can
 * carry, and the picture of the microphone it draws (Analyser). The analysis
 * behind that picture is configured on the Live FFT page.
 */
export const OverlaySettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const shown = (getSetting("overlay_style") ?? "live") !== "none";

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title={t("settings.overlay.groups.appearance")}>
        <ShowOverlay descriptionMode="tooltip" grouped />
      </SettingsGroup>
      {shown ? (
        <>
          <SettingsGroup title={t("settings.overlay.groups.stats")}>
            <SpeechStats descriptionMode="tooltip" grouped />
          </SettingsGroup>
          <OverlayScopeGroup />
        </>
      ) : (
        <p className="text-sm text-text/60 px-1">
          {t("settings.overlay.hiddenNote")}
        </p>
      )}
    </div>
  );
};
