import { createSignal, onCleanup, Show } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { commands } from "@/bindings";
import { sessionToast as toast } from "@/lib/sessionToast";
import { SettingsGroup } from "@/components/ui";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { Button } from "@/components/ui/Button";
import { Slider } from "@/components/ui/Slider";
import { useSettings } from "@/hooks/useSettings";
import { ShowOverlay } from "../ShowOverlay";
import { SpeechStats } from "../SpeechStats";
import { OverlayScopeGroup } from "./OverlayScopeGroup";

export const OverlaySettings = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
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

      <SettingsGroup title={t("settings.overlay.groups.window")}>
        <Slider
          value={getSetting("overlay_window_fade_ms") ?? 300}
          onChange={(next) =>
            updateSetting("overlay_window_fade_ms", Math.round(next))
          }
          min={0}
          max={2000}
          step={50}
          label={t("settings.overlay.window.fadeMs.label")}
          description={t("settings.overlay.window.fadeMs.description")}
          descriptionMode="tooltip"
          grouped
          formatValue={(v) => `${v}ms`}
          onReset={() => updateSetting("overlay_window_fade_ms", 300)}
          disabled={isUpdating("overlay_window_fade_ms")}
        />
        <Slider
          value={getSetting("overlay_window_corner_radius") ?? 0}
          onChange={(next) =>
            updateSetting(
              "overlay_window_corner_radius",
              Math.round(next * 2) / 2,
            )
          }
          min={0}
          max={20}
          step={0.5}
          label={t("settings.overlay.window.cornerRadius.label")}
          description={t("settings.overlay.window.cornerRadius.description")}
          descriptionMode="tooltip"
          grouped
          formatValue={(v) => `${v}px`}
          onReset={() => updateSetting("overlay_window_corner_radius", 0)}
          disabled={isUpdating("overlay_window_corner_radius")}
        />
        <Show when={getSetting("recording_overlay_use_manual_position")}>
          <SettingContainer
            title={t("settings.overlay.window.resetPosition.label")}
            description={t("settings.overlay.window.resetPosition.description")}
            descriptionMode="inline"
            grouped
            layout="stacked"
          >
            <Button
              variant="secondary"
              size="md"
              onClick={async () => {
                try {
                  await commands.resetRecordingOverlayManualPosition();
                } catch (error) {
                  toast.error(String(error));
                }
              }}
            >
              {t("settings.overlay.window.resetPositionButton")}
            </Button>
          </SettingContainer>
        </Show>
        <Show
          when={!(getSetting("recording_overlay_use_manual_position") ?? false)}
        >
          <p class="text-xs text-text/60 px-1">
            {t("settings.overlay.window.noManualPosition")}
          </p>
        </Show>
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
