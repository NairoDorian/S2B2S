import React from "react";
import { useTranslation } from "react-i18next";
import type { FftSource } from "@/bindings";
import { SettingsGroup, ToggleSwitch } from "@/components/ui";
import { Button } from "@/components/ui/Button";
import { useSettings } from "@/hooks/useSettings";
import { NoiseSuppression } from "../NoiseSuppression";
import { NoiseSuppressionParams } from "../NoiseSuppressionParams";
import { VadMeter, VadStatusChip, useVadFrames } from "../VadMeter";
import { VadSensitivity } from "../VadSensitivity";
import { VoiceActivityDetection } from "../VoiceActivityDetection";

/** Mirrors `DEFAULT_VAD_THRESHOLD_EARSHOT` in `src-tauri/src/settings.rs`. */
const DEFAULT_THRESHOLD = 0.5;
const P = "settings.liveFft.voice";

interface VoiceDetectionGroupProps {
  /** `live_fft.show_vad`: the session runs the detector and streams verdicts. */
  enabled: boolean;
  /** The analyser session is running. */
  active: boolean;
  busy: boolean;
  onToggle: (enabled: boolean) => void;
  /** The analysed signal, so the group can offer the suppressed one. */
  source: FftSource;
  onSource: (source: FftSource) => void;
}

/**
 * The speech detector and the noise suppressor beside the spectrum: the
 * detector's verdict frame by frame while the analyser runs (with RNNoise's
 * own speech probability when suppression is on), plus the settings that
 * shape both (VAD on/off for dictation, the threshold, RNNoise and its
 * strength / gate). Everything applies on the next frame, so each change
 * shows up at once; analysing the "after noise suppression" source shows
 * what RNNoise removes.
 */
export const VoiceDetectionGroup: React.FC<VoiceDetectionGroupProps> = ({
  enabled,
  active,
  busy,
  onToggle,
  source,
  onSource,
}) => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const threshold = getSetting("vad_threshold_earshot") ?? DEFAULT_THRESHOLD;
  const denoise = getSetting("denoise_enabled") ?? false;
  const denoiseThreshold = getSetting("denoise_vad_threshold") ?? 0;
  const frames = useVadFrames(enabled && active);

  return (
    <SettingsGroup title={t(`${P}.title`)} description={t(`${P}.description`)}>
      <ToggleSwitch
        checked={enabled}
        onChange={onToggle}
        isUpdating={busy}
        label={t(`${P}.show.label`)}
        description={t(`${P}.show.description`)}
        descriptionMode="tooltip"
        grouped
      />
      {enabled && (
        <div className="px-3 pb-3 flex flex-col gap-3">
          {active ? (
            <>
              <div className="flex items-center justify-end">
                <VadStatusChip frames={frames} />
              </div>
              <VadMeter
                frames={frames}
                threshold={threshold}
                denoiseThreshold={denoiseThreshold}
              />
            </>
          ) : (
            <p className="text-xs text-text/50">{t(`${P}.idle`)}</p>
          )}
        </div>
      )}
      <VoiceActivityDetection descriptionMode="tooltip" grouped />
      <VadSensitivity descriptionMode="tooltip" grouped alwaysShow />
      <NoiseSuppression descriptionMode="tooltip" grouped />
      <NoiseSuppressionParams descriptionMode="tooltip" grouped />
      {denoise && source !== "denoised" && (
        <div className="px-3 pb-3 flex flex-wrap items-center justify-between gap-2">
          <p className="text-xs text-text/60">{t(`${P}.sourceHint`)}</p>
          <Button
            variant="secondary"
            size="sm"
            disabled={busy}
            onClick={() => onSource("denoised")}
          >
            {t(`${P}.sourceSwitch`)}
          </Button>
        </div>
      )}
    </SettingsGroup>
  );
};
