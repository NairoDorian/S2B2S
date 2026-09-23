import { createSignal, createEffect, createMemo, For } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import {
  Activity,
  AudioLines,
  Loader2,
  Pause,
  Play,
  RotateCcw,
  Square,
} from "@/components/icons/lucide";
import { readPref, writePref } from "@/lib/appIdentity";
import { sessionToast as toast } from "@/lib/sessionToast";
import {
  commands,
  type FftBallisticsMode,
  type FftDbReference,
  type FftKaiserBetaMode,
  type FftLoudnessMode,
  type FftMagnitudeNorm,
  type FftOutputBinsMode,
  type FftScale,
  type FftSource,
  type FftWarpAggregation,
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
  initialize,
  isFftActive,
  reset,
  setFrozen,
  start,
  stop,
  useLiveFftStore,
} from "@/stores/liveFftStore";
import { ParamSlider } from "./ParamSlider";
import {
  SpectrumCanvas,
  type HoverInfo,
  type SpectrumStyle,
} from "./SpectrumCanvas";
import { SpectrogramCanvas, WATERFALL_ROWS } from "./SpectrogramCanvas";
import { VoiceDetectionGroup } from "./VoiceDetectionGroup";
import type { SpectralFeatures } from "./liveFftFrame";
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
  LIVE_FFT_QUALITY_PRESETS,
  OUTPUT_BIN_CHOICES,
  applyPreset,
  applyQualityPreset,
  matchingQualityPreset,
  resolveLiveFft,
  type ResolvedLiveFft,
} from "./liveFftPresets";

const STALL_MS = 2000;
const READOUT_INTERVAL_MS = 250;
const SAVE_DEBOUNCE_MS = 60;

const VIEW_PREF = "live_fft.view";

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
  const raw = readPref(VIEW_PREF);
  if (!raw) return DEFAULT_VIEW;
  try {
    return { ...DEFAULT_VIEW, ...(JSON.parse(raw) as Partial<ViewPrefs>) };
  } catch {
    return DEFAULT_VIEW;
  }
};

const writeView = (view: ViewPrefs) => {
  writePref(VIEW_PREF, JSON.stringify(view));
};

const PHASE_CLASSES: Record<LiveFftPhase, string> = {
  idle: "bg-mid-gray/15 text-text/70 border-mid-gray/20",
  starting: "bg-accent/15 text-text border-accent/30",
  running: "bg-green-500/15 text-green-500 border-green-500/30",
  stopping: "bg-accent/15 text-text border-accent/30",
  error: "bg-red-500/15 text-red-400 border-red-500/30",
};

const formatUs = (us: number): string =>
  us >= 1000 ? `${(us / 1000).toFixed(2)} ms` : `${us.toFixed(0)} µs`;

/** A level in dB, or a dash at the features' silence floor (−120). */
const formatDb = (db: number): string =>
  Number.isFinite(db) && db > -119.5 ? `${db.toFixed(1)} dB` : "—";

const formatHzPerBin = (hz: number): string =>
  hz >= 100 ? `${hz.toFixed(0)} Hz` : `${hz.toFixed(hz >= 10 ? 1 : 2)} Hz`;

const opt = (
  value: string,
  label: string,
  description?: string,
): DropdownOption => ({
  value,
  label,
  description,
});

const handleStop = async () => {
  const error = await stop();
  if (error) toast.error(error);
};

