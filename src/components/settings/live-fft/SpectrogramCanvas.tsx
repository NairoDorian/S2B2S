import React, { useEffect, useRef } from "react";
import type { FftLoudnessMode } from "@/bindings";
import { getLatestFrame } from "@/stores/liveFftStore";
import {
  buildColormap,
  cssColor,
  frequencyTicks,
  maxPerColumn,
  valueToUnit,
  type ColormapKind,
  type ValueScale,
} from "./liveFftMath";

interface SpectrogramCanvasProps {
  axisHz: ArrayLike<number>;
  mode: FftLoudnessMode;
  dbRange: number;
  colormap: ColormapKind;
  running: boolean;
  className?: string;
}

/** Rows of history kept in the offscreen image. */
const ROWS = 240;
/** Widest offscreen row; more bins are max-reduced into it. */
const MAX_COLUMNS = 2048;
const PAD_LEFT = 36;
const PAD_RIGHT = 8;
const CEILING_DECAY = 0.995;

/**
 * Scrolling waterfall under the analyser: one row per frame, newest at
 * the bottom, drawn into an offscreen canvas that is scrolled by one pixel
 * per frame and stretched onto the visible one.
 */
export const SpectrogramCanvas: React.FC<SpectrogramCanvasProps> = React.memo(
  ({ axisHz, mode, dbRange, colormap, running, className = "" }) => {
    const canvasRef = useRef<HTMLCanvasElement>(null);
    const propsRef = useRef({ axisHz, mode, dbRange, colormap, running });
    propsRef.current = { axisHz, mode, dbRange, colormap, running };

    useEffect(() => {
      const canvas = canvasRef.current;
      if (!canvas) return;
      const ctx = canvas.getContext("2d");
      if (!ctx) return;
      const off = document.createElement("canvas");
      off.height = ROWS;
      const offCtx = off.getContext("2d");
      if (!offCtx) return;

      let raf = 0;
      let drawnSeq = -2;
      let width = 0;
      let height = 0;
      let lut: Uint8ClampedArray = new Uint8ClampedArray(0);
      let lutKind: ColormapKind | null = null;
      let lutAccent = "";
      let colorsAt = 0;
      let textColor = "#e6e6e6";
      let ceiling = 1e-6;
      let units = new Float32Array(0);
      let columns = new Float32Array(0);
      let row: ImageData | null = null;

      const observer = new ResizeObserver(() => {
        width = 0;
      });
      observer.observe(canvas);

      const ensureLut = () => {
        const accent = cssColor("--color-accent", "#1FE0FF");
        const kind = propsRef.current.colormap;
        if (kind !== lutKind || accent !== lutAccent) {
          lut = buildColormap(kind, accent);
          lutKind = kind;
          lutAccent = accent;
        }
      };

      const pushFrame = (bins: Float32Array) => {
        const n = bins.length;
        if (n === 0) return;
        const p = propsRef.current;
        const cols = Math.min(n, MAX_COLUMNS);
        if (off.width !== cols) {
          off.width = cols;
          offCtx.clearRect(0, 0, cols, ROWS);
          row = null;
          columns = new Float32Array(cols);
        }
        if (!row) row = offCtx.createImageData(cols, 1);
        if (units.length !== n) units = new Float32Array(n);
        const scale: ValueScale = { mode: p.mode, dbRange: p.dbRange, ceiling };
        if (p.mode === "off") {
          let max = 0;
          for (let i = 0; i < n; i++) if (bins[i] > max) max = bins[i];
          ceiling = Math.max(max, ceiling * CEILING_DECAY, 1e-6);
          scale.ceiling = ceiling;
        }
        for (let i = 0; i < n; i++) units[i] = valueToUnit(bins[i], scale);
        const values = n > cols ? maxPerColumn(units, cols, columns) : units;
        const data = row.data;
        for (let x = 0; x < cols; x++) {
          const idx = Math.min(255, Math.max(0, Math.round(values[x] * 255)));
          data[x * 4] = lut[idx * 3];
          data[x * 4 + 1] = lut[idx * 3 + 1];
          data[x * 4 + 2] = lut[idx * 3 + 2];
          data[x * 4 + 3] = 255;
        }
        // Scroll the history up one row (drawing a canvas onto itself is
        // specified to copy first), then append the new row at the bottom.
        offCtx.drawImage(off, 0, -1);
        offCtx.putImageData(row, 0, ROWS - 1);
      };

      const paint = () => {
        const w = width;
        const h = height;
        const plotX = PAD_LEFT;
        const plotW = Math.max(1, w - PAD_LEFT - PAD_RIGHT);
        ctx.clearRect(0, 0, w, h);
        if (off.width > 0) {
          ctx.imageSmoothingEnabled = true;
          ctx.drawImage(off, plotX, 0, plotW, h);
        }
        ctx.font =
          "10px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace";
        ctx.textBaseline = "middle";
        ctx.fillStyle = textColor;
        ctx.strokeStyle = textColor;
        ctx.textAlign = "right";
        ctx.globalAlpha = 0.5;
        ctx.fillText("now", plotX - 4, h - 7);
        ctx.fillText(`−${ROWS}`, plotX - 4, 7);
        for (const tick of frequencyTicks(propsRef.current.axisHz)) {
          const x = Math.round(plotX + plotW * tick.x) + 0.5;
          ctx.globalAlpha = 0.18;
          ctx.beginPath();
          ctx.moveTo(x, 0);
          ctx.lineTo(x, h);
          ctx.stroke();
        }
        ctx.globalAlpha = 1;
      };

      const loop = () => {
        raf = requestAnimationFrame(loop);
        const rect = canvas.getBoundingClientRect();
        const dpr = window.devicePixelRatio || 1;
        const w = Math.max(1, Math.round(rect.width));
        const h = Math.max(1, Math.round(rect.height));
        let dirty = false;
        if (
          w !== width ||
          h !== height ||
          canvas.width !== Math.round(w * dpr)
        ) {
          canvas.width = Math.round(w * dpr);
          canvas.height = Math.round(h * dpr);
          width = w;
          height = h;
          dirty = true;
        }
        const now = performance.now();
        if (now - colorsAt > 1000) {
          textColor = cssColor("--color-text", "#e6e6e6");
          ensureLut();
          colorsAt = now;
          dirty = true;
        }
        const frame = propsRef.current.running ? getLatestFrame() : null;
        const seq = frame?.seq ?? -1;
        if (frame && seq !== drawnSeq) {
          ensureLut();
          pushFrame(frame.bins);
          dirty = true;
        }
        if (!dirty && seq === drawnSeq) return;
        drawnSeq = seq;
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        paint();
      };
      raf = requestAnimationFrame(loop);

      return () => {
        cancelAnimationFrame(raf);
        observer.disconnect();
      };
    }, []);

    return <canvas ref={canvasRef} className={`block w-full ${className}`} />;
  },
);

SpectrogramCanvas.displayName = "SpectrogramCanvas";
