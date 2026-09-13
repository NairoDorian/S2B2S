/** @jsxImportSource @solidjs/web */
// ^ This file is Solid; see the note in `main.tsx` for why the pragma is
// per-file rather than tree-wide.
import { createEffect, createMemo } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import type { FftLoudnessMode } from "@/bindings";
import {
  cssColor,
  maxPerColumn,
  valueToUnit,
  type ValueScale,
} from "@/components/settings/live-fft/liveFftMath";
import type { ResolvedOverlayScope } from "@/lib/overlayScope";

// The recording overlay's miniature analyser: a spectrum and a line of the
// last N samples, both drawn from the same `overlay_scope_frame` poll. The
// backend runs the Live FFT page's pipeline with the page's settings
// (live_fft::scope), so the bins are the page's bins; `overlay_scope` (the
// Overlay settings page) decides which views are drawn, the spectrum style,
// mirroring, peak hold, the waveform window and its display gain. This file
// only maps values to pixels.
//
// Phase 2 of docs/PLAN_SOLIDJS_2.md made this Solid. Everything below the poll
// loop is framework-free and ports verbatim — the decode, the geometry, the
// canvas maths — because `live_fft/scope.rs`'s
// `scope_bins_equal_the_page_pipeline_for_the_same_settings` asserts the bins
// match the Live FFT page's, and the overlay's half of that contract is the
// `Float32Array` view mapping in `decode`. What changed is the loop's
// lifecycle, not its arithmetic.

/** Header words of a frame (live_fft::scope::encode_scope_frame). */
const HEADER_WORDS = 8;
const FLAG_SILENT = 1;
const FLAG_ACTIVE = 2;
const MODES: FftLoudnessMode[] = ["off", "db", "db_normalized"];
/** Linear auto-range: how fast the spectrum ceiling follows a quieter signal. */
const CEILING_DECAY = 0.995;
/** Waveform auto-gain: how fast its ceiling follows a quieter signal. */
const WAVE_CEILING_DECAY = 0.97;
/** Peak-hold fall per frame, in units of the drawing range (as on the page). */
const PEAK_DECAY = 0.0045;

interface ScopeFrame {
  seq: number;
  active: boolean;
  silent: boolean;
  mode: FftLoudnessMode;
  dbRange: number;
  bins: Float32Array;
  wave: Float32Array;
}

const decode = (buffer: ArrayBuffer): ScopeFrame | null => {
  if (buffer.byteLength < HEADER_WORDS * 4) return null;
  const words = new Uint32Array(buffer, 0, HEADER_WORDS);
  const floats = new Float32Array(buffer, 0, HEADER_WORDS);
  const binsLength = words[2];
  const waveLength = words[3];
  const binsOffset = HEADER_WORDS * 4;
  const waveOffset = binsOffset + binsLength * 4;
  if (buffer.byteLength < waveOffset + waveLength * 4) return null;
  return {
    seq: words[0],
    active: (words[1] & FLAG_ACTIVE) !== 0,
    silent: (words[1] & FLAG_SILENT) !== 0,
    mode: MODES[words[4]] ?? "db",
    dbRange: floats[5],
    bins: new Float32Array(buffer, binsOffset, binsLength),
    wave: new Float32Array(buffer, waveOffset, waveLength),
  };
};

interface Colors {
  accent: string;
  muted: string;
}

const readColors = (): Colors => ({
  accent: cssColor("--s-accent", "#1fe0ff"),
  muted: cssColor("--s-muted", "#8a9096"),
});

/** Size the backing store to the CSS box × DPR and clear it. */
const prepare = (canvas: HTMLCanvasElement, ctx: CanvasRenderingContext2D) => {
  const rect = canvas.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  const w = Math.max(1, Math.round(rect.width));
  const h = Math.max(1, Math.round(rect.height));
  if (
    canvas.width !== Math.round(w * dpr) ||
    canvas.height !== Math.round(h * dpr)
  ) {
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, w, h);
  return { w, h };
};

/**
 * Where a 0…1 value lands vertically. Upright: a baseline at the bottom and
 * the value rising from it. Mirrored: the value rises from the centre line
 * and its negative falls from it, so the spectrum reads like the centred
 * waveform beside it.
 */
