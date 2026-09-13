import { createSignal, createEffect, createMemo } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import type { OverlayScopeStyle } from "@/bindings";
import { SettingContainer, SettingsGroup, ToggleSwitch } from "@/components/ui";
import { Button } from "@/components/ui/Button";
import { Dropdown, type DropdownOption } from "@/components/ui/Dropdown";
import { useSettings } from "@/hooks/useSettings";
import {
  OVERLAY_SCOPE_DEFAULTS,
  OVERLAY_SCOPE_LIMITS,
  OVERLAY_SCOPE_STYLES,
  resolveOverlayScope,
  type ResolvedOverlayScope,
} from "@/lib/overlayScope";
import { setSection } from "@/stores/navigationStore";
import { ParamSlider } from "../live-fft/ParamSlider";

/** Slider drags coalesce into one write (and one overlay re-place). */
const SAVE_DEBOUNCE_MS = 150;
const P = "settings.overlay.scope";

/**
 * The picture the recording overlay draws of the microphone: which of the
 * two views to show, how the spectrum is drawn, how much raw audio the
 * waveform covers and how it is faded and scaled, and the size of the views.
 * The analysis behind the spectrum (scale, window, EQ, weighting, dB, update
 * rate) is the Live FFT page's; this group only shapes the display.
 */
