import { useTranslation } from "@/i18n/useTranslation";
import { SettingsGroup } from "@/components/ui";
import { useSettings } from "@/hooks/useSettings";
import { ShowOverlay } from "../ShowOverlay";
import { SpeechStats } from "../SpeechStats";
import { OverlayScopeGroup } from "./OverlayScopeGroup";

export const OverlaySettings = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const shown = (getSetting("overlay_style") ?? "live") !== "none";

  return (
    <div class="max-w-3xl w-full mx-auto space-y-6">
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
        <p class="text-sm text-text/60 px-1">
          {t("settings.overlay.hiddenNote")}
        </p>
      )}
    </div>
  );
};
