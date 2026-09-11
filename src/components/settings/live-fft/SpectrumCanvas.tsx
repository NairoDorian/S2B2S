import React, { useEffect, useRef } from "react";
import type { FftLoudnessMode } from "@/bindings";
import { getLatestFrame, type SpectrumFrame } from "@/stores/liveFftStore";
import {
  axisPosition,
  cssColor,
  formatHz,
  formatValue,
  frequencyTicks,
  maxPerColumn,
  noteName,
  valueTicks,
  valueToUnit,
  type ValueScale,
} from "./liveFftMath";

export type SpectrumStyle = "bars" | "line" | "area";

export interface HoverInfo {
  hz: number;
  value: number;
  bin: number;
}

interface SpectrumCanvasProps {
  /** Frequency of every output bin (from the status event). */
  axisHz: ArrayLike<number>;
  mode: FftLoudnessMode;
  dbRange: number;
  style: SpectrumStyle;
  peakHold: boolean;
  grid: boolean;
  running: boolean;
  labels: { idle: string; silence: string };
  onHover?: (info: HoverInfo | null) => void;
  className?: string;
}

const PAD_LEFT = 36;
const PAD_RIGHT = 8;
const PAD_TOP = 8;
const PAD_BOTTOM = 18;
/** Peak-hold fall per frame, in units of the drawing range (≈0.4 dB at 90 dB). */
const PEAK_DECAY = 0.0045;
/** Linear auto-range: how fast the ceiling follows a quieter signal. */
const CEILING_DECAY = 0.995;

interface Colors {
  accent: string;
  text: string;
}

const readColors = (): Colors => ({
  accent: cssColor("--color-logo-primary", "#1FE0FF"),
  text: cssColor("--color-text", "#e6e6e6"),
});

/**
 * The analyser display. A canvas driven by its own animation loop that
 * reads the newest frame from the store (no React state per frame) and
 * only repaints when a new frame, a resize, a theme refresh or a cursor
 * move calls for it.
 */