export const OverlayScopeGroup = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const options = createMemo(() =>
    resolveOverlayScope(getSetting("overlay_scope")),
  );

  // Local mirror so a slider drag feels immediate; writes are coalesced.
  const [draft, setDraft] = createSignal<ResolvedOverlayScope>(options());
  let saveTimer: number | null = null;
  createEffect(
    () => options(),
    (opts) => {
      if (saveTimer === null) setDraft(opts);
    },
  );

  const save = (patch: Partial<ResolvedOverlayScope>) => {
    const next = { ...draft(), ...patch };
    // Keep the fade inside the window as the window shrinks.
    next.wave_taper_samples = Math.min(
      next.wave_taper_samples,
      Math.floor(next.wave_samples / 2),
    );
    setDraft(next);
    if (saveTimer !== null) window.clearTimeout(saveTimer);
    saveTimer = window.setTimeout(() => {
      saveTimer = null;
      void updateSetting("overlay_scope", draft());
    }, SAVE_DEBOUNCE_MS);
  };
  createEffect(
    () => undefined,
    () => {
      return () => {
        if (saveTimer !== null) {
          window.clearTimeout(saveTimer);
          void updateSetting("overlay_scope", draft());
        }
      };
    },
  );

  const busy = isUpdating("overlay_scope");
  const styleOptions = createMemo<DropdownOption[]>(() =>
    OVERLAY_SCOPE_STYLES.map((style) => ({
      value: style,
      label: t(`${P}.style.options.${style}`),
    })),
  );
  const anyView =
    draft().show_spectrum || draft().show_wave || draft().show_circular;

  return (
    <SettingsGroup title={t(`${P}.title`)} description={t(`${P}.description`)}>
      <ToggleSwitch
        checked={draft().show_spectrum}
        onChange={(checked) => save({ show_spectrum: checked })}
        isUpdating={busy}
        label={t(`${P}.showSpectrum.label`)}
        description={t(`${P}.showSpectrum.description`)}
        descriptionMode="tooltip"
        grouped
      />
      {draft().show_spectrum && (
        <>
          <SettingContainer
            title={t(`${P}.style.label`)}
            description={t(`${P}.style.description`)}
            descriptionMode="tooltip"
            grouped
          >
            <Dropdown
              options={styleOptions()}
              selectedValue={draft().spectrum_style}
              onSelect={(value) =>
                save({ spectrum_style: value as OverlayScopeStyle })
              }
              disabled={busy}
            />
          </SettingContainer>
          <ToggleSwitch
            checked={draft().spectrum_mirror}
            onChange={(checked) => save({ spectrum_mirror: checked })}
            isUpdating={busy}
            label={t(`${P}.mirror.label`)}
            description={t(`${P}.mirror.description`)}
            descriptionMode="tooltip"
            grouped
          />
          <ToggleSwitch
            checked={draft().peak_hold}
            onChange={(checked) => save({ peak_hold: checked })}
            isUpdating={busy}
            label={t(`${P}.peakHold.label`)}
            description={t(`${P}.peakHold.description`)}
            descriptionMode="tooltip"
            grouped
          />
        </>
      )}
      <ToggleSwitch
        checked={draft().show_wave}
        onChange={(checked) => save({ show_wave: checked })}
        isUpdating={busy}
        label={t(`${P}.showWave.label`)}
        description={t(`${P}.showWave.description`)}
        descriptionMode="tooltip"
        grouped
      />
      <ToggleSwitch
        checked={draft().show_circular}
        onChange={(checked) => save({ show_circular: checked })}
        isUpdating={busy}
        label={t(`${P}.showCircular.label`)}
        description={t(`${P}.showCircular.description`)}
        descriptionMode="tooltip"
        grouped
      />
      {draft().show_circular && (
        <>
          <ToggleSwitch
            checked={draft().circular_bars}
            onChange={(checked) => save({ circular_bars: checked })}
            isUpdating={busy}
            label={t(`${P}.circularBars.label`)}
            description={t(`${P}.circularBars.description`)}
            descriptionMode="tooltip"
            grouped
          />
          <ParamSlider
            label={t(`${P}.circularBins.label`)}
            description={t(`${P}.circularBins.description`)}
            value={draft().circular_bins}
            min={OVERLAY_SCOPE_LIMITS.circularBins.min}
            max={OVERLAY_SCOPE_LIMITS.circularBins.max}
            step={1}
            log
            integer
            defaultValue={OVERLAY_SCOPE_DEFAULTS.circular_bins}
            onChange={(v) => save({ circular_bins: Math.round(v) })}
          />
          <ParamSlider
            label={t(`${P}.circularGain.label`)}
            description={t(`${P}.circularGain.description`)}
            value={draft().circular_gain}
            min={OVERLAY_SCOPE_LIMITS.circularGain.min}
            max={OVERLAY_SCOPE_LIMITS.circularGain.max}
            step={0.05}
            log
            defaultValue={OVERLAY_SCOPE_DEFAULTS.circular_gain}
            format={(v) => `${v.toFixed(2)}×`}
            onChange={(v) => save({ circular_gain: v })}
          />
          <ParamSlider
            label={t(`${P}.circularFloor.label`)}
            description={t(`${P}.circularFloor.description`)}
            value={draft().circular_floor}
            min={OVERLAY_SCOPE_LIMITS.circularFloor.min}
            max={OVERLAY_SCOPE_LIMITS.circularFloor.max}
            step={0.01}
            defaultValue={OVERLAY_SCOPE_DEFAULTS.circular_floor}
            format={(v) => `${Math.round(v * 100)}%`}
            onChange={(v) => save({ circular_floor: v })}
          />
        </>
      )}
      {draft().show_wave && (
        <>
          <ParamSlider
            label={t(`${P}.waveSamples.label`)}
            description={t(`${P}.waveSamples.description`)}
            value={draft().wave_samples}
            min={OVERLAY_SCOPE_LIMITS.waveSamples.min}
            max={OVERLAY_SCOPE_LIMITS.waveSamples.max}
            step={1}
            log
            integer
            defaultValue={OVERLAY_SCOPE_DEFAULTS.wave_samples}
            onChange={(v) => save({ wave_samples: Math.round(v) })}
          />
          <ParamSlider
            label={t(`${P}.waveTaper.label`)}
            description={t(`${P}.waveTaper.description`)}
            value={draft().wave_taper_samples}
            min={0}
            max={Math.floor(draft().wave_samples / 2)}
            step={1}
            integer
            defaultValue={Math.min(
              OVERLAY_SCOPE_DEFAULTS.wave_taper_samples,
              Math.floor(draft().wave_samples / 2),
            )}
            onChange={(v) => save({ wave_taper_samples: Math.round(v) })}
          />
          <ParamSlider
            label={t(`${P}.gainFloor.label`)}
            description={t(`${P}.gainFloor.description`)}
            value={draft().wave_gain_floor}
            min={OVERLAY_SCOPE_LIMITS.waveGainFloor.min}
            max={OVERLAY_SCOPE_LIMITS.waveGainFloor.max}
            step={0.001}
            log
            defaultValue={OVERLAY_SCOPE_DEFAULTS.wave_gain_floor}
            format={(v) => `${(20 * Math.log10(v)).toFixed(0)} dBFS`}
            onChange={(v) => save({ wave_gain_floor: v })}
          />
        </>
      )}
      {anyView && (
        <>
          <ParamSlider
            label={t(`${P}.viewWidth.label`)}
            description={t(`${P}.viewWidth.description`)}
            value={draft().view_width}
            min={OVERLAY_SCOPE_LIMITS.viewWidth.min}
            max={OVERLAY_SCOPE_LIMITS.viewWidth.max}
            step={1}
            integer
            unit="px"
            defaultValue={OVERLAY_SCOPE_DEFAULTS.view_width}
            onChange={(v) => save({ view_width: Math.round(v) })}
          />
          <ParamSlider
            label={t(`${P}.viewHeight.label`)}
            description={t(`${P}.viewHeight.description`)}
            value={draft().view_height}
            min={OVERLAY_SCOPE_LIMITS.viewHeight.min}
            max={OVERLAY_SCOPE_LIMITS.viewHeight.max}
            step={1}
            integer
            unit="px"
            defaultValue={OVERLAY_SCOPE_DEFAULTS.view_height}
            onChange={(v) => save({ view_height: Math.round(v) })}
          />
        </>
      )}
      <div class="px-3 pb-3 flex flex-wrap items-center justify-between gap-2">
        <p class="text-xs text-text/60">{t(`${P}.analysisNote`)}</p>
        <Button
          variant="secondary"
          size="sm"
          onClick={() => setSection("liveFft")}
        >
          {t(`${P}.openLiveFft`)}
        </Button>
      </div>
    </SettingsGroup>
  );
};
