import { createEffect, createMemo } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import type { FftLoudnessMode } from "@/bindings";
import {
  PEAK_HOLD_DECAY_PER_MS,
  cssColor,
  decayCeiling,
  decayStepMs,
  maxPerColumn,
  valueToUnit,
  type ValueScale,
} from "@/components/settings/live-fft/liveFftMath";
import type { ResolvedOverlayScope } from "@/lib/overlayScope";
import {
  circularAngleAt,
  circularPointCount,
  circularSignalAt,
  spectrumViewH,
  spectrumViewW,
  waveEnvelope,
  waveEnvelopeApplies,
  waveViewH,
  waveViewW,
} from "@/lib/overlayScope";

// The recording overlay's miniature analyser: a spectrum and a line of the
// last N samples, both drawn from the same `overlay_scope_frame` poll. The
// backend runs the Live FFT page's pipeline with the page's settings
// (live_fft::scope), so the bins are the page's bins; `overlay_scope` (the
// Overlay settings page) decides which views are drawn, the spectrum style,
// mirroring, peak hold, the waveform window and its display gain. This file
// only maps values to pixels.
//
// The React → Solid 2 migration (see CHANGELOG) made this Solid. Everything below the poll
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
/** Waveform auto-gain: how fast its ceiling follows a quieter signal. */
const WAVE_CEILING_DECAY = 0.97;

interface ScopeFrame {
  seq: number;
  active: boolean;
  silent: boolean;
  mode: FftLoudnessMode;
  dbRange: number;
  bins: Float32Array;
  wave: Float32Array;
  /** performance.now() when the reply carrying it arrived. */
  receivedAt: number;
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
    receivedAt: performance.now(),
  };
};

// ---- One shared poll --------------------------------------------------------
//
// The block view and the circular-background view (two OverlayScope
// instances) draw the same frame, so one poll serves both: each instance
// subscribes with its period and the feed polls at the shortest one, one
// request in flight. `knownSeq` makes the backend answer an unchanged frame
// with its 32-byte header alone (no copy of the bins and samples), which the
// feed reads as "nothing new" and answers with the frame it already holds.

type ScopeListener = (frame: ScopeFrame | null) => void;

const scopeFeed = {
  listeners: new Map<ScopeListener, number>(),
  timer: null as ReturnType<typeof setTimeout> | null,
  inFlight: false,
  knownSeq: null as number | null,
  last: null as ScopeFrame | null,
};

const feedPeriod = (): number => {
  let period = Infinity;
  for (const p of scopeFeed.listeners.values()) period = Math.min(period, p);
  return Number.isFinite(period) ? period : 1000;
};

const feedTick = async () => {
  scopeFeed.timer = null;
  if (scopeFeed.listeners.size === 0 || scopeFeed.inFlight) return;
  scopeFeed.inFlight = true;
  try {
    const known = scopeFeed.knownSeq;
    const buffer = await invoke<ArrayBuffer>("overlay_scope_frame", {
      knownSeq: known,
    });
    if (scopeFeed.listeners.size === 0) return;
    const seq =
      buffer.byteLength >= 4 ? new Uint32Array(buffer, 0, 1)[0] : undefined;
    if (known === null || seq !== known) {
      const frame = decode(buffer);
      scopeFeed.last = frame;
      scopeFeed.knownSeq = frame ? frame.seq : null;
    }
    for (const listener of scopeFeed.listeners.keys()) {
      listener(scopeFeed.last);
    }
  } catch {
    // The command only fails while the app is shutting down.
  } finally {
    scopeFeed.inFlight = false;
    if (scopeFeed.listeners.size > 0 && scopeFeed.timer === null) {
      scopeFeed.timer = setTimeout(feedTick, feedPeriod());
    }
  }
};

