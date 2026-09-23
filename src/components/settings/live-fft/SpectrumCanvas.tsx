import { createEffect } from "solid-js";

import type { FftLoudnessMode } from "@/bindings";
import {
  getLatestFrame,
  subscribeFrames,
  type SpectrumFrame,
} from "@/stores/liveFftStore";
import {
  FrameUnitsCache,
  PEAK_HOLD_DECAY_PER_MS,
  axisPosition,
  cssColor,
  decayStepMs,
  formatHz,
  formatValue,
  frequencyTicks,
  maxPerColumn,
  noteName,
  valueTicks,
  type AxisTick,
  type ValueScale,
  type ValueTick,
} from "./liveFftMath";

export type SpectrumStyle = "bars" | "line" | "area";

export interface HoverInfo {
  hz: number;
  value: number;
  bin: number;
}

interface SpectrumCanvasProps {
  axisHz: ArrayLike<number>;
  mode: FftLoudnessMode;
  dbRange: number;
  style: SpectrumStyle;
  peakHold: boolean;
  grid: boolean;
  running: boolean;
  labels: { idle: string; silence: string };
  onHover?: (info: HoverInfo | null) => void;
  class?: string;
}

/**
 * The 0…1 units of the latest frame, shared by the spectrum and the
 * waterfall: whichever paints first computes them, the other reads the
 * cache (one pass per frame instead of one per canvas).
 */
export const pageUnits = new FrameUnitsCache();

const PAD_LEFT = 36;
const PAD_RIGHT = 8;
const PAD_TOP = 8;
const PAD_BOTTOM = 18;
/** Bar opacity is quantised into this many levels, one fill per level. */
const ALPHA_LEVELS = 16;

interface Colors {
  accent: string;
  text: string;
}

const readColors = (): Colors => ({
  accent: cssColor("--color-accent", "#1FE0FF"),
  text: cssColor("--color-text", "#e6e6e6"),
});

/**
 * Watch the document root for a theme or accent change (the `data-theme`
 * attribute and the accent properties on its inline style), so a canvas
 * that is not repainting on its own still picks the new colours up.
 */
export const observeTheme = (onChange: () => void): (() => void) => {
  const observer = new MutationObserver(onChange);
  observer.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ["data-theme", "style", "class"],
  });
  return () => observer.disconnect();
};

const noop = () => {};

/** Element-wise identity of two prop snapshots. */
export const sameValues = (a: unknown[], b: unknown[]): boolean =>
  a.length === b.length && a.every((v, i) => Object.is(v, b[i]));

