import { For } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { SettingsGroup, Slider } from "@/components/ui";
import { Button } from "@/components/ui/Button";
import {
  LIVE_CANVAS_MOTION_DEFAULTS,
  LIVE_CANVAS_MOTION_LIMITS,
  liveCanvasTiming,
  type LiveCanvasMotion,
} from "@/lib/liveCanvasMotion";
import { useLiveCanvasMotionStore } from "@/stores/liveCanvasMotionStore";
import { setLiveCanvas } from "@/stores/navigationStore";

export const LiveCanvasSettings = () => {
  const { t } = useTranslation();
  const store = useLiveCanvasMotionStore();
  const keys: (keyof LiveCanvasMotion)[] = [
    "speed",
    "travel_ms",
    "particle_fade_ms",
    "character_fade_ms",
  ];
  return (
    <div class="w-full max-w-3xl space-y-6">
      <div class="flex items-center justify-between gap-4">
        <h1 class="text-xl font-semibold">{t("sidebar.liveCanvas")}</h1>
        <Button onClick={() => setLiveCanvas(true)}>
          {t("settings.liveCanvas.open")}
        </Button>
      </div>
      <SettingsGroup
        title={t("settings.liveCanvas.animation")}
        description={t("settings.liveCanvas.description")}
      >
        <For each={keys}>
          {(key) => (
            <Slider
              label={t(`settings.liveCanvas.${key}.label`)}
              description={t(`settings.liveCanvas.${key}.description`)}
              grouped
              value={store.motion[key]}
              {...LIVE_CANVAS_MOTION_LIMITS[key]}
              onChange={(value) => store.update(key, value)}
              formatValue={(value) =>
                key === "speed" ? `${value.toFixed(2)}×` : `${value} ms`
              }
              onReset={() =>
                store.update(key, LIVE_CANVAS_MOTION_DEFAULTS[key])
              }
            />
          )}
        </For>
      </SettingsGroup>
      <SettingsGroup
        title={t("settings.liveCanvas.corrections")}
        description={t("settings.liveCanvas.correctionDescription")}
      >
        <For each={["correction_out_ms", "correction_in_ms"] as const}>
          {(key) => (
            <Slider
              grouped
              label={t(`settings.liveCanvas.${key}.label`)}
              description={t(`settings.liveCanvas.${key}.description`)}
              value={store.motion[key]}
              {...LIVE_CANVAS_MOTION_LIMITS[key]}
              onChange={(value) => store.update(key, value)}
              formatValue={(value) => `${value} ms`}
              onReset={() =>
                store.update(key, LIVE_CANVAS_MOTION_DEFAULTS[key])
              }
            />
          )}
        </For>
      </SettingsGroup>
      <SettingsGroup title={t("settings.liveCanvas.signal")}>
        <Slider
          label={t("settings.liveCanvas.signal_scale.label")}
          description={t("settings.liveCanvas.signal_scale.description")}
          grouped
          value={store.motion.signal_scale}
          {...LIVE_CANVAS_MOTION_LIMITS.signal_scale}
          onChange={(value) => store.update("signal_scale", value)}
          formatValue={(value) => `${value.toFixed(1)}×`}
          onReset={() => store.update("signal_scale", 1)}
        />
      </SettingsGroup>
      <div class="flex items-center justify-between gap-4 text-xs text-mid-gray">
        <p>
          {t("settings.liveCanvas.handoff", {
            ms:
              Math.round(liveCanvasTiming(store.motion).characterStart * 100) /
              100,
          })}
        </p>
        <Button variant="ghost" onClick={() => store.reset()}>
          {t("settings.liveCanvas.reset")}
        </Button>
      </div>
    </div>
  );
};
