import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import {
  Activity,
  AudioLines,
  Loader2,
  Pause,
  Play,
  RotateCcw,
  Square,
} from "lucide-react";
import { sessionToast as toast } from "@/lib/sessionToast";
import {
  commands,
  type FftBallisticsMode,
  type FftDbReference,
  type FftLoudnessMode,
  type FftMagnitudeNorm,
  type FftScale,
  type FftSource,
  type FftWarpInterp,
  type FftWeighting,
  type FftWindowLengthMode,
  type FftWindowType,
  type LiveFftPhase,
} from "@/bindings";
import { SettingContainer, SettingsGroup, ToggleSwitch } from "@/components/ui";
import { Alert } from "@/components/ui/Alert";
import { Button } from "@/components/ui/Button";
import { Dropdown, type DropdownOption } from "@/components/ui/Dropdown";
import { useSettings } from "@/hooks/useSettings";
import {
  getLastFrameAt,
  getLatestFrame,
  isFftActive,
  useLiveFftStore,
} from "@/stores/liveFftStore";
import { ParamSlider } from "./ParamSlider";
import {
  SpectrumCanvas,
  type HoverInfo,
  type SpectrumStyle,
} from "./SpectrumCanvas";
import { SpectrogramCanvas } from "./SpectrogramCanvas";
import { VoiceDetectionGroup } from "./VoiceDetectionGroup";
import {
  formatHz,
  formatValue,
  noteName,
  type ColormapKind,
} from "./liveFftMath";
import {
  FFT_SIZES,
  LIVE_FFT_DEFAULTS,
  LIVE_FFT_PRESETS,
  OUTPUT_BIN_CHOICES,
  applyPreset,
  resolveLiveFft,
  type ResolvedLiveFft,
} from "./liveFftPresets";

/** Frames arrive at the update rate; a gap this long means the backend stopped. */
const STALL_MS = 2000;
/** How often the readout row (peak, cursor, telemetry) re-renders. */
const READOUT_INTERVAL_MS = 250;
/** Slider drags are coalesced before they reach the settings store. */
const SAVE_DEBOUNCE_MS = 60;

const VIEW_KEY = "handy.live_fft.view";

interface ViewPrefs {
  style: SpectrumStyle;
  peakHold: boolean;
  waterfall: boolean;
  grid: boolean;
  colormap: ColormapKind;
}

const DEFAULT_VIEW: ViewPrefs = {
  style: "bars",
  peakHold: true,
  waterfall: true,
  grid: true,
  colormap: "inferno",
};

const readView = (): ViewPrefs => {
  try {
    const raw = window.localStorage.getItem(VIEW_KEY);
    if (!raw) return DEFAULT_VIEW;
    return { ...DEFAULT_VIEW, ...(JSON.parse(raw) as Partial<ViewPrefs>) };
  } catch {
    return DEFAULT_VIEW;
  }
};

const writeView = (view: ViewPrefs) => {
  try {
    window.localStorage.setItem(VIEW_KEY, JSON.stringify(view));
  } catch {
    // Display preference only.
  }
};

const PHASE_CLASSES: Record<LiveFftPhase, string> = {
  idle: "bg-mid-gray/15 text-text/70 border-mid-gray/20",
  starting: "bg-logo-primary/15 text-text border-logo-primary/30",
  running: "bg-green-500/15 text-green-500 border-green-500/30",
  stopping: "bg-logo-primary/15 text-text border-logo-primary/30",
  error: "bg-red-500/15 text-red-400 border-red-500/30",
};

const formatUs = (us: number): string =>
  us >= 1000 ? `${(us / 1000).toFixed(2)} ms` : `${us.toFixed(0)} µs`;

