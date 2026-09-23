import { createEffect } from "solid-js";
import type { FftLoudnessMode } from "@/bindings";
import { getLatestFrame, subscribeFrames } from "@/stores/liveFftStore";
import {
  buildColormap,
  cssColor,
  frequencyTicks,
  maxPerColumn,
  type AxisTick,
  type ColormapKind,
} from "./liveFftMath";
import { observeTheme, pageUnits, sameValues } from "./SpectrumCanvas";

interface SpectrogramCanvasProps {
  axisHz: ArrayLike<number>;
  mode: FftLoudnessMode;
  dbRange: number;
  colormap: ColormapKind;
  running: boolean;
  /**
   * Translated canvas text (drawn, so lint cannot see it). `span` labels the
   * top edge: how long ago its row was, `WATERFALL_ROWS` frames back.
   */
  labels: { now: string; span: string };
  class?: string;
}

/** Rows of history the waterfall keeps: one per analysed frame. */
export const WATERFALL_ROWS = 240;
const MAX_COLUMNS = 2048;
const PAD_LEFT = 36;
const PAD_RIGHT = 8;
const noop = () => {};

export const SpectrogramCanvas = (props: SpectrogramCanvasProps) => {
  let canvasRef: HTMLCanvasElement | undefined;
  let propsVersion = 0;
  let requestPaint = noop;
  let lastProps: unknown[] = [];

  createEffect(
    () => [
      props.axisHz,
      props.mode,
      props.dbRange,
      props.colormap,
      props.running,
      props.labels,
    ],
    (next) => {
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
      // The history lives in an offscreen ring: row `head` is the next one
      // written (so the oldest), `head - 1` the newest. A new frame writes
      // one row in place; painting draws the ring in two slices (older rows
      // on top, newer below). Nothing is ever scrolled or copied.
      const off = document.createElement("canvas");
      off.height = WATERFALL_ROWS;
      const offCtx = off.getContext("2d");
      if (!offCtx) return;

      let raf = 0;
      let head = 0;
      let pushedSeq = -2;
      let drawnSeq = -2;
      let drawnVersion = -1;
      let width = 0;
      let height = 0;
      let sizeDirty = true;
      let colorsDirty = true;
      let lut: Uint8ClampedArray = new Uint8ClampedArray(0);
      let lutKind: ColormapKind | null = null;
      let lutAccent = "";
      let textColor = "#e6e6e6";
      let columns = new Float32Array(0);
      let row: ImageData | null = null;
      let ticks: AxisTick[] = [];
      let ticksAxis: ArrayLike<number> | null = null;

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

      const ensureLut = () => {
        const accent = cssColor("--color-accent", "#1FE0FF");
        const kind = props.colormap;
        if (kind !== lutKind || accent !== lutAccent) {
          lut = buildColormap(kind, accent);
          lutKind = kind;
          lutAccent = accent;
        }
      };

      const pushRow = (units: Float32Array) => {
        const n = units.length;
        if (n === 0) return;
        const cols = Math.min(n, MAX_COLUMNS);
        if (off.width !== cols) {
          // A new bin count restarts the history (the old rows describe a
          // different axis).
          off.width = cols;
          offCtx.clearRect(0, 0, cols, WATERFALL_ROWS);
          row = null;
          head = 0;
          columns = new Float32Array(cols);
        }
        if (!row) row = offCtx.createImageData(cols, 1);
        const values = n > cols ? maxPerColumn(units, cols, columns) : units;
        const data = row.data;
        for (let x = 0; x < cols; x++) {
          const idx = Math.min(255, Math.max(0, Math.round(values[x] * 255)));
          data[x * 4] = lut[idx * 3];
          data[x * 4 + 1] = lut[idx * 3 + 1];
          data[x * 4 + 2] = lut[idx * 3 + 2];
          data[x * 4 + 3] = 255;
        }
        offCtx.putImageData(row, 0, head);
        head = (head + 1) % WATERFALL_ROWS;
      };

      const paint = () => {
        const w = width;
        const h = height;
        const plotX = PAD_LEFT;
        const plotW = Math.max(1, w - PAD_LEFT - PAD_RIGHT);
        ctx.clearRect(0, 0, w, h);
        if (off.width > 0) {
          ctx.imageSmoothingEnabled = true;
          const rowH = h / WATERFALL_ROWS;
          const older = WATERFALL_ROWS - head;
          if (older > 0) {
            ctx.drawImage(
              off,
              0,
              head,
              off.width,
              older,
              plotX,
              0,
              plotW,
              older * rowH,
            );
          }
          if (head > 0) {
            ctx.drawImage(
              off,
              0,
              0,
              off.width,
              head,
              plotX,
              older * rowH,
              plotW,
              head * rowH,
            );
          }
        }
        ctx.font =
          "10px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace";
        ctx.textBaseline = "middle";
        ctx.fillStyle = textColor;
        ctx.strokeStyle = textColor;
        ctx.textAlign = "right";
        ctx.globalAlpha = 0.5;
        ctx.fillText(props.labels.now, plotX - 4, h - 7);
        ctx.fillText(props.labels.span, plotX - 4, 7);
        if (props.axisHz !== ticksAxis) {
          ticks = frequencyTicks(props.axisHz);
          ticksAxis = props.axisHz;
        }
        for (const tick of ticks) {
          const x = Math.round(plotX + plotW * tick.x) + 0.5;
          ctx.globalAlpha = 0.18;
          ctx.beginPath();
          ctx.moveTo(x, 0);
          ctx.lineTo(x, h);
          ctx.stroke();
        }
        ctx.globalAlpha = 1;
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
          textColor = cssColor("--color-text", "#e6e6e6");
          ensureLut();
          colorsDirty = false;
          dirty = true;
        }
        const frame = props.running ? getLatestFrame() : null;
        const seq = frame?.seq ?? -1;
        if (frame && seq !== pushedSeq) {
          pushedSeq = seq;
          ensureLut();
          // The spectrum canvas has usually computed these already.
          pushRow(pageUnits.units(frame, props.mode, props.dbRange));
          dirty = true;
        }
        if (!dirty && seq === drawnSeq && propsVersion === drawnVersion) {
          return;
        }
        if (propsVersion !== drawnVersion) ensureLut();
        drawnSeq = seq;
        drawnVersion = propsVersion;
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        paint();
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

  return (
    <canvas
      ref={(el) => (canvasRef = el)}
      class={`block w-full ${props.class ?? ""}`}
    />
  );
};
