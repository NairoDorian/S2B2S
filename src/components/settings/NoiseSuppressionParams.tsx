import { useTranslation } from "@/i18n/useTranslation";
import { Slider } from "../ui/Slider";
import { useSettings } from "../../hooks/useSettings";
import type { JSX } from "@solidjs/web";

interface NoiseSuppressionParamsProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

/** Mirror `DenoiseParams::default()` in `audio_toolkit/audio/denoise.rs`. */
const DEFAULT_STRENGTH = 1;
const DEFAULT_VAD_THRESHOLD = 0;
const DEFAULT_VAD_GRACE_MS = 200;
const P = "settings.advanced.noiseSuppressionParams";

const percent = (v: number) => `${Math.round(v * 100)}%`;

/**
 * The three things that shape RNNoise's output: its wet/dry strength, the
 * threshold on its own speech probability below which frames are muted, and
 * the grace period that keeps word endings. Each applies on the next frame,
 * mid-recording included, so the live meters show the change at once.
 */
export const NoiseSuppressionParams = (
  props: NoiseSuppressionParamsProps,
): JSX.Element => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  // All setting reads live in the JSX bindings: the sliders follow the
  // settings as they change, including the enable-gate.

  return (
    <>
      <Slider
        value={getSetting("denoise_strength") ?? DEFAULT_STRENGTH}
        onChange={(next) =>
          updateSetting("denoise_strength", Math.round(next * 100) / 100)
        }
        min={0}
        max={1}
        step={0.05}
        label={t(`${P}.strength.label`)}
        description={t(`${P}.strength.description`)}
        descriptionMode={props.descriptionMode}
        grouped={props.grouped}
        formatValue={percent}
        onReset={() => updateSetting("denoise_strength", DEFAULT_STRENGTH)}
        disabled={
          !(getSetting("denoise_enabled") ?? false) ||
          isUpdating("denoise_strength")
        }
      />
      <Slider
        value={getSetting("denoise_vad_threshold") ?? DEFAULT_VAD_THRESHOLD}
        onChange={(next) =>
          updateSetting("denoise_vad_threshold", Math.round(next * 100) / 100)
        }
        min={0}
        max={1}
        step={0.05}
        label={t(`${P}.vadThreshold.label`)}
        description={t(`${P}.vadThreshold.description`)}
        descriptionMode={props.descriptionMode}
        grouped={props.grouped}
        formatValue={(v) => (v <= 0 ? t(`${P}.vadThreshold.off`) : percent(v))}
        onReset={() =>
          updateSetting("denoise_vad_threshold", DEFAULT_VAD_THRESHOLD)
        }
        disabled={
          !(getSetting("denoise_enabled") ?? false) ||
          isUpdating("denoise_vad_threshold")
        }
      />
      <Slider
        value={getSetting("denoise_vad_grace_ms") ?? DEFAULT_VAD_GRACE_MS}
        onChange={(next) =>
          updateSetting("denoise_vad_grace_ms", Math.round(next / 10) * 10)
        }
        min={0}
        max={2000}
        step={10}
        label={t(`${P}.vadGrace.label`)}
        description={t(`${P}.vadGrace.description`)}
        descriptionMode={props.descriptionMode}
        grouped={props.grouped}
        formatValue={(v) => `${Math.round(v)} ms`}
        onReset={() =>
          updateSetting("denoise_vad_grace_ms", DEFAULT_VAD_GRACE_MS)
        }
        disabled={
          !(getSetting("denoise_enabled") ?? false) ||
          (getSetting("denoise_vad_threshold") ?? DEFAULT_VAD_THRESHOLD) <= 0 ||
          isUpdating("denoise_vad_grace_ms")
        }
      />
    </>
  );
};
