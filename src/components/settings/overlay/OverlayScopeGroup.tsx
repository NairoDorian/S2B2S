import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
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
import { useNavigationStore } from "@/stores/navigationStore";
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
export const OverlayScopeGroup: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const setSection = useNavigationStore((s) => s.setSection);

  const stored = getSetting("overlay_scope");
  const options = useMemo(() => resolveOverlayScope(stored), [stored]);

  // Local mirror so a slider drag feels immediate; writes are coalesced.
  const [draft, setDraft] = useState<ResolvedOverlayScope>(options);
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const saveTimer = useRef<number | null>(null);
  useEffect(() => {
    if (saveTimer.current === null) setDraft(options);
  }, [options]);

  const save = useCallback(
    (patch: Partial<ResolvedOverlayScope>) => {
      const next = { ...draftRef.current, ...patch };
      // Keep the fade inside the window as the window shrinks.
      next.wave_taper_samples = Math.min(
        next.wave_taper_samples,
        Math.floor(next.wave_samples / 2),
      );
      setDraft(next);
      draftRef.current = next;
      if (saveTimer.current !== null) window.clearTimeout(saveTimer.current);
      saveTimer.current = window.setTimeout(() => {
        saveTimer.current = null;
        void updateSetting("overlay_scope", draftRef.current);
      }, SAVE_DEBOUNCE_MS);
    },
    [updateSetting],
  );
  useEffect(
    () => () => {
      if (saveTimer.current !== null) {
        window.clearTimeout(saveTimer.current);
        void updateSetting("overlay_scope", draftRef.current);
      }
    },
    [updateSetting],
  );

  const busy = isUpdating("overlay_scope");
  const styleOptions = useMemo<DropdownOption[]>(
    () =>
      OVERLAY_SCOPE_STYLES.map((style) => ({
        value: style,
        label: t(`${P}.style.options.${style}`),
      })),
    [t],
  );
  const anyView = draft.show_spectrum || draft.show_wave;

  return (
    <SettingsGroup title={t(`${P}.title`)} description={t(`${P}.description`)}>
      <ToggleSwitch
        checked={draft.show_spectrum}
        onChange={(checked) => save({ show_spectrum: checked })}
        isUpdating={busy}
        label={t(`${P}.showSpectrum.label`)}
        description={t(`${P}.showSpectrum.description`)}
        descriptionMode="tooltip"
        grouped
      />
      {draft.show_spectrum && (
        <>
          <SettingContainer
            title={t(`${P}.style.label`)}
            description={t(`${P}.style.description`)}
            descriptionMode="tooltip"
            grouped
          >
            <Dropdown
              options={styleOptions}
              selectedValue={draft.spectrum_style}
              onSelect={(value) =>
                save({ spectrum_style: value as OverlayScopeStyle })
              }
              disabled={busy}
            />
          </SettingContainer>
          <ToggleSwitch
            checked={draft.spectrum_mirror}
            onChange={(checked) => save({ spectrum_mirror: checked })}
            isUpdating={busy}
            label={t(`${P}.mirror.label`)}
            description={t(`${P}.mirror.description`)}
            descriptionMode="tooltip"
            grouped
          />
          <ToggleSwitch
            checked={draft.peak_hold}
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
        checked={draft.show_wave}
        onChange={(checked) => save({ show_wave: checked })}
        isUpdating={busy}
        label={t(`${P}.showWave.label`)}
        description={t(`${P}.showWave.description`)}
        descriptionMode="tooltip"
        grouped
      />
      {draft.show_wave && (
        <>
          <ParamSlider
            label={t(`${P}.waveSamples.label`)}
            description={t(`${P}.waveSamples.description`)}
            value={draft.wave_samples}
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
            value={draft.wave_taper_samples}
            min={0}
            max={Math.floor(draft.wave_samples / 2)}
            step={1}
            integer
            defaultValue={Math.min(
              OVERLAY_SCOPE_DEFAULTS.wave_taper_samples,
              Math.floor(draft.wave_samples / 2),
            )}
            onChange={(v) => save({ wave_taper_samples: Math.round(v) })}
          />
          <ParamSlider
            label={t(`${P}.gainFloor.label`)}
            description={t(`${P}.gainFloor.description`)}
            value={draft.wave_gain_floor}
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
            value={draft.view_width}
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
            value={draft.view_height}
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
      <div className="px-3 pb-3 flex flex-wrap items-center justify-between gap-2">
        <p className="text-xs text-text/60">{t(`${P}.analysisNote`)}</p>
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
