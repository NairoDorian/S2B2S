import { createSignal, onCleanup, Show } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { commands } from "@/bindings";
import { sessionToast as toast } from "@/lib/sessionToast";
import { SettingsGroup } from "@/components/ui";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { Button } from "@/components/ui/Button";
import { useSettings } from "@/hooks/useSettings";
import { ShowOverlay } from "../ShowOverlay";
import { SpeechStats } from "../SpeechStats";
import { OverlayScopeGroup } from "./OverlayScopeGroup";

export const OverlaySettings = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  // Read inside the JSX: a body-level read would freeze at mount.
  const shown = () => (getSetting("overlay_style") ?? "live") !== "none";

  const [previewing, setPreviewing] = createSignal(false);

  const startPreview = async () => {
    try {
      await commands.startOverlayPreview();
      setPreviewing(true);
    } catch (error) {
      // "Already recording" and friends arrive here; the backend started nothing.
      toast.error(String(error));
    }
  };

  const stopPreview = async () => {
    setPreviewing(false);
    try {
      await commands.stopOverlayPreview();
    } catch (error) {
      toast.error(String(error));
    }
  };

  // The preview is driven from this page, so the page owns its lifetime: a
  // closed page would leave the microphone open behind a button nobody can
  // press any more. (The cancel hotkey and the backend's 10-minute safety cap
  // cover the other ways this page can go away.)
  onCleanup(() => {
    if (previewing()) {
      void commands.stopOverlayPreview().catch(() => {});
    }
  });

  return (
    <div class="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title={t("settings.overlay.preview.title")}>
        <SettingContainer
          title={t("settings.overlay.preview.title")}
          description={t("settings.overlay.preview.description")}
          descriptionMode="inline"
          grouped
          layout="stacked"
        >
          <Show
            when={!previewing()}
            fallback={
              <Button variant="danger" size="md" onClick={stopPreview}>
                {t("settings.overlay.preview.stop")}
              </Button>
            }
          >
            <Button variant="primary" size="md" onClick={startPreview}>
              {t("settings.overlay.preview.start")}
            </Button>
          </Show>
          <Show when={previewing()}>
            <p class="text-xs text-mid-gray mt-2">
              {t("settings.overlay.preview.runningNote")}
            </p>
          </Show>
        </SettingContainer>
      </SettingsGroup>

      <SettingsGroup title={t("settings.overlay.groups.appearance")}>
        <ShowOverlay descriptionMode="tooltip" grouped />
      </SettingsGroup>
      <Show when={shown()}>
        <>
          <SettingsGroup title={t("settings.overlay.groups.stats")}>
            <SpeechStats descriptionMode="tooltip" grouped />
          </SettingsGroup>
          <OverlayScopeGroup />
        </>
      </Show>
      <Show when={!shown()}>
        <p class="text-sm text-text/60 px-1">
          {t("settings.overlay.hiddenNote")}
        </p>
      </Show>
    </div>
  );
};