export const SpectrumCanvas = (props: SpectrumCanvasProps) => {
  let canvasRef: HTMLCanvasElement | undefined;
  let hoverRef: number | null = null;
  let propsVersion = 0;
  // Set by the paint loop once it exists; a no-op before mount.
  let requestPaint = noop;
  let lastProps: unknown[] = [];

  createEffect(
    () => [
      props.axisHz,
      props.mode,
      props.dbRange,
      props.style,
      props.peakHold,
      props.grid,
      props.running,
      props.labels,
    ],
    (next) => {
      // The getters may re-run for an unrelated settings change (they read
      // the page's whole draft); repaint only when a value really moved.
      if (sameValues(next, lastProps)) return;
      lastProps = next;
      propsVersion += 1;
      requestPaint();
    },
  );

  createEffect(
    () => undefined,
    () => {
      const canvas = canvasRef;
      if (!canvas) return;
      const ctx = canvas.getContext("2d");
      if (!ctx) return;

      // Painting is event-driven: a rAF is requested only when a frame
      // arrives, the pointer moves, a prop changes, the box resizes or the
      // theme changes. An idle or stopped analyser costs nothing.
      let raf = 0;
      let drawnSeq = -2;
      let drawnHover: number | null = null;
      let drawnVersion = -1;
      let width = 0;
      let height = 0;
      let sizeDirty = true;
      let colors = readColors();
      let colorsDirty = false;
      let held = new Float32Array(0);
      let heldFrame: SpectrumFrame | null = null;
      let heldAt = 0;
      let columns = new Float32Array(0);
      let columnsHeld = new Float32Array(0);
      let levels = new Uint8Array(0);
      let order = new Int32Array(0);
      const levelStart = new Int32Array(ALPHA_LEVELS + 1);
      const fillAt = new Int32Array(ALPHA_LEVELS);
      let lastHoverInfo: HoverInfo | null = null;
      let freqTicks: AxisTick[] = [];
      let freqTicksAxis: ArrayLike<number> | null = null;
      let valTicks: ValueTick[] = [];
      let valTicksKey = "";
      const idleScale: ValueScale = { mode: "db", dbRange: 90, ceiling: 1 };

      const schedule = () => {
        if (raf === 0) raf = requestAnimationFrame(render);
      };
      requestPaint = schedule;

      const observer = new ResizeObserver((entries) => {
        const box = entries[entries.length - 1]?.contentRect;
        if (!box) return;
        width = Math.max(1, Math.round(box.width));
        height = Math.max(1, Math.round(box.height));
        sizeDirty = true;
        schedule();
      });
      observer.observe(canvas);
      const stopTheme = observeTheme(() => {
        colorsDirty = true;
        schedule();
      });
      const unsubscribe = subscribeFrames(schedule);

      const frequencyGrid = (axis: ArrayLike<number>) => {
        if (axis !== freqTicksAxis) {
          freqTicks = frequencyTicks(axis);
          freqTicksAxis = axis;
        }
        return freqTicks;
      };
      const valueGrid = (scale: ValueScale) => {
        const key = `${scale.mode}:${scale.dbRange}:${scale.mode === "off" ? scale.ceiling : 0}`;
        if (key !== valTicksKey) {
          valTicks = valueTicks(scale);
          valTicksKey = key;
        }
        return valTicks;
      };

      const paintBars = (
        values: Float32Array,
        count: number,
        plotX: number,
        plotY: number,
        plotW: number,
        plotH: number,
      ) => {
        // One fill per opacity level instead of one fillRect + alpha change
        // per column: bucket the columns by level (counting sort), then add
        // each level's rectangles to one path.
        const colW = plotW / count;
        const gap = colW > 3 ? 1 : 0;
        const barW = Math.max(1, colW - gap);
        if (levels.length < count) {
          levels = new Uint8Array(count);
          order = new Int32Array(count);
        }
        levelStart.fill(0);
        for (let i = 0; i < count; i++) {
          const v = values[i];
          // `v > 0` also sends a NaN to level 0, keeping the counts exact.
          const level =
            v > 0
              ? Math.min(ALPHA_LEVELS - 1, Math.floor(v * ALPHA_LEVELS))
              : 0;
          levels[i] = level;
          levelStart[level + 1] += 1;
        }
        for (let l = 0; l < ALPHA_LEVELS; l++) {
          levelStart[l + 1] += levelStart[l];
        }
        fillAt.set(levelStart.subarray(0, ALPHA_LEVELS));
        for (let i = 0; i < count; i++) order[fillAt[levels[i]]++] = i;
        ctx.fillStyle = colors.accent;
        for (let l = 0; l < ALPHA_LEVELS; l++) {
          const from = levelStart[l];
          const to = levelStart[l + 1];
          if (to <= from) continue;
          ctx.beginPath();
          let any = false;
          for (let k = from; k < to; k++) {
            const i = order[k];
            const barH = values[i] * plotH;
            if (barH <= 0) continue;
            ctx.rect(plotX + i * colW, plotY + plotH - barH, barW, barH);
            any = true;
          }
          if (!any) continue;
          ctx.globalAlpha = 0.35 + (0.65 * (l + 0.5)) / ALPHA_LEVELS;
          ctx.fill();
        }
        ctx.globalAlpha = 1;
      };

      const paint = (frame: SpectrumFrame | null, hover: number | null) => {
        const p = props;
        const w = width;
        const h = height;
        const plotX = PAD_LEFT;
        const plotY = PAD_TOP;
        const plotW = Math.max(1, w - PAD_LEFT - PAD_RIGHT);
        const plotH = Math.max(1, h - PAD_TOP - PAD_BOTTOM);
        ctx.clearRect(0, 0, w, h);
        ctx.font =
          "10px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace";
        ctx.textBaseline = "middle";

        // One units pass per frame, shared with the waterfall.
        const units = frame ? pageUnits.units(frame, p.mode, p.dbRange) : null;
        let scale: ValueScale = pageUnits.scale;
        if (!units) {
          idleScale.mode = p.mode;
          idleScale.dbRange = p.dbRange;
          idleScale.ceiling = pageUnits.scale.ceiling;
          scale = idleScale;
        }

        if (p.grid) {
          ctx.strokeStyle = colors.text;
          ctx.fillStyle = colors.text;
          ctx.lineWidth = 1;
          for (const tick of valueGrid(scale)) {
            const y = Math.round(plotY + plotH * (1 - tick.y)) + 0.5;
            ctx.globalAlpha = 0.12;
            ctx.beginPath();
            ctx.moveTo(plotX, y);
            ctx.lineTo(plotX + plotW, y);
            ctx.stroke();
            ctx.globalAlpha = 0.55;
            ctx.textAlign = "right";
            ctx.fillText(tick.label, plotX - 4, y);
          }
          for (const tick of frequencyGrid(p.axisHz)) {
            const x = Math.round(plotX + plotW * tick.x) + 0.5;
            ctx.globalAlpha = 0.12;
            ctx.beginPath();
            ctx.moveTo(x, plotY);
            ctx.lineTo(x, plotY + plotH);
            ctx.stroke();
            ctx.globalAlpha = 0.55;
            ctx.textAlign = "center";
            ctx.fillText(tick.label, x, h - PAD_BOTTOM / 2);
          }
          ctx.globalAlpha = 1;
        }

        if (!frame || !units) {
          ctx.globalAlpha = 0.45;
          ctx.fillStyle = colors.text;
          ctx.textAlign = "center";
          ctx.fillText(p.labels.idle, plotX + plotW / 2, plotY + plotH / 2);
          ctx.globalAlpha = 1;
          // No frame, nothing under the cursor: drop a readout left over from
          // the last frame so it does not outlive the stream.
          if (lastHoverInfo !== null) {
            lastHoverInfo = null;
            p.onHover?.(null);
          }
          return;
        }

        const bins = frame.bins;
        const n = bins.length;

        // Peak hold advances once per new frame and falls per millisecond,
        // so it drops at the same speed whatever the update rate.
        if (held.length !== n) {
          held = new Float32Array(n);
          heldFrame = null;
        }
        if (frame !== heldFrame) {
          const fall =
            PEAK_HOLD_DECAY_PER_MS * decayStepMs(frame.receivedAt, heldAt);
          heldFrame = frame;
          heldAt = frame.receivedAt;
          for (let i = 0; i < n; i++) {
            const fallen = held[i] - fall;
            held[i] = units[i] > fallen ? units[i] : fallen;
          }
        }

        const cols = Math.min(n, Math.floor(plotW));
        if (columns.length !== cols) {
          columns = new Float32Array(cols);
          columnsHeld = new Float32Array(cols);
        }
        const values = n > cols ? maxPerColumn(units, cols, columns) : units;
        const count = values.length;
        const colW = plotW / Math.max(1, count);

        if (count > 0 && p.style === "bars") {
          paintBars(values, count, plotX, plotY, plotW, plotH);
        } else if (count > 0) {
          ctx.beginPath();
          for (let i = 0; i < count; i++) {
            const x = plotX + (i + 0.5) * colW;
            const y = plotY + plotH * (1 - values[i]);
            if (i === 0) ctx.moveTo(x, y);
            else ctx.lineTo(x, y);
          }
          if (p.style === "area") {
            ctx.lineTo(plotX + (count - 0.5) * colW, plotY + plotH);
            ctx.lineTo(plotX + 0.5 * colW, plotY + plotH);
            ctx.closePath();
            ctx.globalAlpha = 0.28;
            ctx.fillStyle = colors.accent;
            ctx.fill();
            ctx.globalAlpha = 1;
            ctx.beginPath();
            for (let i = 0; i < count; i++) {
              const x = plotX + (i + 0.5) * colW;
              const y = plotY + plotH * (1 - values[i]);
              if (i === 0) ctx.moveTo(x, y);
              else ctx.lineTo(x, y);
            }
          }
          ctx.strokeStyle = colors.accent;
          ctx.lineWidth = 1.5;
          ctx.lineJoin = "round";
          ctx.stroke();
        }

        if (p.peakHold && count > 0) {
          const heldValues =
            n > cols ? maxPerColumn(held, cols, columnsHeld) : held;
          const markW = Math.max(1, colW - 1);
          ctx.fillStyle = colors.text;
          ctx.globalAlpha = 0.7;
          ctx.beginPath();
          for (let i = 0; i < count; i++) {
            const y = plotY + plotH * (1 - heldValues[i]);
            ctx.rect(plotX + i * colW, y - 1, markW, 1.5);
          }
          ctx.fill();
          ctx.globalAlpha = 1;
        }

        if (frame.silent) {
          ctx.globalAlpha = 0.4;
          ctx.fillStyle = colors.text;
          ctx.textAlign = "center";
          ctx.fillText(p.labels.silence, plotX + plotW / 2, plotY + 12);
          ctx.globalAlpha = 1;
        } else if (frame.peakHz > 0) {
          const xp = axisPosition(p.axisHz, frame.peakHz);
          if (xp !== null) {
            const x = plotX + plotW * xp;
            ctx.fillStyle = colors.text;
            ctx.globalAlpha = 0.85;
            ctx.beginPath();
            ctx.moveTo(x, plotY + 6);
            ctx.lineTo(x - 4, plotY);
            ctx.lineTo(x + 4, plotY);
            ctx.closePath();
            ctx.fill();
            const note = noteName(frame.peakHz);
            const text = note
              ? `${formatHz(frame.peakHz)} · ${note}`
              : formatHz(frame.peakHz);
            ctx.textAlign = x > plotX + plotW * 0.75 ? "right" : "left";
            ctx.fillText(
              text,
              x + (ctx.textAlign === "right" ? -6 : 6),
              plotY + 6,
            );
            ctx.globalAlpha = 1;
          }
        }

        let info: HoverInfo | null = null;
        if (hover !== null && n > 0) {
          const bin = Math.min(n - 1, Math.max(0, Math.round(hover * (n - 1))));
          const hz = Number(p.axisHz[bin] ?? 0);
          info = { hz, value: bins[bin], bin };
          const x = Math.round(plotX + plotW * hover) + 0.5;
          ctx.strokeStyle = colors.text;
          ctx.globalAlpha = 0.5;
          ctx.setLineDash([3, 3]);
          ctx.beginPath();
          ctx.moveTo(x, plotY);
          ctx.lineTo(x, plotY + plotH);
          ctx.stroke();
          ctx.setLineDash([]);
          const y = plotY + plotH * (1 - units[bin]);
          ctx.globalAlpha = 1;
          ctx.fillStyle = colors.accent;
          ctx.beginPath();
          ctx.arc(x, y, 3, 0, Math.PI * 2);
          ctx.fill();
          const label = `${formatHz(hz)}  ${formatValue(bins[bin], p.mode)}`;
          ctx.textAlign = hover > 0.7 ? "right" : "left";
          const tx = x + (hover > 0.7 ? -8 : 8);
          const ty = Math.max(plotY + 22, y - 10);
          ctx.fillStyle = colors.text;
          ctx.fillText(label, tx, ty);
        }
        if (
          (info?.bin ?? -1) !== (lastHoverInfo?.bin ?? -1) ||
          // Object.is: with no hover on either side both are NaN, and
          // NaN !== NaN would re-send null on every painted frame.
          !Object.is(info?.value ?? NaN, lastHoverInfo?.value ?? NaN)
        ) {
          lastHoverInfo = info;
          p.onHover?.(info);
        }
      };

      const render = () => {
        raf = 0;
        if (width === 0 || height === 0) return;
        const dpr = window.devicePixelRatio || 1;
        let dirty = false;
        if (sizeDirty || canvas.width !== Math.round(width * dpr)) {
          canvas.width = Math.round(width * dpr);
          canvas.height = Math.round(height * dpr);
          sizeDirty = false;
          dirty = true;
        }
        if (colorsDirty) {
          colors = readColors();
          colorsDirty = false;
          dirty = true;
        }
        const frame = props.running ? getLatestFrame() : null;
        const seq = frame?.seq ?? -1;
        const hover = hoverRef;
        if (
          !dirty &&
          seq === drawnSeq &&
          hover === drawnHover &&
          propsVersion === drawnVersion
        ) {
          return;
        }
        drawnSeq = seq;
        drawnHover = hover;
        drawnVersion = propsVersion;
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        paint(frame, hover);
      };

      return () => {
        requestPaint = noop;
        if (raf !== 0) cancelAnimationFrame(raf);
        observer.disconnect();
        stopTheme();
        unsubscribe();
      };
    },
  );

  const updateHover = (
    event: MouseEvent & { currentTarget: HTMLCanvasElement },
  ) => {
    // offsetX is relative to the canvas box, so no layout read is needed.
    const plotW = event.currentTarget.clientWidth - PAD_LEFT - PAD_RIGHT;
    const x = (event.offsetX - PAD_LEFT) / Math.max(1, plotW);
    hoverRef = x >= 0 && x <= 1 ? x : null;
    requestPaint();
  };

  return (
    <canvas
      ref={(el) => (canvasRef = el)}
      class={`block w-full ${props.class ?? ""}`}
      onMouseMove={updateHover}
      onMouseLeave={() => {
        hoverRef = null;
        requestPaint();
      }}
    />
  );
};