export const SpectrumCanvas: React.FC<SpectrumCanvasProps> = React.memo(
  ({
    axisHz,
    mode,
    dbRange,
    style,
    peakHold,
    grid,
    running,
    labels,
    onHover,
    className = "",
  }) => {
    const canvasRef = useRef<HTMLCanvasElement>(null);
    const hoverRef = useRef<number | null>(null);
    const propsRef = useRef({
      axisHz,
      mode,
      dbRange,
      style,
      peakHold,
      grid,
      running,
      labels,
      onHover,
    });
    propsRef.current = {
      axisHz,
      mode,
      dbRange,
      style,
      peakHold,
      grid,
      running,
      labels,
      onHover,
    };
    // A prop change must repaint even when no new frame arrives.
    const propsVersion = useRef(0);
    useEffect(() => {
      propsVersion.current += 1;
    }, [axisHz, mode, dbRange, style, peakHold, grid, running, labels]);

    useEffect(() => {
      const canvas = canvasRef.current;
      if (!canvas) return;
      const ctx = canvas.getContext("2d");
      if (!ctx) return;

      let raf = 0;
      let drawnSeq = -2;
      let drawnHover: number | null = null;
      let drawnVersion = -1;
      let width = 0;
      let height = 0;
      let colors = readColors();
      let colorsAt = 0;
      let ceiling = 1e-6;
      let units = new Float32Array(0);
      let held = new Float32Array(0);
      let heldSeq = -1;
      let columns = new Float32Array(0);
      let columnsHeld = new Float32Array(0);
      let lastHoverInfo: HoverInfo | null = null;

      const observer = new ResizeObserver(() => {
        width = 0;
      });
      observer.observe(canvas);

      const paint = (frame: SpectrumFrame | null, hover: number | null) => {
        const p = propsRef.current;
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

        const scale: ValueScale = { mode: p.mode, dbRange: p.dbRange, ceiling };

        // --- value grid ---
        if (p.grid) {
          ctx.strokeStyle = colors.text;
          ctx.fillStyle = colors.text;
          ctx.lineWidth = 1;
          for (const tick of valueTicks(scale)) {
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
          for (const tick of frequencyTicks(p.axisHz)) {
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

        // --- frame ---
        if (!frame) {
          ctx.globalAlpha = 0.45;
          ctx.fillStyle = colors.text;
          ctx.textAlign = "center";
          ctx.fillText(p.labels.idle, plotX + plotW / 2, plotY + plotH / 2);
          ctx.globalAlpha = 1;
          return;
        }

        const bins = frame.bins;
        const n = bins.length;
        if (units.length !== n) units = new Float32Array(n);
        if (p.mode === "off") {
          let max = 0;
          for (let i = 0; i < n; i++) if (bins[i] > max) max = bins[i];
          ceiling = Math.max(max, ceiling * CEILING_DECAY, 1e-6);
          scale.ceiling = ceiling;
        }
        for (let i = 0; i < n; i++) units[i] = valueToUnit(bins[i], scale);

        // Peak hold advances once per new frame.
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

        const cols = Math.min(n, Math.floor(plotW));
        if (columns.length !== cols) {
          columns = new Float32Array(cols);
          columnsHeld = new Float32Array(cols);
        }
        const values = n > cols ? maxPerColumn(units, cols, columns) : units;
        const heldValues =
          n > cols ? maxPerColumn(held, cols, columnsHeld) : held;
        const count = values.length;
        const colW = plotW / count;

        const gradient = ctx.createLinearGradient(0, plotY, 0, plotY + plotH);
        gradient.addColorStop(0, colors.accent);
        gradient.addColorStop(1, colors.accent);

        if (p.style === "bars") {
          ctx.fillStyle = colors.accent;
          const gap = colW > 3 ? 1 : 0;
          for (let i = 0; i < count; i++) {
            const barH = values[i] * plotH;
            if (barH <= 0) continue;
            const x = plotX + i * colW;
            ctx.globalAlpha = 0.35 + 0.65 * values[i];
            ctx.fillRect(
              x,
              plotY + plotH - barH,
              Math.max(1, colW - gap),
              barH,
            );
          }
          ctx.globalAlpha = 1;
        } else {
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
            ctx.fillStyle = gradient;
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

        if (p.peakHold) {
          ctx.fillStyle = colors.text;
          ctx.globalAlpha = 0.7;
          for (let i = 0; i < count; i++) {
            const y = plotY + plotH * (1 - heldValues[i]);
            ctx.fillRect(plotX + i * colW, y - 1, Math.max(1, colW - 1), 1.5);
          }
          ctx.globalAlpha = 1;
        }

        // --- peak marker ---
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

        // --- cursor ---
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
          (info?.value ?? NaN) !== (lastHoverInfo?.value ?? NaN)
        ) {
          lastHoverInfo = info;
          p.onHover?.(info);
        }
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
          colors = readColors();
          colorsAt = now;
          dirty = true;
        }
        const frame = propsRef.current.running ? getLatestFrame() : null;
        const seq = frame?.seq ?? -1;
        const hover = hoverRef.current;
        if (
          !dirty &&
          seq === drawnSeq &&
          hover === drawnHover &&
          propsVersion.current === drawnVersion
        ) {
          return;
        }
        drawnSeq = seq;
        drawnHover = hover;
        drawnVersion = propsVersion.current;
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        paint(frame, hover);
      };
      raf = requestAnimationFrame(loop);

      return () => {
        cancelAnimationFrame(raf);
        observer.disconnect();
      };
    }, []);

    const updateHover = (event: React.MouseEvent<HTMLCanvasElement>) => {
      const rect = event.currentTarget.getBoundingClientRect();
      const plotW = rect.width - PAD_LEFT - PAD_RIGHT;
      const x = (event.clientX - rect.left - PAD_LEFT) / Math.max(1, plotW);
      hoverRef.current = x >= 0 && x <= 1 ? x : null;
    };

    return (
      <canvas
        ref={canvasRef}
        className={`block w-full ${className}`}
        onMouseMove={updateHover}
        onMouseLeave={() => {
          hoverRef.current = null;
        }}
      />
    );
  },
);

SpectrumCanvas.displayName = "SpectrumCanvas";