export const LiveFftSettings = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const store = useLiveFftStore();
  const status = () => store.status;
  const active = () => isFftActive(status());
  const phase = () => status().phase;

  const options = createMemo<ResolvedLiveFft>(() =>
    resolveLiveFft(getSetting("live_fft")),
  );

  const [draft, setDraft] = createSignal<ResolvedLiveFft>(options());
  let saveTimer: number | null = null;
  createEffect(
    () => options(),
    (next) => {
      if (saveTimer === null) setDraft(next);
    },
  );

  const save = (patch: Partial<ResolvedLiveFft>) => {
    const next = { ...draft(), ...patch };
    setDraft(next);
    if (saveTimer !== null) window.clearTimeout(saveTimer);
    saveTimer = window.setTimeout(() => {
      saveTimer = null;
      void updateSetting("live_fft", draft());
    }, SAVE_DEBOUNCE_MS);
  };
  createEffect(
    () => undefined,
    () => () => {
      if (saveTimer !== null) {
        window.clearTimeout(saveTimer);
        void updateSetting("live_fft", draft());
      }
    },
  );

  const [view, setView] = createSignal<ViewPrefs>(readView());
  const updateView = (patch: Partial<ViewPrefs>) =>
    setView((prev) => {
      const next = { ...prev, ...patch };
      writeView(next);
      return next;
    });

  createEffect(
    () => undefined,
    () => {
      void initialize();
    },
  );

  createEffect(
    () => undefined,
    () => () => {
      if (active()) void commands.liveFftStop();
    },
  );

  const [readout, setReadout] = createSignal<{
    peakHz: number;
    peakValue: number;
    silent: boolean;
    stalled: boolean;
    dspUs: number;
    features: SpectralFeatures | null;
  }>({
    peakHz: 0,
    peakValue: 0,
    silent: false,
    stalled: false,
    dspUs: 0,
    features: null,
  });
  createEffect(
    () => active(),
    (isActive) => {
      if (!isActive) return;
      const id = window.setInterval(() => {
        const frame = getLatestFrame();
        // A frozen view pauses the poll on purpose: not a stall.
        const stalled =
          !store.frozen && Date.now() - getLastFrameAt() > STALL_MS;
        setReadout({
          peakHz: frame?.peakHz ?? 0,
          peakValue: frame?.peakValue ?? 0,
          silent: frame?.silent ?? false,
          stalled,
          dspUs: frame?.dspUs ?? 0,
          features: frame?.features ?? null,
        });
      }, READOUT_INTERVAL_MS);
      return () => window.clearInterval(id);
    },
  );
  const [hover, setHover] = createSignal<HoverInfo | null>(null);

  const handleStart = async () => {
    const error = await start();
    if (error) toast.error(t("settings.liveFft.errors.start", { error }));
  };

  // A plain invoke: a backend failure rejects instead of returning a result.
  const applyRaw = async () => {
    try {
      const defaults = await commands.liveFftRawDefaults();
      save(applyPreset(draft(), resolveLiveFft(defaults)));
      toast.success(t("settings.liveFft.presets.applied"));
    } catch (error) {
      toast.error(String(error));
    }
  };

  // Replaced only when the backend's axis version moves (a warp, bin count
  // or rate change), so the canvases repaint their grid only then.
  const axisHz = () => store.axis.hz;
  const frameRate = () =>
    status().update_rate_hz > 0
      ? status().update_rate_hz
      : draft().update_rate_hz;
  const canvasLabels = createMemo(() => ({
    idle: t("settings.liveFft.canvas.idle"),
    silence: t("settings.liveFft.canvas.silence"),
  }));
  // A string memo: the ~1 Hz status heartbeat recomputes it, but an equal
  // string stops there instead of repainting the waterfall.
  const spanLabel = createMemo(() =>
    t("settings.liveFft.canvas.span", {
      seconds: (WATERFALL_ROWS / Math.max(1, frameRate())).toFixed(1),
    }),
  );
  const waterfallLabels = createMemo(() => ({
    now: t("settings.liveFft.canvas.now"),
    span: spanLabel(),
  }));

  const P = "settings.liveFft";
  const sourceOptions = createMemo<DropdownOption[]>(() => [
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
  ]);
  const scaleOptions = createMemo<DropdownOption[]>(() =>
    (
      ["log", "mel", "erb", "bark", "chroma", "linear", "melog"] as FftScale[]
    ).map((v) => opt(v, t(`${P}.spectrum.scale.options.${v}`))),
  );
  const interpOptions = createMemo<DropdownOption[]>(() => [
    opt("linear", t(`${P}.spectrum.warpInterp.linear`)),
    opt("cubic", t(`${P}.spectrum.warpInterp.cubic`)),
  ]);
  const windowModeOptions = createMemo<DropdownOption[]>(() => [
    opt("samples", t(`${P}.spectrum.windowMode.samples`)),
    opt("milliseconds", t(`${P}.spectrum.windowMode.milliseconds`)),
  ]);
  const binOptions = createMemo<DropdownOption[]>(() =>
    OUTPUT_BIN_CHOICES.map((n) => opt(String(n), String(n))),
  );
  const fftSizeOptions = createMemo<DropdownOption[]>(() =>
    FFT_SIZES.map((n) => opt(String(n), `${n / 1024}K`)),
  );
  const windowTypeOptions = createMemo<DropdownOption[]>(() =>
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
  );
  const weightingOptions = createMemo<DropdownOption[]>(() =>
    (["off", "a", "c", "itu468"] as FftWeighting[]).map((v) =>
      opt(v, t(`${P}.window.weighting.options.${v}`)),
    ),
  );
  const normOptions = createMemo<DropdownOption[]>(() =>
    (["coherent_gain", "full_scale"] as FftMagnitudeNorm[]).map((v) =>
      opt(v, t(`${P}.window.magnitudeNorm.options.${v}`)),
    ),
  );
  const loudnessOptions = createMemo<DropdownOption[]>(() =>
    (["off", "db", "db_normalized"] as FftLoudnessMode[]).map((v) =>
      opt(v, t(`${P}.loudness.mode.options.${v}`)),
    ),
  );
  const dbRefOptions = createMemo<DropdownOption[]>(() =>
    (["frame_peak", "dbfs", "agc"] as FftDbReference[]).map((v) =>
      opt(v, t(`${P}.loudness.dbReference.options.${v}`)),
    ),
  );
  const ballModeOptions = createMemo<DropdownOption[]>(() => [
    opt("coefficient", t(`${P}.loudness.ballisticsMode.coefficient`)),
    opt("milliseconds", t(`${P}.loudness.ballisticsMode.milliseconds`)),
  ]);
  const binsModeOptions = createMemo<DropdownOption[]>(() => [
    opt("fixed", t(`${P}.spectrum.outputBinsMode.fixed`)),
    opt("auto", t(`${P}.spectrum.outputBinsMode.auto`)),
  ]);
  const aggregationOptions = createMemo<DropdownOption[]>(() =>
    (["off", "peak", "rms"] as FftWarpAggregation[]).map((v) =>
      opt(v, t(`${P}.spectrum.warpAggregation.options.${v}`)),
    ),
  );
  const betaModeOptions = createMemo<DropdownOption[]>(() => [
    opt("manual", t(`${P}.window.kaiserBetaMode.manual`)),
    opt("auto", t(`${P}.window.kaiserBetaMode.auto`)),
  ]);
  const styleOptions = createMemo<DropdownOption[]>(() => [
    opt("bars", t(`${P}.view.bars`)),
    opt("line", t(`${P}.view.line`)),
    opt("area", t(`${P}.view.area`)),
  ]);
  const colormapOptions = createMemo<DropdownOption[]>(() => [
    opt("inferno", t(`${P}.view.colormap.inferno`)),
    opt("accent", t(`${P}.view.colormap.accent`)),
    opt("ice", t(`${P}.view.colormap.ice`)),
  ]);

  const nyquist = () => status().nyquist_hz ?? 0;
  const resolutionHz = () => {
    const s = status();
    return s.sample_rate > 0 && s.window_samples > 0
      ? s.sample_rate / s.window_samples
      : 0;
  };
  const peakNote = () =>
    readout().peakHz > 0 ? noteName(readout().peakHz) : null;
  const cursorText = createMemo(() => {
    const h = hover();
    return h
      ? `${formatHz(h.hz)} ${formatValue(h.value, draft().loudness_mode)}`
      : "—";
  });
  const busy = () => isUpdating("live_fft");

  // Enable states, as Plugin_FFT greys them out (catalog §1.7). Raw bins
  // fixes the axis, so every axis-shaping control stands down with it.
  const raw = () => draft().raw_bins;
  const binsAuto = () => draft().output_bins_mode === "auto";
  const isKaiser = () => draft().window_type === "kaiser";
  const betaAuto = () => draft().kaiser_beta_mode === "auto";
  const loud = () => draft().loudness_mode !== "off";
  const qualityMatch = () => matchingQualityPreset(draft());
  const hasTelemetry = () => active() && status().sample_rate > 0;

  return (
    <div class="max-w-3xl w-full mx-auto space-y-6 pb-8">
      <SettingsGroup
        title={t(`${P}.title`)}
        description={t(`${P}.description`)}
      >
        <div class="p-3 space-y-3">
          {phase() === "error" && status().error && (
            <Alert variant="error">{status().error}</Alert>
          )}
          {!active() && status().stop_reason && (
            <Alert variant="warning">
              {t(`${P}.stopReason.${status().stop_reason}`)}
            </Alert>
          )}
          <div class="flex flex-wrap items-center gap-3 rounded-lg border border-mid-gray/20 bg-background p-3">
            <div
              class={`p-3 rounded-full ${active() ? "bg-green-500/15" : "bg-mid-gray/10"}`}
            >
              {phase() === "running" ? (
                <Activity class="w-6 h-6 text-green-500" />
              ) : active() ? (
                <Loader2 class="w-6 h-6 text-accent animate-spin" />
              ) : (
                <AudioLines class="w-6 h-6 text-mid-gray" />
              )}
            </div>
            <div class="flex-1 min-w-[200px]">
              <div class="flex flex-wrap items-center gap-2">
                <span
                  class={`inline-flex items-center px-2 py-0.5 rounded text-[11px] font-medium border ${PHASE_CLASSES[phase()]}`}
                >
                  {t(`${P}.status.${phase()}`)}
                </span>
                {active() && readout().stalled && (
                  <span class="inline-flex items-center px-2 py-0.5 rounded text-[11px] font-medium border bg-amber-500/10 text-amber-400 border-amber-500/20">
                    {t(`${P}.noSignal`)}
                  </span>
                )}
                {active() && (
                  <span class="text-xs text-mid-gray">
                    {t(`${P}.telemetry.threading`, {
                      mode: status().async_analysis
                        ? t(`${P}.telemetry.async`)
                        : t(`${P}.telemetry.inline`),
                    })}
                  </span>
                )}
              </div>
              <dl class="mt-2 grid grid-cols-3 sm:grid-cols-6 gap-x-3 gap-y-1 text-xs">
                <div>
                  <dt class="text-mid-gray">
                    {t(`${P}.telemetry.sampleRate`)}
                  </dt>
                  <dd class="font-mono">
                    {status().sample_rate > 0 ? `${status().sample_rate}` : "—"}
                  </dd>
                </div>
                <div>
                  <dt class="text-mid-gray">{t(`${P}.telemetry.fftSize`)}</dt>
                  <dd class="font-mono">
                    {status().fft_size > 0 ? `${status().fft_size}` : "—"}
                  </dd>
                </div>
                <div>
                  <dt class="text-mid-gray">{t(`${P}.telemetry.window`)}</dt>
                  <dd class="font-mono">
                    {status().window_samples > 0
                      ? `${status().window_samples}`
                      : "—"}
                  </dd>
                </div>
                <div>
                  <dt class="text-mid-gray">
                    {t(`${P}.telemetry.resolution`)}
                  </dt>
                  <dd class="font-mono">
                    {resolutionHz() > 0
                      ? `${resolutionHz().toFixed(1)} Hz`
                      : "—"}
                  </dd>
                </div>
                <div>
                  <dt class="text-mid-gray">{t(`${P}.telemetry.dsp`)}</dt>
                  <dd class="font-mono">
                    {active() && status().dsp_us_avg
                      ? formatUs(status().dsp_us_avg ?? 0)
                      : "—"}
                  </dd>
                </div>
                <div>
                  <dt class="text-mid-gray">{t(`${P}.telemetry.dropped`)}</dt>
                  <dd class="font-mono">
                    {active() ? `${status().dropped_samples}` : "—"}
                  </dd>
                </div>
                <div>
                  <dt class="text-mid-gray">{t(`${P}.telemetry.hzPerBin`)}</dt>
                  <dd class="font-mono">
                    {hasTelemetry() && (status().hz_per_bin ?? 0) > 0
                      ? formatHzPerBin(status().hz_per_bin ?? 0)
                      : "—"}
                  </dd>
                </div>
                <div>
                  <dt class="text-mid-gray">
                    {t(`${P}.telemetry.magnitudeBins`)}
                  </dt>
                  <dd class="font-mono">
                    {hasTelemetry()
                      ? t(`${P}.telemetry.magnitudeBinsValue`, {
                          used: status().magnitude_bins,
                          total: status().linear_bins,
                        })
                      : "—"}
                  </dd>
                </div>
                <div>
                  <dt class="text-mid-gray">
                    {t(`${P}.telemetry.aggregated`)}
                  </dt>
                  <dd class="font-mono">
                    {hasTelemetry()
                      ? `${status().aggregated_bins} / ${status().output_bins}`
                      : "—"}
                  </dd>
                </div>
                <div>
                  <dt class="text-mid-gray">
                    {t(`${P}.telemetry.kaiserBeta`)}
                  </dt>
                  <dd class="font-mono">
                    {hasTelemetry() && (status().kaiser_beta ?? 0) > 0
                      ? (status().kaiser_beta ?? 0).toFixed(2)
                      : "—"}
                  </dd>
                </div>
                <div>
                  <dt class="text-mid-gray">{t(`${P}.telemetry.latency`)}</dt>
                  <dd class="font-mono">
                    {hasTelemetry() && (status().analysis_latency_ms ?? 0) > 0
                      ? `${(status().analysis_latency_ms ?? 0).toFixed(1)} ms`
                      : "—"}
                  </dd>
                </div>
              </dl>
            </div>
            <div class="flex items-center gap-2">
              {active() && (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setFrozen(!store.frozen)}
                  title={store.frozen ? t(`${P}.unfreeze`) : t(`${P}.freeze`)}
                >
                  <span class="inline-flex items-center gap-1.5">
                    {store.frozen ? (
                      <Play class="w-4 h-4" />
                    ) : (
                      <Pause class="w-4 h-4" />
                    )}
                    {store.frozen ? t(`${P}.unfreeze`) : t(`${P}.freeze`)}
                  </span>
                </Button>
              )}
              {active() ? (
                <Button
                  variant="danger"
                  onClick={handleStop}
                  disabled={phase() === "stopping"}
                >
                  <span class="inline-flex items-center gap-1.5">
                    <Square class="w-4 h-4" />
                    {t(`${P}.stop`)}
                  </span>
                </Button>
              ) : (
                <Button variant="primary" onClick={handleStart}>
                  <span class="inline-flex items-center gap-1.5">
                    <AudioLines class="w-4 h-4" />
                    {t(`${P}.start`)}
                  </span>
                </Button>
              )}
            </div>
          </div>
          <p class="text-xs text-text/50">{t(`${P}.hotkeysNote`)}</p>
        </div>
      </SettingsGroup>

      <VoiceDetectionGroup
        enabled={draft().show_vad}
        active={active()}
        busy={busy()}
        onToggle={(checked) => save({ show_vad: checked })}
        source={draft().source}
        onSource={(source) => save({ source })}
      />

      <SettingsGroup title={t(`${P}.view.title`)}>
        <div class="p-3 space-y-2">
          <div class="flex flex-wrap items-center gap-2 text-xs">
            <Dropdown
              options={styleOptions()}
              selectedValue={view().style}
              onSelect={(v) => updateView({ style: v as SpectrumStyle })}
              class="min-w-[110px]"
            />
            <ToggleChip
              active={view().peakHold}
              onClick={() => updateView({ peakHold: !view().peakHold })}
              label={t(`${P}.view.peakHold`)}
            />
            <ToggleChip
              active={view().grid}
              onClick={() => updateView({ grid: !view().grid })}
              label={t(`${P}.view.grid`)}
            />
            <ToggleChip
              active={view().waterfall}
              onClick={() => updateView({ waterfall: !view().waterfall })}
              label={t(`${P}.view.waterfall`)}
            />
            {view().waterfall && (
              <Dropdown
                options={colormapOptions()}
                selectedValue={view().colormap}
                onSelect={(v) => updateView({ colormap: v as ColormapKind })}
                class="min-w-[120px]"
              />
            )}
          </div>
          <div class="rounded-lg border border-mid-gray/20 bg-background overflow-hidden">
            <SpectrumCanvas
              axisHz={axisHz()}
              mode={draft().loudness_mode}
              dbRange={draft().db_range}
              style={view().style}
              peakHold={view().peakHold}
              grid={view().grid}
              running={active()}
              labels={canvasLabels()}
              onHover={setHover}
              class="h-64"
            />
            {view().waterfall && (
              <SpectrogramCanvas
                axisHz={axisHz()}
                mode={draft().loudness_mode}
                dbRange={draft().db_range}
                colormap={view().colormap}
                running={active()}
                labels={waterfallLabels()}
                class="h-36 border-t border-mid-gray/15"
              />
            )}
          </div>
          <div class="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-text/70 font-mono min-h-[18px]">
            <span>
              {t(`${P}.view.peak`)}{" "}
              {active() && !readout().silent && readout().peakHz > 0
                ? `${formatHz(readout().peakHz)}${peakNote() ? ` (${peakNote()})` : ""} ${formatValue(readout().peakValue, draft().loudness_mode)}`
                : "—"}
            </span>
            <span>
              {t(`${P}.view.cursor`)} {cursorText()}
            </span>
            {active() && (
              <span class="text-text/50">
                {t(`${P}.telemetry.frame`, {
                  bins: status().output_bins,
                  rate: status().update_rate_hz,
                  dsp: formatUs(readout().dspUs),
                })}
              </span>
            )}
          </div>
          {draft().spectral_features && (
            <FeaturesReadout
              features={active() ? readout().features : null}
              title={t(`${P}.features.title`)}
              label={(key) => t(`${P}.features.${key}`)}
            />
          )}
        </div>
      </SettingsGroup>

      <SettingsGroup
        title={t(`${P}.presets.title`)}
        description={t(`${P}.presets.description`)}
      >
        <div class="p-3 flex flex-wrap gap-2">
          <For each={LIVE_FFT_PRESETS}>
            {(preset) => (
              <Button
                variant="secondary"
                size="sm"
                disabled={busy()}
                onClick={() => {
                  save(applyPreset(draft(), preset.patch));
                  toast.success(t(`${P}.presets.applied`));
                }}
              >
                {t(`${P}.presets.${preset.id}`)}
              </Button>
            )}
          </For>
          <Button
            variant="secondary"
            size="sm"
            disabled={busy()}
            onClick={applyRaw}
          >
            {t(`${P}.presets.raw`)}
          </Button>
        </div>
        <div class="px-3 pb-3 space-y-2">
          <div class="flex flex-wrap items-baseline gap-x-2 gap-y-1">
            <h4 class="text-xs font-medium text-text/80">
              {t(`${P}.presets.quality.title`)}
            </h4>
            <p class="text-xs text-text/50">
              {t(`${P}.presets.quality.description`)}
            </p>
          </div>
          <div class="flex flex-wrap items-center gap-2">
            <For each={LIVE_FFT_QUALITY_PRESETS}>
              {(preset) => (
                <Button
                  variant={
                    qualityMatch() === preset.id ? "primary-soft" : "secondary"
                  }
                  size="sm"
                  disabled={busy()}
                  onClick={() => {
                    save(applyQualityPreset(draft(), preset.patch));
                    toast.success(t(`${P}.presets.applied`));
                  }}
                >
                  {t(`${P}.presets.quality.${preset.id}`)}
                </Button>
              )}
            </For>
            {qualityMatch() && (
              <span class="text-xs text-text/50">
                {t(`${P}.presets.quality.active`, {
                  preset: t(`${P}.presets.quality.${qualityMatch()}`),
                })}
              </span>
            )}
          </div>
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
            options={sourceOptions()}
            selectedValue={draft().source}
            onSelect={(v) => save({ source: v as FftSource })}
            class="min-w-[220px]"
          />
        </SettingContainer>
        <ToggleSwitch
          checked={draft().raw_bins}
          onChange={(checked) => save({ raw_bins: checked })}
          label={t(`${P}.spectrum.rawBins.label`)}
          description={t(`${P}.spectrum.rawBins.description`)}
          descriptionMode="tooltip"
          grouped
        />
        <SettingContainer
          title={t(`${P}.spectrum.scale.label`)}
          description={t(`${P}.spectrum.scale.description`)}
          descriptionMode="tooltip"
          grouped
          layout="horizontal"
          disabled={raw()}
        >
          <Dropdown
            options={scaleOptions()}
            selectedValue={draft().scale}
            onSelect={(v) => save({ scale: v as FftScale })}
            disabled={raw()}
            class="min-w-[200px]"
          />
        </SettingContainer>
        <SettingContainer
          title={t(`${P}.spectrum.warpInterp.label`)}
          description={t(`${P}.spectrum.warpInterp.description`)}
          descriptionMode="tooltip"
          grouped
          layout="horizontal"
          disabled={raw()}
        >
          <Dropdown
            options={interpOptions()}
            selectedValue={draft().warp_interpolation}
            onSelect={(v) => save({ warp_interpolation: v as FftWarpInterp })}
            disabled={raw()}
            class="min-w-[200px]"
          />
        </SettingContainer>
        <SettingContainer
          title={t(`${P}.spectrum.warpAggregation.label`)}
          description={t(`${P}.spectrum.warpAggregation.description`)}
          descriptionMode="tooltip"
          grouped
          layout="horizontal"
          disabled={raw()}
        >
          <Dropdown
            options={aggregationOptions()}
            selectedValue={draft().warp_aggregation}
            onSelect={(v) =>
              save({ warp_aggregation: v as FftWarpAggregation })
            }
            disabled={raw()}
            class="min-w-[200px]"
          />
        </SettingContainer>
        <ParamSlider
          label={t(`${P}.spectrum.displayMax.label`)}
          description={
            nyquist() > 0
              ? `${t(`${P}.spectrum.displayMax.description`)} ${t(`${P}.spectrum.nyquistNote`, { hz: formatHz(nyquist()) })}`
              : t(`${P}.spectrum.displayMax.description`)
          }
          value={draft().display_max_hz}
          min={100}
          max={192000}
          step={10}
          log
          unit="Hz"
          disabled={raw()}
          defaultValue={LIVE_FFT_DEFAULTS.display_max_hz}
          onChange={(v) => save({ display_max_hz: Math.round(v) })}
        />
        <SettingContainer
          title={t(`${P}.spectrum.outputBinsMode.label`)}
          description={t(`${P}.spectrum.outputBinsMode.description`)}
          descriptionMode="tooltip"
          grouped
          layout="horizontal"
          disabled={raw()}
        >
          <Dropdown
            options={binsModeOptions()}
            selectedValue={draft().output_bins_mode}
            onSelect={(v) => save({ output_bins_mode: v as FftOutputBinsMode })}
            disabled={raw()}
            class="min-w-[160px]"
          />
        </SettingContainer>
        <SettingContainer
          title={t(`${P}.spectrum.outputBins.label`)}
          description={t(`${P}.spectrum.outputBins.description`)}
          descriptionMode="tooltip"
          grouped
          layout="horizontal"
          disabled={raw() || binsAuto()}
        >
          <Dropdown
            options={binOptions()}
            selectedValue={String(draft().output_bins)}
            onSelect={(v) => save({ output_bins: Number(v) })}
            disabled={raw() || binsAuto()}
            class="min-w-[110px]"
          />
        </SettingContainer>
        <ParamSlider
          label={t(`${P}.spectrum.warpBlend.label`)}
          description={t(`${P}.spectrum.warpBlend.description`)}
          value={draft().warp_blend}
          min={0}
          max={1}
          step={0.001}
          disabled={raw()}
          defaultValue={LIVE_FFT_DEFAULTS.warp_blend}
          onChange={(v) => save({ warp_blend: v })}
        />
        <ParamSlider
          label={t(`${P}.spectrum.logFloor.label`)}
          description={t(`${P}.spectrum.logFloor.description`)}
          value={draft().log_floor_hz}
          min={1}
          max={5000}
          step={1}
          log
          unit="Hz"
          disabled={raw()}
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
            options={windowModeOptions()}
            selectedValue={draft().window_length_mode}
            onSelect={(v) =>
              save({ window_length_mode: v as FftWindowLengthMode })
            }
            class="min-w-[200px]"
          />
        </SettingContainer>
        {draft().window_length_mode === "samples" ? (
          <ParamSlider
            label={t(`${P}.spectrum.windowSamples.label`)}
            description={t(`${P}.spectrum.windowSamples.description`)}
            value={draft().window_samples}
            min={1}
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
            value={draft().window_ms}
            min={0.1}
            max={5000}
            step={0.1}
            log
            unit="ms"
            defaultValue={LIVE_FFT_DEFAULTS.window_ms}
            onChange={(v) =>
              save({ window_ms: Math.max(0.1, Math.round(v * 10) / 10) })
            }
          />
        )}
        <ToggleSwitch
          checked={draft().zero_padding}
          onChange={(checked) => save({ zero_padding: checked })}
          label={t(`${P}.spectrum.zeroPadding.label`)}
          description={t(`${P}.spectrum.zeroPadding.description`)}
          descriptionMode="tooltip"
          grouped
        />
        <SettingContainer
          title={t(`${P}.spectrum.fftSize.label`)}
          description={t(`${P}.spectrum.fftSize.description`)}
          descriptionMode="tooltip"
          grouped
          layout="horizontal"
          disabled={!draft().zero_padding}
        >
          <Dropdown
            options={fftSizeOptions()}
            selectedValue={String(draft().fft_size)}
            onSelect={(v) => save({ fft_size: Number(v) })}
            disabled={!draft().zero_padding}
            class="min-w-[110px]"
          />
        </SettingContainer>
      </SettingsGroup>

      <SettingsGroup
        title={t(`${P}.eq.title`)}
        description={t(`${P}.eq.description`)}
      >
        <ToggleSwitch
          checked={draft().eq_enabled}
          onChange={(checked) => save({ eq_enabled: checked })}
          label={t(`${P}.eq.enable.label`)}
          description={t(`${P}.eq.enable.description`)}
          descriptionMode="tooltip"
          grouped
        />
        {draft().eq_enabled && (
          <>
            <ToggleSwitch
              checked={draft().high_shelf}
              onChange={(checked) => save({ high_shelf: checked })}
              label={t(`${P}.eq.highShelf.label`)}
              description={t(`${P}.eq.highShelf.description`)}
              descriptionMode="tooltip"
              grouped
            />
            <ParamSlider
              label={t(`${P}.eq.highGain.label`)}
              description={t(`${P}.eq.highGain.description`)}
              value={draft().high_gain_db}
              min={-24}
              max={24}
              step={0.1}
              unit="dB"
              disabled={!draft().high_shelf}
              defaultValue={LIVE_FFT_DEFAULTS.high_gain_db}
              onChange={(v) => save({ high_gain_db: v })}
            />
            <ParamSlider
              label={t(`${P}.eq.highCutoff.label`)}
              description={t(`${P}.eq.highCutoff.description`)}
              value={draft().high_cutoff_hz}
              min={20}
              max={20000}
              step={1}
              log
              unit="Hz"
              disabled={!draft().high_shelf}
              defaultValue={LIVE_FFT_DEFAULTS.high_cutoff_hz}
              onChange={(v) => save({ high_cutoff_hz: Math.round(v) })}
            />
            <ToggleSwitch
              checked={draft().low_shelf}
              onChange={(checked) => save({ low_shelf: checked })}
              label={t(`${P}.eq.lowShelf.label`)}
              description={t(`${P}.eq.lowShelf.description`)}
              descriptionMode="tooltip"
              grouped
            />
            <ParamSlider
              label={t(`${P}.eq.lowGain.label`)}
              description={t(`${P}.eq.lowGain.description`)}
              value={draft().low_gain_db}
              min={-24}
              max={24}
              step={0.1}
              unit="dB"
              disabled={!draft().low_shelf}
              defaultValue={LIVE_FFT_DEFAULTS.low_gain_db}
              onChange={(v) => save({ low_gain_db: v })}
            />
            <ParamSlider
              label={t(`${P}.eq.lowCutoff.label`)}
              description={t(`${P}.eq.lowCutoff.description`)}
              value={draft().low_cutoff_hz}
              min={20}
              max={5000}
              step={1}
              log
              unit="Hz"
              disabled={!draft().low_shelf}
              defaultValue={LIVE_FFT_DEFAULTS.low_cutoff_hz}
              onChange={(v) => save({ low_cutoff_hz: Math.round(v) })}
            />
            <ParamSlider
              label={t(`${P}.eq.q.label`)}
              description={t(`${P}.eq.q.description`)}
              value={draft().eq_q}
              min={0.1}
              max={4}
              step={0.001}
              defaultValue={LIVE_FFT_DEFAULTS.eq_q}
              onChange={(v) => save({ eq_q: v })}
            />
            <ParamSlider
              label={t(`${P}.eq.amount.label`)}
              description={t(`${P}.eq.amount.description`)}
              value={draft().eq_amount}
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
            options={windowTypeOptions()}
            selectedValue={draft().window_type}
            onSelect={(v) => save({ window_type: v as FftWindowType })}
            class="min-w-[200px]"
          />
        </SettingContainer>
        {isKaiser() && (
          <>
            <SettingContainer
              title={t(`${P}.window.kaiserBetaMode.label`)}
              description={t(`${P}.window.kaiserBetaMode.description`)}
              descriptionMode="tooltip"
              grouped
              layout="horizontal"
            >
              <Dropdown
                options={betaModeOptions()}
                selectedValue={draft().kaiser_beta_mode}
                onSelect={(v) =>
                  save({ kaiser_beta_mode: v as FftKaiserBetaMode })
                }
                class="min-w-[200px]"
              />
            </SettingContainer>
            <ParamSlider
              label={t(`${P}.window.kaiserBeta.label`)}
              description={t(`${P}.window.kaiserBeta.description`)}
              value={draft().kaiser_beta}
              min={0}
              max={100}
              step={0.1}
              disabled={betaAuto()}
              defaultValue={LIVE_FFT_DEFAULTS.kaiser_beta}
              onChange={(v) => save({ kaiser_beta: v })}
            />
          </>
        )}
        <SettingContainer
          title={t(`${P}.window.weighting.label`)}
          description={t(`${P}.window.weighting.description`)}
          descriptionMode="tooltip"
          grouped
          layout="horizontal"
        >
          <Dropdown
            options={weightingOptions()}
            selectedValue={draft().weighting}
            onSelect={(v) => save({ weighting: v as FftWeighting })}
            class="min-w-[200px]"
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
            options={normOptions()}
            selectedValue={draft().magnitude_norm}
            onSelect={(v) => save({ magnitude_norm: v as FftMagnitudeNorm })}
            class="min-w-[200px]"
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
            options={loudnessOptions()}
            selectedValue={draft().loudness_mode}
            onSelect={(v) => save({ loudness_mode: v as FftLoudnessMode })}
            class="min-w-[200px]"
          />
        </SettingContainer>
        {loud() && (
          <SettingContainer
            title={t(`${P}.loudness.dbReference.label`)}
            description={t(`${P}.loudness.dbReference.description`)}
            descriptionMode="tooltip"
            grouped
            layout="horizontal"
          >
            <Dropdown
              options={dbRefOptions()}
              selectedValue={draft().db_reference}
              onSelect={(v) => save({ db_reference: v as FftDbReference })}
              class="min-w-[200px]"
            />
          </SettingContainer>
        )}
        {/* The range also sets an auto Kaiser beta, so it stays reachable
            with the linear mode when that is what it drives. */}
        {(loud() || (isKaiser() && betaAuto())) && (
          <ParamSlider
            label={t(`${P}.loudness.dbRange.label`)}
            description={t(`${P}.loudness.dbRange.description`)}
            value={draft().db_range}
            min={10}
            max={160}
            step={1}
            unit="dB"
            defaultValue={LIVE_FFT_DEFAULTS.db_range}
            onChange={(v) => save({ db_range: Math.round(v) })}
          />
        )}
        <ToggleSwitch
          checked={draft().ballistics_enabled}
          onChange={(checked) => save({ ballistics_enabled: checked })}
          label={t(`${P}.loudness.ballistics.label`)}
          description={t(`${P}.loudness.ballistics.description`)}
          descriptionMode="tooltip"
          grouped
        />
        {draft().ballistics_enabled && (
          <>
            <SettingContainer
              title={t(`${P}.loudness.ballisticsMode.label`)}
              description={t(`${P}.loudness.ballisticsMode.description`)}
              descriptionMode="tooltip"
              grouped
              layout="horizontal"
            >
              <Dropdown
                options={ballModeOptions()}
                selectedValue={draft().ballistics_mode}
                onSelect={(v) =>
                  save({ ballistics_mode: v as FftBallisticsMode })
                }
                class="min-w-[200px]"
              />
            </SettingContainer>
            {draft().ballistics_mode === "milliseconds" ? (
              <>
                <ParamSlider
                  label={t(`${P}.loudness.attackMs.label`)}
                  description={t(`${P}.loudness.attackMs.description`)}
                  value={draft().attack_ms}
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
                  value={draft().release_ms}
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
                  value={draft().attack}
                  min={0}
                  max={0.99}
                  step={0.01}
                  defaultValue={LIVE_FFT_DEFAULTS.attack}
                  onChange={(v) => save({ attack: v })}
                />
                <ParamSlider
                  label={t(`${P}.loudness.release.label`)}
                  description={t(`${P}.loudness.release.description`)}
                  value={draft().release}
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
            onClick={() => void reset()}
            disabled={!active()}
          >
            <span class="inline-flex items-center gap-1.5">
              <RotateCcw class="w-3.5 h-3.5" />
              {t(`${P}.loudness.reset.button`)}
            </span>
          </Button>
        </SettingContainer>
      </SettingsGroup>

      <SettingsGroup title={t(`${P}.performance.title`)}>
        <ToggleSwitch
          checked={draft().async_analysis}
          onChange={(checked) => save({ async_analysis: checked })}
          label={t(`${P}.performance.async.label`)}
          description={t(`${P}.performance.async.description`)}
          descriptionMode="tooltip"
          grouped
        />
        <ParamSlider
          label={t(`${P}.performance.updateRate.label`)}
          description={t(`${P}.performance.updateRate.description`)}
          value={draft().update_rate_hz}
          min={5}
          max={60}
          step={1}
          integer
          unit="Hz"
          defaultValue={LIVE_FFT_DEFAULTS.update_rate_hz}
          onChange={(v) => save({ update_rate_hz: Math.round(v) })}
        />
        <ToggleSwitch
          checked={draft().spectral_features}
          onChange={(checked) => save({ spectral_features: checked })}
          label={t(`${P}.performance.features.label`)}
          description={t(`${P}.performance.features.description`)}
          descriptionMode="tooltip"
          grouped
        />
        <div class="px-3 py-2">
          <p class="text-xs text-text/50">{t(`${P}.performance.note`)}</p>
        </div>
      </SettingsGroup>
    </div>
  );
};

type FeatureKey =
  | "centroid"
  | "rolloff"
  | "flatness"
  | "flux"
  | "rms"
  | "bass"
  | "mid"
  | "high";

const FEATURE_ROWS: {
  key: FeatureKey;
  format: (f: SpectralFeatures) => string;
}[] = [
  { key: "centroid", format: (f) => formatHz(f.centroidHz) },
  { key: "rolloff", format: (f) => formatHz(f.rolloffHz) },
  { key: "flatness", format: (f) => f.flatness.toFixed(3) },
  { key: "flux", format: (f) => f.flux.toFixed(3) },
  { key: "rms", format: (f) => formatDb(f.rmsDb) },
  { key: "bass", format: (f) => formatDb(f.bassDb) },
  { key: "mid", format: (f) => formatDb(f.midDb) },
  { key: "high", format: (f) => formatDb(f.highDb) },
];

interface FeaturesReadoutProps {
  features: SpectralFeatures | null;
  title: string;
  label: (key: FeatureKey) => string;
}

/** The eight spectral features of the latest frame (4 Hz readout). */
const FeaturesReadout = (props: FeaturesReadoutProps) => (
  <div class="rounded-lg border border-mid-gray/20 bg-background px-3 py-2">
    <div class="text-[11px] font-medium text-mid-gray mb-1">{props.title}</div>
    <dl class="grid grid-cols-4 sm:grid-cols-8 gap-x-3 gap-y-1 text-xs">
      <For each={FEATURE_ROWS}>
        {(row) => (
          <div>
            <dt class="text-mid-gray">{props.label(row.key)}</dt>
            <dd class="font-mono">
              {props.features ? row.format(props.features) : "—"}
            </dd>
          </div>
        )}
      </For>
    </dl>
  </div>
);

interface ToggleChipProps {
  active: boolean;
  onClick: () => void;
  label: string;
}

const ToggleChip = (props: ToggleChipProps) => (
  <button
    type="button"
    role="switch"
    aria-checked={props.active ? "true" : "false"}
    onClick={props.onClick}
    class={`px-2 py-1 rounded-md text-xs font-medium border transition-colors cursor-pointer ${props.active ? "bg-accent/20 text-text border-accent/40" : "bg-mid-gray/10 text-text/60 border-mid-gray/20 hover:bg-mid-gray/15"}`}
  >
    {props.label}
  </button>
);