interface Geometry {
  /** y of the baseline (bottom edge upright, centre line mirrored). */
  base: number;
  /** y of a value above the baseline. */
  top: (v: number) => number;
  /** y of the mirrored value, or the baseline when not mirroring. */
  bottom: (v: number) => number;
}

const geometry = (h: number, mirror: boolean): Geometry => {
  if (mirror) {
    const mid = h / 2;
    const half = Math.max(1, mid - 1);
    return {
      base: mid,
      top: (v) => mid - v * half,
      bottom: (v) => mid + v * half,
    };
  }
  const base = h - 1;
  const plotH = Math.max(1, h - 2);
  return { base, top: (v) => base - v * plotH, bottom: () => base };
};

interface OverlayScopeProps {
  /** Poll rate: the analyser's `update_rate_hz`. */
  rateHz: number;
  /** Microphone samples are flowing; before that the arming state is drawn. */
  ready: boolean;
  /** Nobody is speaking: drawn in the muted colour. */
  quiet: boolean;
  /** `overlay_scope`: which views, how they are drawn and sized. */
  config: ResolvedOverlayScope;
}

export function OverlayScope(props: OverlayScopeProps) {
  // Callback refs, not `useRef` boxes: a Solid `ref` is called once with the
  // element and never again — `RefCallback<T>` returns `void`, so there is no
  // unmount call to null these the way React's ref objects are nulled. Nothing
  // below depends on that: the guards ask the config what is being drawn (see
  // the early return in the effect), and a handle left pointing at a detached
  // canvas only ever paints into a canvas nobody can see.
  let spectrum: HTMLCanvasElement | undefined;
  let wave: HTMLCanvasElement | undefined;
  let circular: HTMLCanvasElement | undefined;

  // React's dependency array becomes the effect's compute phase, and the memo's
  // own equality gate is what makes it behave like one: a re-render that hands
  // down a new-but-equal `config` object re-runs the compute but not the apply,
  // so the loop is not torn down — and its ceiling, peak-hold and column state
  // not reset — by a parent update that changed nothing. (An effect with a bare
  // compute would re-run: `createEffect` applies no equality gate of its own.)
  const pollKey = createMemo(
    () =>
      `${props.rateHz}:${props.config.show_spectrum}:${props.config.show_wave}:${props.config.show_circular}`,
  );

  createEffect(pollKey, () => {
    const sctx = spectrum?.getContext("2d") ?? null;
    const wctx = wave?.getContext("2d") ?? null;
    const cctx = circular?.getContext("2d") ?? null;
    // All views off: nothing to draw, so nothing to poll for. The React
    // version reached this through two nulled refs; with refs that are never
    // nulled the question is asked of the setting that put the canvases there.
    if (
      !props.config.show_spectrum &&
      !props.config.show_wave &&
      !props.config.show_circular
    )
      return;

    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    let inFlight = false;
    let paintedKey = "";
    let colors = readColors();
    let colorsAt = 0;
    let ceiling = 1e-6;
    let waveCeiling = props.config.wave_gain_floor;
    let units = new Float32Array(0);
    let columns = new Float32Array(0);
    let held = new Float32Array(0);
    let heldSeq = -1;
    let columnsHeld = new Float32Array(0);
    let circPooled = new Float32Array(0);
    const period = Math.max(16, Math.round(1000 / Math.max(1, props.rateHz)));

    const paintSpectrum = (frame: ScopeFrame | null) => {
      if (!spectrum || !sctx) return;
      const { w, h } = prepare(spectrum, sctx);
      const color = props.quiet ? colors.muted : colors.accent;
      const g = geometry(h, props.config.spectrum_mirror);
      const baseline = Math.round(g.base) + 0.5;
      if (!frame || !props.ready) {
        sctx.globalAlpha = 0.35;
        sctx.strokeStyle = color;
        sctx.lineWidth = 1;
        sctx.beginPath();
        sctx.moveTo(0, baseline);
        sctx.lineTo(w, baseline);
        sctx.stroke();
        sctx.globalAlpha = 1;
        return;
      }
      const bins = frame.bins;
      const n = bins.length;
      if (units.length !== n) units = new Float32Array(n);
      const scale: ValueScale = {
        mode: frame.mode,
        dbRange: frame.dbRange,
        ceiling,
      };
      if (frame.mode === "off") {
        let max = 0;
        for (let i = 0; i < n; i++) if (bins[i] > max) max = bins[i];
        ceiling = Math.max(max, ceiling * CEILING_DECAY, 1e-6);
        scale.ceiling = ceiling;
      }
      for (let i = 0; i < n; i++) units[i] = valueToUnit(bins[i], scale);
      // Peak hold advances once per new frame, like the page's.
      if (held.length !== n) {
        held = new Float32Array(n);
        heldSeq = -1;
      }
      if (frame.seq !== heldSeq) {
        heldSeq = frame.seq;
        for (let i = 0; i < n; i++) {
          const fallen = held[i] - PEAK_DECAY;
          held[i] = units[i] > fallen ? units[i] : fallen;
        }
      }
      const cols = Math.min(n, Math.max(1, Math.floor(w)));
      if (columns.length !== cols) {
        columns = new Float32Array(cols);
        columnsHeld = new Float32Array(cols);
      }
      const values = n > cols ? maxPerColumn(units, cols, columns) : units;
      const heldValues =
        n > cols ? maxPerColumn(held, cols, columnsHeld) : held;
      const count = values.length;
      const colW = w / count;
      const mirror = props.config.spectrum_mirror;

      const tracePath = (edge: (v: number) => number) => {
        sctx.beginPath();
        for (let i = 0; i < count; i++) {
          const x = (i + 0.5) * colW;
          const y = edge(values[i]);
          if (i === 0) sctx.moveTo(x, y);
          else sctx.lineTo(x, y);
        }
      };

      if (props.config.spectrum_style === "bars") {
        sctx.fillStyle = color;
        const gap = colW > 3 ? 1 : 0;
        for (let i = 0; i < count; i++) {
          const y0 = g.top(values[i]);
          const y1 = g.bottom(values[i]);
          const barH = y1 - y0;
          if (barH <= 0) continue;
          sctx.globalAlpha = 0.35 + 0.65 * values[i];
          sctx.fillRect(i * colW, y0, Math.max(1, colW - gap), barH);
        }
        sctx.globalAlpha = 1;
      } else {
        if (props.config.spectrum_style === "area") {
          // Upright: the trace closed down to the baseline. Mirrored: the
          // band between the trace and its reflection.
          sctx.beginPath();
          sctx.moveTo(0, g.base);
          for (let i = 0; i < count; i++) {
            sctx.lineTo((i + 0.5) * colW, g.top(values[i]));
          }
          sctx.lineTo(w, g.base);
          if (mirror) {
            for (let i = count - 1; i >= 0; i--) {
              sctx.lineTo((i + 0.5) * colW, g.bottom(values[i]));
            }
          }
          sctx.closePath();
          sctx.globalAlpha = 0.28;
          sctx.fillStyle = color;
          sctx.fill();
          sctx.globalAlpha = 1;
        }
        sctx.strokeStyle = color;
        sctx.lineWidth = 1;
        sctx.lineJoin = "round";
        tracePath(g.top);
        sctx.stroke();
        if (mirror) {
          tracePath(g.bottom);
          sctx.stroke();
        }
      }

      if (props.config.peak_hold) {
        sctx.fillStyle = color;
        sctx.globalAlpha = 0.7;
        const markW = Math.max(1, colW - 1);
        for (let i = 0; i < count; i++) {
          const x = i * colW;
          sctx.fillRect(x, g.top(heldValues[i]) - 1, markW, 1.5);
          if (mirror) {
            sctx.fillRect(x, g.bottom(heldValues[i]) - 0.5, markW, 1.5);
          }
        }
        sctx.globalAlpha = 1;
      }
    };

    // The circular spectrum: the pipeline's bins peak-pooled to
    // `circular_bins`, mirrored about their centre and joined end-to-end
    // (`[p, reversed(p)]` — symmetric by construction), the two paths ±p
    // offset by +1 around a unit circle, the quarter arc rotated four times
    // into a seamless closed loop. Outer ring at radius 1+p, inner ring at
    // 1-p. The gain is deliberately a fixed scale — no dynamic
    // normalisation — so the loop breathes with the signal instead of
    // always filling the ring.
    const paintCircular = (frame: ScopeFrame | null) => {
      if (!circular || !cctx) return;
      const { w, h } = prepare(circular, cctx);
      const color = props.quiet ? colors.muted : colors.accent;
      const cx = w / 2;
      const cy = h / 2;
      // Unit-circle scale: the loudest possible loop (1+1) still fits the box.
      const S = (Math.min(w, h) / 2 - 1) / 2;
      const baseRing = () => {
        cctx.globalAlpha = 0.35;
        cctx.strokeStyle = color;
        cctx.lineWidth = 1;
        cctx.beginPath();
        cctx.arc(cx, cy, S, 0, Math.PI * 2);
        cctx.stroke();
        cctx.globalAlpha = 1;
      };
      if (!frame || !props.ready) {
        baseRing();
        return;
      }

      const cfg = props.config;
      const p = Math.max(1, Math.round(cfg.circular_bins));
      if (circPooled.length !== p) circPooled = new Float32Array(p);
      const scale: ValueScale = {
        mode: frame.mode,
        dbRange: frame.dbRange,
        ceiling: 1,
      };
      // Peak-pool the pipeline's bins down to the display count: each display
      // bin is the max of its bucket, so fewer bins means chunkier bars
      // rather than a lossy decimation.
      const bucket = frame.bins.length / p;
      for (let b = 0; b < p; b++) {
        const i0 = Math.floor(b * bucket);
        const i1 = Math.min(
          frame.bins.length,
          Math.max(i0 + 1, Math.floor((b + 1) * bucket)),
        );
        let m = 0;
        for (let i = i0; i < i1; i++) {
          if (frame.bins[i] > m) m = frame.bins[i];
        }
        let v: number;
        if (frame.mode === "off") {
          // Raw magnitudes: the fixed gain IS the scale. The linear view's
          // auto-ceiling is deliberately not used here — it would normalise
          // dynamically and the ring would always fill.
          v = m;
        } else {
          // dB modes are already fixed against 0 dBFS.
          v = valueToUnit(m, scale);
        }
        v = v < 0 ? 0 : v > 1 ? 1 : v;
        // The floor gates the ring BEFORE the gain, so the two tune
        // independently: the floor is a threshold in signal units (without
        // it the ambient room tone paints every angle and the loop reads as
        // a filled disc), the gain amplifies only what survives it — fixed
        // scaling, never a dynamic normalisation.
        v =
          v <= cfg.circular_floor
            ? 0
            : (v - cfg.circular_floor) / (1 - cfg.circular_floor);
        circPooled[b] = Math.min(1, v * cfg.circular_gain);
      }

      const D = p * 2;
      // combined[k]: the joined [p, reversed(p)] signal, so the loop is
      // symmetric about its centre and the four 90° rotations of the quarter
      // arc join without a seam.
      const combinedAt = (k: number) => circPooled[k < p ? k : D - 1 - k];
      const angleAt = (k: number) => (2 * Math.PI * k) / D - Math.PI / 2;

      if (cfg.circular_bars) {
        cctx.strokeStyle = color;
        cctx.lineCap = "round";
        cctx.lineWidth = Math.max(1, (2 * Math.PI * S) / D - 1);
        for (let k = 0; k < D; k++) {
          const c = combinedAt(k);
          if (c <= 0) continue;
          const th = angleAt(k);
          const co = Math.cos(th);
          const si = Math.sin(th);
          cctx.globalAlpha = 0.35 + 0.65 * c;
          cctx.beginPath();
          cctx.moveTo(cx + co * S * (1 - c), cy + si * S * (1 - c));
          cctx.lineTo(cx + co * S * (1 + c), cy + si * S * (1 + c));
          cctx.stroke();
        }
        cctx.globalAlpha = 1;
      } else {
        // The two loops as lines: inner (1-p) first, dimmer; outer (1+p) on top.
        const loop = (sign: number, alpha: number) => {
          cctx.globalAlpha = alpha;
          cctx.strokeStyle = color;
          cctx.lineWidth = 1;
          cctx.lineJoin = "round";
          cctx.beginPath();
          for (let k = 0; k < D; k++) {
            const r = S * (1 + sign * combinedAt(k));
            const th = angleAt(k);
            const x = cx + Math.cos(th) * r;
            const y = cy + Math.sin(th) * r;
            if (k === 0) cctx.moveTo(x, y);
            else cctx.lineTo(x, y);
          }
          cctx.closePath();
          cctx.stroke();
          cctx.globalAlpha = 1;
        };
        loop(-1, 0.55);
        loop(1, 1);
      }
    };

    const paintWave = (frame: ScopeFrame | null) => {
      if (!wave || !wctx) return;
      const { w, h } = prepare(wave, wctx);
      const color = props.quiet ? colors.muted : colors.accent;
      const mid = h / 2;
      wctx.strokeStyle = color;
      wctx.lineWidth = 1;
      wctx.lineJoin = "round";
      if (!frame || !props.ready || frame.wave.length < 2) {
        wctx.globalAlpha = 0.35;
        wctx.beginPath();
        wctx.moveTo(0, mid + 0.5);
        wctx.lineTo(w, mid + 0.5);
        wctx.stroke();
        wctx.globalAlpha = 1;
        return;
      }
      const samples = frame.wave;
      const n = samples.length;
      // Auto-gain: the trace fills the box for the loudest recent swing and
      // relaxes as the signal quiets, never amplifying below the floor.
      let peak = 0;
      for (let i = 0; i < n; i++) {
        const a = Math.abs(samples[i]);
        if (a > peak) peak = a;
      }
      waveCeiling = Math.max(
        peak,
        waveCeiling * WAVE_CEILING_DECAY,
        props.config.wave_gain_floor,
      );
      const gain = (mid - 1) / waveCeiling;
      const step = w / (n - 1);
      wctx.beginPath();
      wctx.moveTo(0, mid - samples[0] * gain);
      for (let i = 1; i < n; i++) {
        wctx.lineTo(i * step, mid - samples[i] * gain);
      }
      wctx.stroke();
    };

    const paint = (frame: ScopeFrame | null) => {
      const now = performance.now();
      if (now - colorsAt > 1000) {
        colors = readColors();
        colorsAt = now;
      }
      paintSpectrum(frame);
      paintWave(frame);
      paintCircular(frame);
    };

    const tick = async () => {
      if (disposed) return;
      if (!inFlight) {
        inFlight = true;
        try {
          const buffer = await invoke<ArrayBuffer>("overlay_scope_frame");
          if (disposed) return;
          const frame = decode(buffer);
          const live = frame?.active ? frame : null;
          const c = props.config;
          const key = `${live?.seq ?? -1}:${props.ready}:${props.quiet}:${c.spectrum_style}:${c.spectrum_mirror}:${c.peak_hold}:${c.wave_gain_floor}:${c.show_circular}:${c.circular_bars}:${c.circular_bins}:${c.circular_gain}:${c.circular_floor}`;
          if (key !== paintedKey) {
            paintedKey = key;
            paint(live);
          }
        } catch {
          // The command only fails while the app is shutting down.
        } finally {
          inFlight = false;
        }
      }
      if (!disposed) timer = setTimeout(tick, period);
    };

    paint(null);
    tick();

    // Returned, not registered with `onCleanup`: in Solid 2 the effect's
    // apply returns its own teardown, and it is the only way to tie it to this
    // run of the loop rather than to the component.
    return () => {
      disposed = true;
      if (timer !== null) clearTimeout(timer);
    };
  });

  return (
    <>
      {props.config.show_spectrum && (
        <canvas
          class="sscope sscope-fft"
          ref={(el: HTMLCanvasElement) => (spectrum = el)}
        />
      )}
      {props.config.show_wave && (
        <canvas
          class="sscope sscope-wave"
          ref={(el: HTMLCanvasElement) => (wave = el)}
        />
      )}
      {props.config.show_circular && (
        <canvas
          class="sscope sscope-circular"
          ref={(el: HTMLCanvasElement) => (circular = el)}
        />
      )}
    </>
  );
}

export default OverlayScope;