export const LiveFftSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const store = useLiveFftStore();
  const status = store.status;
  const active = isFftActive(status);
  const phase = status.phase;

  const stored = getSetting("live_fft");
  const options = useMemo<ResolvedLiveFft>(
    () => resolveLiveFft(stored),
    [stored],
  );

  // Local mirror so a slider drag feels immediate; writes are coalesced.
  const [draft, setDraft] = useState<ResolvedLiveFft>(options);
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const saveTimer = useRef<number | null>(null);
  useEffect(() => {
    if (saveTimer.current === null) setDraft(options);
  }, [options]);

  const save = useCallback(
    (patch: Partial<ResolvedLiveFft>) => {
      const next = { ...draftRef.current, ...patch };
      setDraft(next);
      draftRef.current = next;
      if (saveTimer.current !== null) window.clearTimeout(saveTimer.current);
      saveTimer.current = window.setTimeout(() => {
        saveTimer.current = null;
        void updateSetting("live_fft", draftRef.current);
      }, SAVE_DEBOUNCE_MS);
    },
    [updateSetting],
  );
  useEffect(
    () => () => {
      if (saveTimer.current !== null) {
        window.clearTimeout(saveTimer.current);
        void updateSetting("live_fft", draftRef.current);
      }
    },
    [updateSetting],
  );

  const [view, setView] = useState<ViewPrefs>(readView);
  const updateView = (patch: Partial<ViewPrefs>) =>
    setView((prev) => {
      const next = { ...prev, ...patch };
      writeView(next);
      return next;
    });

  useEffect(() => {
    void store.initialize();
  }, [store.initialize]);

  // Stop the analyser when the page goes away (docs/PERFORMANCE.md rule 9).
  const activeRef = useRef(active);
  activeRef.current = active;
  useEffect(
    () => () => {
      if (activeRef.current) void commands.liveFftStop();
    },
    [],
  );

  // Readout row: peak / cursor / stall detection, a few times a second.
  const [readout, setReadout] = useState({
    peakHz: 0,
    peakValue: 0,
    silent: false,
    stalled: false,
    dspUs: 0,
  });
  useEffect(() => {
    if (!active) return;
    const id = window.setInterval(() => {
      const frame = getLatestFrame();
      const stalled = Date.now() - getLastFrameAt() > STALL_MS;
      setReadout({
        peakHz: frame?.peakHz ?? 0,
        peakValue: frame?.peakValue ?? 0,
        silent: frame?.silent ?? false,
        stalled,
        dspUs: frame?.dspUs ?? 0,
      });
    }, READOUT_INTERVAL_MS);
    return () => window.clearInterval(id);
  }, [active]);
  const [hover, setHover] = useState<HoverInfo | null>(null);

  const start = async () => {
    const error = await store.start();
    if (error) toast.error(t("settings.liveFft.errors.start", { error }));
  };
  const stop = async () => {
    const error = await store.stop();
    if (error) toast.error(error);
  };

  const applyRaw = async () => {
    const defaults = await commands.liveFftRawDefaults();
    save(applyPreset(draftRef.current, resolveLiveFft(defaults)));
    toast.success(t("settings.liveFft.presets.applied"));
  };

  const axisHz = useMemo(
    () => Float32Array.from(status.axis_hz, (v) => v ?? 0),
    [status.axis_hz],
  );
  const canvasLabels = useMemo(
    () => ({
      idle: t("settings.liveFft.canvas.idle"),
      silence: t("settings.liveFft.canvas.silence"),
    }),
    [t],
  );

  const opt = (
    value: string,
    label: string,
    description?: string,
  ): DropdownOption => ({
    value,
    label,
    description,
  });
  const P = "settings.liveFft";
  const sourceOptions = useMemo<DropdownOption[]>(
    () => [
      opt(
        "microphone",
        t(`${P}.spectrum.source.microphone`),
        t(`${P}.spectrum.source.microphoneHint`),
      ),
      opt(
        "denoised",
        t(`${P}.spectrum.source.denoised`),
        t(`${P}.spectrum.source.denoisedHint`),
      ),
      opt(
        "processed",
        t(`${P}.spectrum.source.processed`),
        t(`${P}.spectrum.source.processedHint`),
      ),
    ],
    [t],
  );
  const scaleOptions = useMemo<DropdownOption[]>(
    () =>
      (
        ["log", "mel", "erb", "bark", "chroma", "linear", "melog"] as FftScale[]
      ).map((v) => opt(v, t(`${P}.spectrum.scale.options.${v}`))),
    [t],
  );
  const interpOptions = useMemo<DropdownOption[]>(
    () => [
      opt("linear", t(`${P}.spectrum.warpInterp.linear`)),
      opt("cubic", t(`${P}.spectrum.warpInterp.cubic`)),
    ],
    [t],
  );
  const windowModeOptions = useMemo<DropdownOption[]>(
    () => [
      opt("samples", t(`${P}.spectrum.windowMode.samples`)),
      opt("milliseconds", t(`${P}.spectrum.windowMode.milliseconds`)),
    ],
    [t],
  );
  const binOptions = useMemo<DropdownOption[]>(
    () => OUTPUT_BIN_CHOICES.map((n) => opt(String(n), String(n))),
    [],
  );
  const fftSizeOptions = useMemo<DropdownOption[]>(
    () => FFT_SIZES.map((n) => opt(String(n), `${n / 1024}K`)),
    [],
  );
  const windowTypeOptions = useMemo<DropdownOption[]>(
    () =>
      (
        [
          "kaiser",
          "hann",
          "hamming",
          "blackman",
          "blackman_harris",
          "rectangular",
        ] as FftWindowType[]
      ).map((v) => opt(v, t(`${P}.window.type.options.${v}`))),
    [t],
  );
  const weightingOptions = useMemo<DropdownOption[]>(
    () =>
      (["off", "a", "c", "itu468"] as FftWeighting[]).map((v) =>
        opt(v, t(`${P}.window.weighting.options.${v}`)),
      ),
    [t],
  );
  const normOptions = useMemo<DropdownOption[]>(
    () =>
      (["coherent_gain", "full_scale"] as FftMagnitudeNorm[]).map((v) =>
        opt(v, t(`${P}.window.magnitudeNorm.options.${v}`)),
      ),
    [t],
  );
  const loudnessOptions = useMemo<DropdownOption[]>(
    () =>
      (["off", "db", "db_normalized"] as FftLoudnessMode[]).map((v) =>
        opt(v, t(`${P}.loudness.mode.options.${v}`)),
      ),
    [t],
  );
  const dbRefOptions = useMemo<DropdownOption[]>(
    () =>
      (["frame_peak", "dbfs", "agc"] as FftDbReference[]).map((v) =>
        opt(v, t(`${P}.loudness.dbReference.options.${v}`)),
      ),
    [t],
  );
  const ballModeOptions = useMemo<DropdownOption[]>(
    () => [
      opt("coefficient", t(`${P}.loudness.ballisticsMode.coefficient`)),
      opt("milliseconds", t(`${P}.loudness.ballisticsMode.milliseconds`)),
    ],
    [t],
  );
  const styleOptions = useMemo<DropdownOption[]>(
    () => [
      opt("bars", t(`${P}.view.bars`)),
      opt("line", t(`${P}.view.line`)),
      opt("area", t(`${P}.view.area`)),
    ],
    [t],
  );
  const colormapOptions = useMemo<DropdownOption[]>(
    () => [
      opt("inferno", t(`${P}.view.colormap.inferno`)),
      opt("accent", t(`${P}.view.colormap.accent`)),
      opt("ice", t(`${P}.view.colormap.ice`)),
    ],
    [t],
  );

  const nyquist = status.nyquist_hz ?? 0;
  const resolutionHz =
    status.sample_rate > 0 && status.window_samples > 0
      ? status.sample_rate / status.window_samples
      : 0;
  const peakNote = readout.peakHz > 0 ? noteName(readout.peakHz) : null;
  const busy = isUpdating("live_fft");

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6 pb-8">
      <SettingsGroup
        title={t(`${P}.title`)}
        description={t(`${P}.description`)}
      >
        <div className="p-3 space-y-3">
          {phase === "error" && status.error && (
            <Alert variant="error">{status.error}</Alert>
          )}
          {!active && status.stop_reason && (
            <Alert variant="warning">
              {t(`${P}.stopReason.${status.stop_reason}`)}
            </Alert>
          )}

          <div className="flex flex-wrap items-center gap-3 rounded-lg border border-mid-gray/20 bg-background p-3">
            <div
              className={`p-3 rounded-full ${active ? "bg-green-500/15" : "bg-mid-gray/10"}`}
            >
              {phase === "running" ? (
                <Activity className="w-6 h-6 text-green-500" />
              ) : active ? (
                <Loader2 className="w-6 h-6 text-logo-primary animate-spin" />
              ) : (
                <AudioLines className="w-6 h-6 text-mid-gray" />
              )}
            </div>
            <div className="flex-1 min-w-[200px]">
              <div className="flex flex-wrap items-center gap-2">
                <span
                  className={`inline-flex items-center px-2 py-0.5 rounded text-[11px] font-medium border ${PHASE_CLASSES[phase]}`}
                >
                  {t(`${P}.status.${phase}`)}
                </span>
                {active && readout.stalled && (
                  <span className="inline-flex items-center px-2 py-0.5 rounded text-[11px] font-medium border bg-amber-500/10 text-amber-400 border-amber-500/20">
                    {t(`${P}.noSignal`)}
                  </span>
                )}
                {active && (
                  <span className="text-xs text-mid-gray">
                    {t(`${P}.telemetry.threading`, {
                      mode: status.async_analysis
                        ? t(`${P}.telemetry.async`)
                        : t(`${P}.telemetry.inline`),
                    })}
                  </span>
                )}
              </div>
              <dl className="mt-2 grid grid-cols-3 sm:grid-cols-6 gap-x-3 gap-y-1 text-xs">
                <div>
                  <dt className="text-mid-gray">
                    {t(`${P}.telemetry.sampleRate`)}
                  </dt>
                  <dd className="font-mono">
                    {status.sample_rate > 0 ? `${status.sample_rate}` : "—"}
                  </dd>
                </div>
                <div>
                  <dt className="text-mid-gray">
                    {t(`${P}.telemetry.fftSize`)}
                  </dt>
                  <dd className="font-mono">
                    {status.fft_size > 0 ? `${status.fft_size}` : "—"}
                  </dd>
                </div>
                <div>
                  <dt className="text-mid-gray">
                    {t(`${P}.telemetry.window`)}
                  </dt>
                  <dd className="font-mono">
                    {status.window_samples > 0
                      ? `${status.window_samples}`
                      : "—"}
                  </dd>
                </div>
                <div>
                  <dt className="text-mid-gray">
                    {t(`${P}.telemetry.resolution`)}
                  </dt>
                  <dd className="font-mono">
                    {resolutionHz > 0 ? `${resolutionHz.toFixed(1)} Hz` : "—"}
                  </dd>
                </div>
                <div>
                  <dt className="text-mid-gray">{t(`${P}.telemetry.dsp`)}</dt>
                  <dd className="font-mono">
                    {active && status.dsp_us_avg
                      ? formatUs(status.dsp_us_avg)
                      : "—"}
                  </dd>
                </div>
                <div>
                  <dt className="text-mid-gray">
                    {t(`${P}.telemetry.dropped`)}
                  </dt>
                  <dd className="font-mono">
                    {active ? `${status.dropped_samples}` : "—"}
                  </dd>
                </div>
              </dl>
            </div>
            <div className="flex items-center gap-2">
              {active && (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => store.setFrozen(!store.frozen)}
                  title={store.frozen ? t(`${P}.unfreeze`) : t(`${P}.freeze`)}
                >
                  <span className="inline-flex items-center gap-1.5">
                    {store.frozen ? (
                      <Play className="w-4 h-4" />
                    ) : (
                      <Pause className="w-4 h-4" />
                    )}
                    {store.frozen ? t(`${P}.unfreeze`) : t(`${P}.freeze`)}
                  </span>
                </Button>
              )}
              {active ? (
                <Button
                  variant="danger"
                  onClick={stop}
                  disabled={phase === "stopping"}
                >
                  <span className="inline-flex items-center gap-1.5">
                    <Square className="w-4 h-4" />
                    {t(`${P}.stop`)}
                  </span>
                </Button>
              ) : (
                <Button variant="primary" onClick={start}>
                  <span className="inline-flex items-center gap-1.5">
                    <AudioLines className="w-4 h-4" />
                    {t(`${P}.start`)}
                  </span>
                </Button>
              )}
            </div>
          </div>
          <p className="text-xs text-text/50">{t(`${P}.hotkeysNote`)}</p>
        </div>
      </SettingsGroup>

      <VoiceDetectionGroup
        enabled={draft.show_vad}
        active={active}
        busy={busy}
        onToggle={(checked) => save({ show_vad: checked })}
        source={draft.source}
        onSource={(source) => save({ source })}
      />

      <SettingsGroup title={t(`${P}.view.title`)}>
        <div className="p-3 space-y-2">
          <div className="flex flex-wrap items-center gap-2 text-xs">
            <Dropdown
              options={styleOptions}
              selectedValue={view.style}
              onSelect={(v) => updateView({ style: v as SpectrumStyle })}
              className="min-w-[110px]"
            />
            <ToggleChip
              active={view.peakHold}
              onClick={() => updateView({ peakHold: !view.peakHold })}
              label={t(`${P}.view.peakHold`)}
            />
            <ToggleChip
              active={view.grid}
              onClick={() => updateView({ grid: !view.grid })}
              label={t(`${P}.view.grid`)}
            />
            <ToggleChip
              active={view.waterfall}
              onClick={() => updateView({ waterfall: !view.waterfall })}
              label={t(`${P}.view.waterfall`)}
            />
            {view.waterfall && (
              <Dropdown
                options={colormapOptions}
                selectedValue={view.colormap}
                onSelect={(v) => updateView({ colormap: v as ColormapKind })}
                className="min-w-[120px]"
              />
            )}
          </div>

          <div className="rounded-lg border border-mid-gray/20 bg-background overflow-hidden">
            <SpectrumCanvas
              axisHz={axisHz}
              mode={draft.loudness_mode}
              dbRange={draft.db_range}
              style={view.style}
              peakHold={view.peakHold}
              grid={view.grid}
              running={active}
              labels={canvasLabels}
              onHover={setHover}
              className="h-64"
            />
            {view.waterfall && (
              <SpectrogramCanvas
                axisHz={axisHz}
                mode={draft.loudness_mode}
                dbRange={draft.db_range}
                colormap={view.colormap}
                running={active}
                className="h-36 border-t border-mid-gray/15"
              />
            )}
          </div>

          <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-text/70 font-mono min-h-[18px]">
            <span>
              {t(`${P}.view.peak`)}{" "}
              {active && !readout.silent && readout.peakHz > 0
                ? `${formatHz(readout.peakHz)}${peakNote ? ` (${peakNote})` : ""} ${formatValue(readout.peakValue, draft.loudness_mode)}`
                : "—"}
            </span>
            <span>
              {t(`${P}.view.cursor`)}{" "}
              {hover
                ? `${formatHz(hover.hz)} ${formatValue(hover.value, draft.loudness_mode)}`
                : "—"}
            </span>
            {active && (
              <span className="text-text/50">
                {t(`${P}.telemetry.frame`, {
                  bins: status.output_bins,
                  rate: status.update_rate_hz,
                  dsp: formatUs(readout.dspUs),
                })}
              </span>
            )}
          </div>
        </div>
      </SettingsGroup>

      <SettingsGroup
        title={t(`${P}.presets.title`)}
        description={t(`${P}.presets.description`)}
      >
        <div className="p-3 flex flex-wrap gap-2">
          {LIVE_FFT_PRESETS.map((preset) => (
            <Button
              key={preset.id}
              variant="secondary"
              size="sm"
              disabled={busy}
              onClick={() => {
                save(applyPreset(draftRef.current, preset.patch));
                toast.success(t(`${P}.presets.applied`));
              }}
            >
              {t(`${P}.presets.${preset.id}`)}
            </Button>
          ))}
          <Button
            variant="secondary"
            size="sm"
            disabled={busy}
            onClick={applyRaw}
          >
            {t(`${P}.presets.raw`)}
          </Button>
        </div>
      </SettingsGroup>

      <SettingsGroup title={t(`${P}.spectrum.title`)}>
        <SettingContainer
          title={t(`${P}.spectrum.source.label`)}
          description={t(`${P}.spectrum.source.description`)}
          descriptionMode="tooltip"
          grouped
          layout="horizontal"
        >
          <Dropdown
            options={sourceOptions}
            selectedValue={draft.source}
            onSelect={(v) => save({ source: v as FftSource })}
            className="min-w-[220px]"
          />
        </SettingContainer>
        <SettingContainer
          title={t(`${P}.spectrum.scale.label`)}
          description={t(`${P}.spectrum.scale.description`)}
          descriptionMode="tooltip"
          grouped
          layout="horizontal"
        >
          <Dropdown
            options={scaleOptions}
            selectedValue={draft.scale}
            onSelect={(v) => save({ scale: v as FftScale })}
            className="min-w-[200px]"
          />
        </SettingContainer>
        <SettingContainer
          title={t(`${P}.spectrum.warpInterp.label`)}
          description={t(`${P}.spectrum.warpInterp.description`)}
          descriptionMode="tooltip"
          grouped
          layout="horizontal"
        >
          <Dropdown
            options={interpOptions}
            selectedValue={draft.warp_interpolation}
            onSelect={(v) => save({ warp_interpolation: v as FftWarpInterp })}
            className="min-w-[200px]"
          />
        </SettingContainer>
        <ParamSlider
          label={t(`${P}.spectrum.displayMax.label`)}
          description={
            nyquist > 0
              ? `${t(`${P}.spectrum.displayMax.description`)} ${t(`${P}.spectrum.nyquistNote`, { hz: formatHz(nyquist) })}`
              : t(`${P}.spectrum.displayMax.description`)
          }
          value={draft.display_max_hz}
          min={100}
          max={48000}
          step={10}
          log
          unit="Hz"
          defaultValue={LIVE_FFT_DEFAULTS.display_max_hz}
          onChange={(v) => save({ display_max_hz: Math.round(v) })}
        />
        <SettingContainer
          title={t(`${P}.spectrum.outputBins.label`)}
          description={t(`${P}.spectrum.outputBins.description`)}
          descriptionMode="tooltip"
          grouped
          layout="horizontal"
        >
          <Dropdown
            options={binOptions}
            selectedValue={String(draft.output_bins)}
            onSelect={(v) => save({ output_bins: Number(v) })}
            className="min-w-[110px]"
          />
        </SettingContainer>
        <ParamSlider
          label={t(`${P}.spectrum.warpBlend.label`)}
          description={t(`${P}.spectrum.warpBlend.description`)}
          value={draft.warp_blend}
          min={0}
          max={1}
          step={0.001}
          defaultValue={LIVE_FFT_DEFAULTS.warp_blend}
          onChange={(v) => save({ warp_blend: v })}
        />
        <ParamSlider
          label={t(`${P}.spectrum.logFloor.label`)}
          description={t(`${P}.spectrum.logFloor.description`)}
          value={draft.log_floor_hz}
          min={1}
          max={500}
          step={1}
          log
          unit="Hz"
          defaultValue={LIVE_FFT_DEFAULTS.log_floor_hz}
          onChange={(v) => save({ log_floor_hz: Math.round(v * 10) / 10 })}
        />
        <SettingContainer
          title={t(`${P}.spectrum.windowMode.label`)}
          description={t(`${P}.spectrum.windowMode.description`)}
          descriptionMode="tooltip"
          grouped
          layout="horizontal"
        >
          <Dropdown
            options={windowModeOptions}
            selectedValue={draft.window_length_mode}
            onSelect={(v) =>
              save({ window_length_mode: v as FftWindowLengthMode })
            }
            className="min-w-[200px]"
          />
        </SettingContainer>
        {draft.window_length_mode === "samples" ? (
          <ParamSlider
            label={t(`${P}.spectrum.windowSamples.label`)}
            description={t(`${P}.spectrum.windowSamples.description`)}
            value={draft.window_samples}
            min={16}
            max={65536}
            step={1}
            log
            integer
            defaultValue={LIVE_FFT_DEFAULTS.window_samples}
            onChange={(v) => save({ window_samples: Math.round(v) })}
          />
        ) : (
          <ParamSlider
            label={t(`${P}.spectrum.windowMs.label`)}
            description={t(`${P}.spectrum.windowMs.description`)}
            value={draft.window_ms}
            min={1}
            max={1000}
            step={0.5}
            log
            unit="ms"
            defaultValue={LIVE_FFT_DEFAULTS.window_ms}
            onChange={(v) => save({ window_ms: Math.round(v * 2) / 2 })}
          />
        )}
        <SettingContainer
          title={t(`${P}.spectrum.fftSize.label`)}
          description={t(`${P}.spectrum.fftSize.description`)}
          descriptionMode="tooltip"
          grouped
          layout="horizontal"
        >
          <Dropdown
            options={fftSizeOptions}
            selectedValue={String(draft.fft_size)}
            onSelect={(v) => save({ fft_size: Number(v) })}
            className="min-w-[110px]"
          />
        </SettingContainer>
      </SettingsGroup>

      <SettingsGroup
        title={t(`${P}.eq.title`)}
        description={t(`${P}.eq.description`)}
      >
        <ToggleSwitch
          checked={draft.eq_enabled}
          onChange={(checked) => save({ eq_enabled: checked })}
          label={t(`${P}.eq.enable.label`)}
          description={t(`${P}.eq.enable.description`)}
          descriptionMode="tooltip"
          grouped
        />
        {draft.eq_enabled && (
          <>
            <ToggleSwitch
              checked={draft.high_shelf}
              onChange={(checked) => save({ high_shelf: checked })}
              label={t(`${P}.eq.highShelf.label`)}
              description={t(`${P}.eq.highShelf.description`)}
              descriptionMode="tooltip"
              grouped
            />
            <ParamSlider
              label={t(`${P}.eq.highGain.label`)}
              description={t(`${P}.eq.highGain.description`)}
              value={draft.high_gain_db}
              min={-24}
              max={24}
              step={0.1}
              unit="dB"
              disabled={!draft.high_shelf}
              defaultValue={LIVE_FFT_DEFAULTS.high_gain_db}
              onChange={(v) => save({ high_gain_db: v })}
            />
            <ParamSlider
              label={t(`${P}.eq.highCutoff.label`)}
              description={t(`${P}.eq.highCutoff.description`)}
              value={draft.high_cutoff_hz}
              min={20}
              max={20000}
              step={1}
              log
              unit="Hz"
              disabled={!draft.high_shelf}
              defaultValue={LIVE_FFT_DEFAULTS.high_cutoff_hz}
              onChange={(v) => save({ high_cutoff_hz: Math.round(v) })}
            />
            <ToggleSwitch
              checked={draft.low_shelf}
              onChange={(checked) => save({ low_shelf: checked })}
              label={t(`${P}.eq.lowShelf.label`)}
              description={t(`${P}.eq.lowShelf.description`)}
              descriptionMode="tooltip"
              grouped
            />
            <ParamSlider
              label={t(`${P}.eq.lowGain.label`)}
              description={t(`${P}.eq.lowGain.description`)}
              value={draft.low_gain_db}
              min={-24}
              max={24}
              step={0.1}
              unit="dB"
              disabled={!draft.low_shelf}
              defaultValue={LIVE_FFT_DEFAULTS.low_gain_db}
              onChange={(v) => save({ low_gain_db: v })}
            />
            <ParamSlider
              label={t(`${P}.eq.lowCutoff.label`)}
              description={t(`${P}.eq.lowCutoff.description`)}
              value={draft.low_cutoff_hz}
              min={20}
              max={5000}
              step={1}
              log
              unit="Hz"
              disabled={!draft.low_shelf}
              defaultValue={LIVE_FFT_DEFAULTS.low_cutoff_hz}
              onChange={(v) => save({ low_cutoff_hz: Math.round(v) })}
            />
            <ParamSlider
              label={t(`${P}.eq.q.label`)}
              description={t(`${P}.eq.q.description`)}
              value={draft.eq_q}
              min={0.1}
              max={4}
              step={0.001}
              defaultValue={LIVE_FFT_DEFAULTS.eq_q}
              onChange={(v) => save({ eq_q: v })}
            />
            <ParamSlider
              label={t(`${P}.eq.amount.label`)}
              description={t(`${P}.eq.amount.description`)}
              value={draft.eq_amount}
              min={0}
              max={5}
              step={0.01}
              defaultValue={LIVE_FFT_DEFAULTS.eq_amount}
              onChange={(v) => save({ eq_amount: v })}
            />
          </>
        )}
      </SettingsGroup>

      <SettingsGroup title={t(`${P}.window.title`)}>
        <SettingContainer
          title={t(`${P}.window.type.label`)}
          description={t(`${P}.window.type.description`)}
          descriptionMode="tooltip"
          grouped
          layout="horizontal"
        >
          <Dropdown
            options={windowTypeOptions}
            selectedValue={draft.window_type}
            onSelect={(v) => save({ window_type: v as FftWindowType })}
            className="min-w-[200px]"
          />
        </SettingContainer>
        {draft.window_type === "kaiser" && (
          <ParamSlider
            label={t(`${P}.window.kaiserBeta.label`)}
            description={t(`${P}.window.kaiserBeta.description`)}
            value={draft.kaiser_beta}
            min={0}
            max={55}
            step={0.1}
            defaultValue={LIVE_FFT_DEFAULTS.kaiser_beta}
            onChange={(v) => save({ kaiser_beta: v })}
          />
        )}
        <SettingContainer
          title={t(`${P}.window.weighting.label`)}
          description={t(`${P}.window.weighting.description`)}
          descriptionMode="tooltip"
          grouped
          layout="horizontal"
        >
          <Dropdown
            options={weightingOptions}
            selectedValue={draft.weighting}
            onSelect={(v) => save({ weighting: v as FftWeighting })}
            className="min-w-[200px]"
          />
        </SettingContainer>
        <SettingContainer
          title={t(`${P}.window.magnitudeNorm.label`)}
          description={t(`${P}.window.magnitudeNorm.description`)}
          descriptionMode="tooltip"
          grouped
          layout="horizontal"
        >
          <Dropdown
            options={normOptions}
            selectedValue={draft.magnitude_norm}
            onSelect={(v) => save({ magnitude_norm: v as FftMagnitudeNorm })}
            className="min-w-[200px]"
          />
        </SettingContainer>
      </SettingsGroup>

      <SettingsGroup title={t(`${P}.loudness.title`)}>
        <SettingContainer
          title={t(`${P}.loudness.mode.label`)}
          description={t(`${P}.loudness.mode.description`)}
          descriptionMode="tooltip"
          grouped
          layout="horizontal"
        >
          <Dropdown
            options={loudnessOptions}
            selectedValue={draft.loudness_mode}
            onSelect={(v) => save({ loudness_mode: v as FftLoudnessMode })}
            className="min-w-[200px]"
          />
        </SettingContainer>
        {draft.loudness_mode !== "off" && (
          <>
            <SettingContainer
              title={t(`${P}.loudness.dbReference.label`)}
              description={t(`${P}.loudness.dbReference.description`)}
              descriptionMode="tooltip"
              grouped
              layout="horizontal"
            >
              <Dropdown
                options={dbRefOptions}
                selectedValue={draft.db_reference}
                onSelect={(v) => save({ db_reference: v as FftDbReference })}
                className="min-w-[200px]"
              />
            </SettingContainer>
            <ParamSlider
              label={t(`${P}.loudness.dbRange.label`)}
              description={t(`${P}.loudness.dbRange.description`)}
              value={draft.db_range}
              min={10}
              max={160}
              step={1}
              unit="dB"
              defaultValue={LIVE_FFT_DEFAULTS.db_range}
              onChange={(v) => save({ db_range: Math.round(v) })}
            />
          </>
        )}
        <ToggleSwitch
          checked={draft.ballistics_enabled}
          onChange={(checked) => save({ ballistics_enabled: checked })}
          label={t(`${P}.loudness.ballistics.label`)}
          description={t(`${P}.loudness.ballistics.description`)}
          descriptionMode="tooltip"
          grouped
        />
        {draft.ballistics_enabled && (
          <>
            <SettingContainer
              title={t(`${P}.loudness.ballisticsMode.label`)}
              description={t(`${P}.loudness.ballisticsMode.description`)}
              descriptionMode="tooltip"
              grouped
              layout="horizontal"
            >
              <Dropdown
                options={ballModeOptions}
                selectedValue={draft.ballistics_mode}
                onSelect={(v) =>
                  save({ ballistics_mode: v as FftBallisticsMode })
                }
                className="min-w-[200px]"
              />
            </SettingContainer>
            {draft.ballistics_mode === "milliseconds" ? (
              <>
                <ParamSlider
                  label={t(`${P}.loudness.attackMs.label`)}
                  description={t(`${P}.loudness.attackMs.description`)}
                  value={draft.attack_ms}
                  min={0}
                  max={2000}
                  step={1}
                  unit="ms"
                  defaultValue={LIVE_FFT_DEFAULTS.attack_ms}
                  onChange={(v) => save({ attack_ms: Math.round(v) })}
                />
                <ParamSlider
                  label={t(`${P}.loudness.releaseMs.label`)}
                  description={t(`${P}.loudness.releaseMs.description`)}
                  value={draft.release_ms}
                  min={0}
                  max={5000}
                  step={1}
                  unit="ms"
                  defaultValue={LIVE_FFT_DEFAULTS.release_ms}
                  onChange={(v) => save({ release_ms: Math.round(v) })}
                />
              </>
            ) : (
              <>
                <ParamSlider
                  label={t(`${P}.loudness.attack.label`)}
                  description={t(`${P}.loudness.attack.description`)}
                  value={draft.attack}
                  min={0}
                  max={0.99}
                  step={0.01}
                  defaultValue={LIVE_FFT_DEFAULTS.attack}
                  onChange={(v) => save({ attack: v })}
                />
                <ParamSlider
                  label={t(`${P}.loudness.release.label`)}
                  description={t(`${P}.loudness.release.description`)}
                  value={draft.release}
                  min={0}
                  max={0.99}
                  step={0.01}
                  defaultValue={LIVE_FFT_DEFAULTS.release}
                  onChange={(v) => save({ release: v })}
                />
              </>
            )}
          </>
        )}
        <SettingContainer
          title={t(`${P}.loudness.reset.label`)}
          description={t(`${P}.loudness.reset.description`)}
          descriptionMode="tooltip"
          grouped
          layout="horizontal"
        >
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void store.reset()}
            disabled={!active}
          >
            <span className="inline-flex items-center gap-1.5">
              <RotateCcw className="w-3.5 h-3.5" />
              {t(`${P}.loudness.reset.button`)}
            </span>
          </Button>
        </SettingContainer>
      </SettingsGroup>

      <SettingsGroup title={t(`${P}.performance.title`)}>
        <ToggleSwitch
          checked={draft.async_analysis}
          onChange={(checked) => save({ async_analysis: checked })}
          label={t(`${P}.performance.async.label`)}
          description={t(`${P}.performance.async.description`)}
          descriptionMode="tooltip"
          grouped
        />
        <ParamSlider
          label={t(`${P}.performance.updateRate.label`)}
          description={t(`${P}.performance.updateRate.description`)}
          value={draft.update_rate_hz}
          min={5}
          max={60}
          step={1}
          integer
          unit="Hz"
          defaultValue={LIVE_FFT_DEFAULTS.update_rate_hz}
          onChange={(v) => save({ update_rate_hz: Math.round(v) })}
        />
        <div className="px-3 py-2">
          <p className="text-xs text-text/50">{t(`${P}.performance.note`)}</p>
        </div>
      </SettingsGroup>
    </div>
  );
};

interface ToggleChipProps {
  active: boolean;
  onClick: () => void;
  label: string;
}

const ToggleChip: React.FC<ToggleChipProps> = ({ active, onClick, label }) => (
  <button
    type="button"
    role="switch"
    aria-checked={active}
    onClick={onClick}
    className={`px-2 py-1 rounded-md text-xs font-medium border transition-colors cursor-pointer ${
      active
        ? "bg-logo-primary/20 text-text border-logo-primary/40"
        : "bg-mid-gray/10 text-text/60 border-mid-gray/20 hover:bg-mid-gray/15"
    }`}
  >
    {label}
  </button>
);