const subscribeScope = (listener: ScopeListener, period: number) => {
  scopeFeed.listeners.set(listener, period);
  if (scopeFeed.timer === null && !scopeFeed.inFlight) void feedTick();
  return () => {
    scopeFeed.listeners.delete(listener);
    if (scopeFeed.listeners.size === 0) {
      if (scopeFeed.timer !== null) clearTimeout(scopeFeed.timer);
      scopeFeed.timer = null;
      // A later recording restarts the scope's sequence: forget this one.
      scopeFeed.knownSeq = null;
      scopeFeed.last = null;
    }
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
  /**
   * Background variant: one full-window canvas behind the card that draws
   * only the circular spectrum. A mount-time constant — each usage site
   * passes a literal, so the body-level read here is a stable snapshot.
   */
  background?: boolean;
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
  const pollKey = createMemo(() => {
    const c = props.config;
    const circBlock = c.show_circular && !c.circular_background;
    return `${props.background ? "bg" : "block"}:${props.rateHz}:${
      c.show_spectrum
    }:${c.show_wave}:${circBlock}:${c.wave_inside_circular}`;
  });

  createEffect(pollKey, () => {
    const bg = props.background;
    const sctx = bg ? null : (spectrum?.getContext("2d") ?? null);
    const wctx = bg ? null : (wave?.getContext("2d") ?? null);
    const cctx =
      bg || (circular && props.config.show_circular)
        ? (circular?.getContext("2d") ?? null)
        : null;
    // All views off: nothing to draw, so nothing to poll for. The React
    // version reached this through two nulled refs; with refs that are never
    // nulled the question is asked of the setting that put the canvases there.
    if (!sctx && !wctx && !cctx) return;

    let disposed = false;
    let paintedKey = "";
    let colors = readColors();
    let colorsAt = 0;
    let ceiling = 1e-6;
    let waveCeiling = props.config.wave_gain_floor;
    let columns = new Float32Array(0);
    let held = new Float32Array(0);
    let heldSeq = -1;
    let heldAt = 0;
    let ceilingAt = 0;
    let columnsHeld = new Float32Array(0);
    let waveMin = new Float32Array(0);
    let waveMax = new Float32Array(0);
    // The intensity-scaled copies the spectrum paints from, reused across
    // frames like `columns` and reallocated only when the column count changes.
    let scaledValues = new Float32Array(0);
    let scaledHeld = new Float32Array(0);
    let circPooled = new Float32Array(0);
    const period = Math.max(16, Math.round(1000 / Math.max(1, props.rateHz)));

    // The 0…1 units every view draws from, computed once per frame. The
    // circular view shares this — it must move exactly like the linear
    // spectrum does — and shapes it with its own fixed floor and gain. (Its
    // earlier version scaled the pipeline's raw magnitudes directly: in the
    // linear loudness mode those are huge, so every bar clamped to full and
    // the ring never moved.)
    let unitsCache = new Float32Array(0);
    let unitsSeq = -1;
    const frameUnits = (frame: ScopeFrame): Float32Array => {
      const bins = frame.bins;
      const n = bins.length;
      if (unitsCache.length !== n) unitsCache = new Float32Array(n);
      if (frame.seq === unitsSeq) return unitsCache;
      unitsSeq = frame.seq;
      const scale: ValueScale = {
        mode: frame.mode,
        dbRange: frame.dbRange,
        ceiling,
      };
      if (frame.mode === "off") {
        // Per millisecond, like the page, so the range relaxes at the same
        // speed whatever the update rate.
        let max = 0;
        for (let i = 0; i < n; i++) if (bins[i] > max) max = bins[i];
        const dt = decayStepMs(frame.receivedAt, ceilingAt);
        ceiling = decayCeiling(ceiling, max, dt);
        ceilingAt = frame.receivedAt;
        scale.ceiling = ceiling;
      }
      for (let i = 0; i < n; i++) unitsCache[i] = valueToUnit(bins[i], scale);
      return unitsCache;
    };

    /**
     * Trace `samples` across [x0, x0 + width] about `mid`, with the gain
     * `gainFor(peak)` returns for their largest |sample|. With two or more
     * samples per pixel column it draws each column's min…max span as one
     * vertical stroke joined to the next (the pixels a per-sample polyline
     * covers, at a fraction of the path length); otherwise the per-sample
     * polyline. The envelope pass also yields the peak, so the samples are
     * read once.
     */
    const traceWave = (
      ctx: CanvasRenderingContext2D,
      samples: Float32Array,
      x0: number,
      width: number,
      mid: number,
      gainFor: (peak: number) => number,
    ) => {
      const n = samples.length;
      const cols = Math.max(1, Math.floor(width));
      ctx.beginPath();
      if (!waveEnvelopeApplies(n, cols)) {
        let peak = 0;
        for (let i = 0; i < n; i++) {
          const a = Math.abs(samples[i]);
          if (a > peak) peak = a;
        }
        const gain = gainFor(peak);
        const step = width / (n - 1);
        ctx.moveTo(x0, mid - samples[0] * gain);
        for (let i = 1; i < n; i++) {
          ctx.lineTo(x0 + i * step, mid - samples[i] * gain);
        }
        return;
      }
      if (waveMin.length < cols) {
        waveMin = new Float32Array(cols);
        waveMax = new Float32Array(cols);
      }
      const gain = gainFor(waveEnvelope(samples, cols, waveMin, waveMax));
      const colW = width / cols;
      // Enter each column from the end nearer the previous one, so the
      // joins stay short, as the polyline's would.
      let lastY = mid;
      for (let x = 0; x < cols; x++) {
        const cx = x0 + (x + 0.5) * colW;
        const yHi = mid - waveMax[x] * gain;
        const yLo = mid - waveMin[x] * gain;
        const hiFirst = Math.abs(yHi - lastY) <= Math.abs(yLo - lastY);
        const first = hiFirst ? yHi : yLo;
        const second = hiFirst ? yLo : yHi;
        if (x === 0) ctx.moveTo(cx, first);
        else ctx.lineTo(cx, first);
        ctx.lineTo(cx, second);
        lastY = second;
      }
    };

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
      const units = frameUnits(frame);
      const n = units.length;
      // Peak hold advances once per new frame and falls per millisecond,
      // like the page's.
      if (held.length !== n) {
        held = new Float32Array(n);
        heldSeq = -1;
      }
      if (frame.seq !== heldSeq) {
        heldSeq = frame.seq;
        const fall =
          PEAK_HOLD_DECAY_PER_MS * decayStepMs(frame.receivedAt, heldAt);
        heldAt = frame.receivedAt;
        for (let i = 0; i < n; i++) {
          const fallen = held[i] - fall;
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
      // Apply the intensity scale to the display units. The scale multiplies
      // the 0…1 signal before it is mapped to pixels, so the spectrum reads
      // taller without changing the view's pixel width. Clamped back into
      // display units so the gain-style alpha stays well-behaved.
      const sigScale = props.config.spectrum_signal_scale;
      const scaled = (v: number) => Math.min(1, v * sigScale);
      if (scaledValues.length !== values.length) {
        scaledValues = new Float32Array(values.length);
      }
      if (scaledHeld.length !== heldValues.length) {
        scaledHeld = new Float32Array(heldValues.length);
      }
      for (let i = 0; i < values.length; i++)
        scaledValues[i] = scaled(values[i]);
      for (let i = 0; i < heldValues.length; i++)
        scaledHeld[i] = scaled(heldValues[i]);

      const count = scaledValues.length;
      const colW = w / count;
      const mirror = props.config.spectrum_mirror;

      const tracePath = (edge: (v: number) => number, vals: Float32Array) => {
        sctx.beginPath();
        for (let i = 0; i < count; i++) {
          const x = (i + 0.5) * colW;
          const y = edge(vals[i]);
          if (i === 0) sctx.moveTo(x, y);
          else sctx.lineTo(x, y);
        }
      };

      if (props.config.spectrum_style === "bars") {
        sctx.fillStyle = color;
        const gap = colW > 3 ? 1 : 0;
        for (let i = 0; i < count; i++) {
          const y0 = g.top(scaledValues[i]);
          const y1 = g.bottom(scaledValues[i]);
          const barH = y1 - y0;
          if (barH <= 0) continue;
          sctx.globalAlpha = 0.35 + 0.65 * scaledValues[i];
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
            sctx.lineTo((i + 0.5) * colW, g.top(scaledValues[i]));
          }
          sctx.lineTo(w, g.base);
          if (mirror) {
            for (let i = count - 1; i >= 0; i--) {
              sctx.lineTo((i + 0.5) * colW, g.bottom(scaledValues[i]));
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
        tracePath(g.top, scaledValues);
        sctx.stroke();
        if (mirror) {
          tracePath(g.bottom, scaledValues);
          sctx.stroke();
        }
      }

      if (props.config.peak_hold) {
        sctx.fillStyle = color;
        sctx.globalAlpha = 0.7;
        const markW = Math.max(1, colW - 1);
        for (let i = 0; i < count; i++) {
          const x = i * colW;
          sctx.fillRect(x, g.top(scaledHeld[i]) - 1, markW, 1.5);
          if (mirror) {
            sctx.fillRect(x, g.bottom(scaledHeld[i]) - 0.5, markW, 1.5);
          }
        }
        sctx.globalAlpha = 1;
      }
    };

    // The circular spectrum, per the construction: the pooled bins are
    // mirrored about their centre and joined end-to-end (`[p, reversed(p)]`
    // — symmetric by construction), and that ONE mirrored signal is laid
    // over the WHOLE circle — D = 2·p points across a full 2π sweep,
    // starting and ending half a step either side of 3 o'clock (the right of
    // the ring, canvas angle 0), where the spectrum's low edge meets its
    // mirrored tail. The mirrored signal's
    // own symmetry is what closes the loop without a seam. The two paths
    // ±p ride at radius 1+p (outer) and 1-p (inner). The gain is a fixed
    // scale — no dynamic normalisation — so the loop breathes with the
    // signal instead of always filling the ring.
    const paintCircular = (
      canvasEl: HTMLCanvasElement | undefined,
      ctx: CanvasRenderingContext2D | null,
      frame: ScopeFrame | null,
    ) => {
      if (!canvasEl || !ctx) return;
      const { w, h } = prepare(canvasEl, ctx);
      const color = props.quiet ? colors.muted : colors.accent;
      const cx = w / 2;
      const cy = h / 2;
      // Unit-circle scale. Block view: the loudest possible loop (1+1) fits
      // the box. Background: the ring may span the whole window.
      const S = props.background
        ? Math.min(w, h) / 2 - 2
        : (Math.min(w, h) / 2 - 1) / 2;
      const baseRing = () => {
        ctx.globalAlpha = 0.35;
        ctx.strokeStyle = color;
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.arc(cx, cy, S, 0, Math.PI * 2);
        ctx.stroke();
        ctx.globalAlpha = 1;
      };
      if (!frame || !props.ready) {
        baseRing();
        return;
      }

      const cfg = props.config;
      const p = Math.max(1, Math.round(cfg.circular_bins));
      if (circPooled.length !== p) circPooled = new Float32Array(p);
      // Peak-pool the shared per-frame units (the exact values the linear
      // spectrum draws) down to the display count: each display bin is the
      // max of its bucket, so fewer bins means chunkier bars rather than a
      // lossy decimation.
      const units = frameUnits(frame);
      const bucket = units.length / p;
      for (let b = 0; b < p; b++) {
        const i0 = Math.floor(b * bucket);
        const i1 = Math.min(
          units.length,
          Math.max(i0 + 1, Math.floor((b + 1) * bucket)),
        );
        let m = 0;
        for (let i = i0; i < i1; i++) {
          if (units[i] > m) m = units[i];
        }
        // The floor gates the ring BEFORE the gain, so the two tune
        // independently: the floor is a threshold in shared units (without
        // it the ambient room tone paints every angle and the loop reads as
        // a filled disc), the gain amplifies only what survives it — a fixed
        // display multiplier, never a dynamic normalisation.
        const v =
          m <= cfg.circular_floor
            ? 0
            : (m - cfg.circular_floor) / (1 - cfg.circular_floor);
        circPooled[b] = Math.min(1, v * cfg.circular_gain);
      }

      const D = circularPointCount(p);
      // combined[k]: the summed input signal — the original spectrum with
      // its inversion appended, plus the original with its inversion
      // prepended, added point-wise and halved into display units. The
      // seam straddles the RIGHT of the ring (the 90° rotation); the two
      // ±branches ride at radius 1 ± combined.
      const combinedAt = (k: number) => circularSignalAt(k, circPooled);
      const angleAt = (k: number) => circularAngleAt(k, D);

      const signalScale = cfg.circular_signal_scale;
      // Apply the intensity scale to the pooled signal, then clamp back into
      // 0…1 display units. A scale > 1 makes the ring breathe more
      // dramatically; < 1 calms it. The view's pixel size is untouched.
      const scaledCombinedAt = (k: number) =>
        Math.min(1, Math.max(0, combinedAt(k) * signalScale));

      if (cfg.circular_bars) {
        ctx.strokeStyle = color;
        ctx.lineCap = "round";
        ctx.lineWidth = Math.max(1, (2 * Math.PI * S) / D - 1);
        for (let k = 0; k < D; k++) {
          const c = scaledCombinedAt(k);
          if (c <= 0) continue;
          const th = angleAt(k);
          const co = Math.cos(th);
          const si = Math.sin(th);
          ctx.globalAlpha = 0.35 + 0.65 * c;
          ctx.beginPath();
          ctx.moveTo(cx + co * S * (1 - c), cy + si * S * (1 - c));
          ctx.lineTo(cx + co * S * (1 + c), cy + si * S * (1 + c));
          ctx.stroke();
        }
        ctx.globalAlpha = 1;
      } else {
        // The two loops drawn as lines only — inner (1-p) dimmer, outer
        // (1+p) on top. No fill between them: the inside of the ring stays
        // empty so only the two signal lines show against the card.
        const radius = (k: number, sign: number) =>
          S * (1 + sign * scaledCombinedAt(k));
        const loopPath = (sign: number) => {
          ctx.beginPath();
          for (let k = 0; k < D; k++) {
            const th = angleAt(k);
            const x = cx + Math.cos(th) * radius(k, sign);
            const y = cy + Math.sin(th) * radius(k, sign);
            if (k === 0) ctx.moveTo(x, y);
            else ctx.lineTo(x, y);
          }
          ctx.closePath();
        };
        ctx.strokeStyle = color;
        ctx.lineWidth = 1;
        ctx.lineJoin = "round";
        // Inner loop first (dimmer), then outer loop on top. When
        // circular_show_inner is off only the outer loop is drawn — the
        // inner (negative) line disappears, leaving just the ring's
        // positive-expanding edge. The outer line is always visible.
        if (cfg.circular_show_inner) {
          loopPath(-1);
          ctx.globalAlpha = 0.55;
          ctx.stroke();
          ctx.globalAlpha = 1;
        }
        loopPath(1);
        ctx.stroke();
      }

      // Waveform traced inside the ring: a centred horizontal line from the
      // ring's left edge to its right edge, only when the toggle is on.
      if (cfg.wave_inside_circular && frame) {
        const samples = frame.wave;
        const n = samples.length;
        if (n >= 2) {
          // The ring's usable horizontal span: the trace runs ±0.85·S about
          // the centre, so it stays inside the base ring rather than touching
          // its left and right extremes.
          const innerR = S * 0.15; // keep the trace inside the loops
          const span = S - innerR;
          ctx.strokeStyle = color;
          ctx.globalAlpha = 0.85;
          ctx.lineWidth = 1;
          ctx.lineJoin = "round";
          // Track the wave ceiling from the raw analyser samples — same
          // source the linear waveform view reads — so the trace fills the
          // ring's horizontal span for the loudest recent swing.
          traceWave(
            ctx,
            samples,
            cx - span,
            2 * span,
            cy,
            (peak) =>
              (span / Math.max(peak, cfg.wave_gain_floor)) *
              cfg.wave_signal_scale,
          );
          ctx.stroke();
          ctx.globalAlpha = 1;
        }
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
      // Auto-gain: the trace fills the box for the loudest recent swing and
      // relaxes as the signal quiets, never amplifying below the floor.
      // The intensity scale multiplies the raw samples before the peak search,
      // so the trace swings taller without changing the view's pixel width.
      const wScale = props.config.wave_signal_scale;
      traceWave(wctx, frame.wave, 0, w, mid, (peak) => {
        waveCeiling = Math.max(
          peak * Math.abs(wScale),
          waveCeiling * WAVE_CEILING_DECAY,
          props.config.wave_gain_floor,
        );
        return ((mid - 1) / waveCeiling) * wScale;
      });
      wctx.stroke();
    };

    const paint = (frame: ScopeFrame | null) => {
      const now = performance.now();
      if (now - colorsAt > 1000) {
        colors = readColors();
        colorsAt = now;
      }
      if (props.background) {
        paintCircular(circular, cctx, frame);
        return;
      }
      paintSpectrum(frame);
      // When wave_inside_circular is on, the waveform is traced inside the
      // ring by paintCircular — skip the standalone waveform view so they
      // don't draw on top of each other.
      const waveInside =
        props.config.wave_inside_circular &&
        props.config.show_circular &&
        !props.config.circular_background;
      if (!waveInside) {
        paintWave(frame);
      }
      if (props.config.show_circular && !props.config.circular_background) {
        paintCircular(circular, cctx, frame);
      }
    };

    // Called by the shared feed on every poll, with the frame it holds (an
    // unchanged one when the reply was header-only). The key also carries
    // the props, so a ready/quiet/config change repaints on the next poll.
    const onFrame = (frame: ScopeFrame | null) => {
      if (disposed) return;
      const live = frame?.active ? frame : null;
      const c = props.config;
      const key = `${live?.seq ?? -1}:${props.ready}:${props.quiet}:${c.spectrum_style}:${c.spectrum_mirror}:${c.peak_hold}:${c.wave_gain_floor}:${c.spectrum_signal_scale}:${c.wave_signal_scale}:${c.circular_signal_scale}:${c.wave_inside_circular}:${c.circular_show_inner}:${c.show_circular}:${c.circular_bars}:${c.circular_bins}:${c.circular_gain}:${c.circular_floor}`;
      if (key !== paintedKey) {
        paintedKey = key;
        paint(live);
      }
    };

    paint(null);
    const unsubscribe = subscribeScope(onFrame, period);

    // Returned, not registered with `onCleanup`: in Solid 2 the effect's
    // apply returns its own teardown, and it is the only way to tie it to this
    // run of the loop rather than to the component.
    return () => {
      disposed = true;
      unsubscribe();
    };
  });

  // Background variant: one fixed, full-window canvas behind the card.
  if (props.background) {
    return (
      <canvas
        class="sscope sscope-bg"
        ref={(el: HTMLCanvasElement) => (circular = el)}
      />
    );
  }

  return (
    <>
      {props.config.show_spectrum && (
        <canvas
          class="sscope sscope-fft"
          style={{
            width: `${spectrumViewW(props.config)}px`,
            height: `${spectrumViewH(props.config)}px`,
          }}
          ref={(el: HTMLCanvasElement) => (spectrum = el)}
        />
      )}
      {props.config.show_wave &&
        !(
          props.config.wave_inside_circular &&
          props.config.show_circular &&
          !props.config.circular_background
        ) && (
          <canvas
            class="sscope sscope-wave"
            style={{
              width: `${waveViewW(props.config)}px`,
              height: `${waveViewH(props.config)}px`,
            }}
            ref={(el: HTMLCanvasElement) => (wave = el)}
          />
        )}
      {props.config.show_circular && !props.config.circular_background && (
        <canvas
          class="sscope sscope-circular"
          style={{
            width: `${props.config.circular_size}px`,
            height: `${props.config.circular_size}px`,
          }}
          ref={(el: HTMLCanvasElement) => (circular = el)}
        />
      )}
    </>
  );
}

export default OverlayScope;
