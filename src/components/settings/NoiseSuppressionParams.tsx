import React from "react";
import { useTranslation } from "react-i18next";
import { Slider } from "../ui/Slider";
import { useSettings } from "../../hooks/useSettings";

interface NoiseSuppressionParamsProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

/** Mirror `DenoiseParams::default()` in `audio_toolkit/audio/denoise.rs`. */
const DEFAULT_STRENGTH = 1;
const DEFAULT_VAD_THRESHOLD = 0;
const DEFAULT_VAD_GRACE_MS = 200;
const P = "settings.advanced.noiseSuppressionParams";

/**
 * The three things that shape RNNoise's output: its wet/dry strength, the
 * threshold on its own speech probability below which frames are muted, and
 * the grace period that keeps word endings. Each applies on the next frame,
 * mid-recording included, so the live meters show the change at once.
 */
export const NoiseSuppressionParams: React.FC<NoiseSuppressionParamsProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const enabled = getSetting("denoise_enabled") ?? false;
  const strength = getSetting("denoise_strength") ?? DEFAULT_STRENGTH;
  const threshold =
    getSetting("denoise_vad_threshold") ?? DEFAULT_VAD_THRESHOLD;
  const grace = getSetting("denoise_vad_grace_ms") ?? DEFAULT_VAD_GRACE_MS;
  const percent = (v: number) => `${Math.round(v * 100)}%`;

  return (
    <>
      <Slider
        value={strength}
        onChange={(next) =>
          updateSetting("denoise_strength", Math.round(next * 100) / 100)
        }
        min={0}
        max={1}
        step={0.05}
        label={t(`${P}.strength.label`)}
        description={t(`${P}.strength.description`)}
        descriptionMode={descriptionMode}
        grouped={grouped}
        formatValue={percent}
        onReset={() => updateSetting("denoise_strength", DEFAULT_STRENGTH)}
        disabled={!enabled || isUpdating("denoise_strength")}
      />
      <Slider
        value={threshold}
        onChange={(next) =>
          updateSetting("denoise_vad_threshold", Math.round(next * 100) / 100)
        }
        min={0}
        max={1}
        step={0.05}
        label={t(`${P}.vadThreshold.label`)}
        description={t(`${P}.vadThreshold.description`)}
        descriptionMode={descriptionMode}
        grouped={grouped}
        formatValue={(v) => (v <= 0 ? t(`${P}.vadThreshold.off`) : percent(v))}
        onReset={() =>
          updateSetting("denoise_vad_threshold", DEFAULT_VAD_THRESHOLD)
        }
        disabled={!enabled || isUpdating("denoise_vad_threshold")}
      />
      <Slider
        value={grace}
        onChange={(next) =>
          updateSetting("denoise_vad_grace_ms", Math.round(next / 10) * 10)
        }
        min={0}
        max={2000}
        step={10}
        label={t(`${P}.vadGrace.label`)}
        description={t(`${P}.vadGrace.description`)}
        descriptionMode={descriptionMode}
        grouped={grouped}
        formatValue={(v) => `${Math.round(v)} ms`}
        onReset={() =>
          updateSetting("denoise_vad_grace_ms", DEFAULT_VAD_GRACE_MS)
        }
        disabled={
          !enabled || threshold <= 0 || isUpdating("denoise_vad_grace_ms")
        }
      />
    </>
  );
};
