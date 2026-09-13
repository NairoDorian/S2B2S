import { useTranslation } from "@/i18n/useTranslation";
import type { FftSource } from "@/bindings";
import { SettingsGroup, ToggleSwitch } from "@/components/ui";
import { Button } from "@/components/ui/Button";
import { useSettings } from "@/hooks/useSettings";
import { NoiseSuppression } from "../NoiseSuppression";
import { NoiseSuppressionParams } from "../NoiseSuppressionParams";
import { VadMeter, VadStatusChip, useVadFrames } from "../VadMeter";
import { VadSensitivity } from "../VadSensitivity";
import { VoiceActivityDetection } from "../VoiceActivityDetection";

const DEFAULT_THRESHOLD = 0.5;
const P = "settings.liveFft.voice";

interface VoiceDetectionGroupProps {
  enabled: boolean;
  active: boolean;
  busy: boolean;
  onToggle: (enabled: boolean) => void;
  source: FftSource;
  onSource: (source: FftSource) => void;
}

export const VoiceDetectionGroup = (props: VoiceDetectionGroupProps) => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const threshold = () =>
    getSetting("vad_threshold_earshot") ?? DEFAULT_THRESHOLD;
  const denoise = () => getSetting("denoise_enabled") ?? false;
  const denoiseThreshold = () => getSetting("denoise_vad_threshold") ?? 0;
  const frames = useVadFrames(() => props.enabled && props.active);

  return (
    <SettingsGroup title={t(`${P}.title`)} description={t(`${P}.description`)}>
      <ToggleSwitch
        checked={props.enabled}
        onChange={props.onToggle}
        isUpdating={props.busy}
        label={t(`${P}.show.label`)}
        description={t(`${P}.show.description`)}
        descriptionMode="tooltip"
        grouped
      />
      {props.enabled && (
        <div class="px-3 pb-3 flex flex-col gap-3">
          {props.active ? (
            <>
              <div class="flex items-center justify-end">
                <VadStatusChip frames={frames} />
              </div>
              <VadMeter
                frames={frames}
                threshold={threshold()}
                denoiseThreshold={denoiseThreshold()}
              />
            </>
          ) : (
            <p class="text-xs text-text/50">{t(`${P}.idle`)}</p>
          )}
        </div>
      )}
      <VoiceActivityDetection descriptionMode="tooltip" grouped />
      <VadSensitivity descriptionMode="tooltip" grouped alwaysShow />
      <NoiseSuppression descriptionMode="tooltip" grouped />
      <NoiseSuppressionParams descriptionMode="tooltip" grouped />
      {denoise() && props.source !== "denoised" && (
        <div class="px-3 pb-3 flex flex-wrap items-center justify-between gap-2">
          <p class="text-xs text-text/60">{t(`${P}.sourceHint`)}</p>
          <Button
            variant="secondary"
            size="sm"
            disabled={props.busy}
            onClick={() => props.onSource("denoised")}
          >
            {t(`${P}.sourceSwitch`)}
          </Button>
        </div>
      )}
    </SettingsGroup>
  );
};
