import React, { useState, useEffect, useRef, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { getLanguageDirection } from "@/lib/utils/rtl";
import { syncLanguageFromSettings } from "@/i18n";
import Avatar3D from "./avatar/Avatar3D";

type OverlayPhase =
  | "idle"
  | "listening"
  | "thinking"
  | "seeing"
  | "speaking"
  | "done"
  | "error"
  | "hidden";

interface OverlayStatePayload {
  phase: OverlayPhase;
}

const BrainOverlayApp: React.FC = () => {
  const { t } = useTranslation();
  const dir = getLanguageDirection("en");
  const autoHideRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const streamingRef = useRef(false);

  const [phase, setPhase] = useState<OverlayPhase>("hidden");
  const [replyText, setReplyText] = useState("");
  const [isVisible, setIsVisible] = useState(false);
  const [micLevel, setMicLevel] = useState(0);
  const [metric, setMetric] = useState("");

  const dismiss = useCallback(async () => {
    if (streamingRef.current) {
      try {
        await invoke("brain_abort");
      } catch {}
      streamingRef.current = false;
    }
    setPhase("hidden");
    setIsVisible(false);
    setReplyText("");
    setMetric("");
    try {
      await invoke("overlay_fx_dismiss");
    } catch {}
  }, []);

  const scheduleAutoHide = useCallback(
    (delayMs: number) => {
      if (autoHideRef.current) clearTimeout(autoHideRef.current);
      autoHideRef.current = setTimeout(() => {
        dismiss();
      }, delayMs);
    },
    [dismiss],
  );

  useEffect(() => {
    const unlisteners: Array<() => void> = [];

    const setup = async () => {
      // ── Show / hide ──────────────────────────────────────────
      unlisteners.push(
        await listen<OverlayStatePayload>("overlay:state", async (event) => {
          await syncLanguageFromSettings();
          const p = event.payload.phase;
          setPhase(p);
          setIsVisible(p !== "hidden");
          if (p !== "hidden") {
            setReplyText("");
            setMetric("");
            streamingRef.current = false;
          }
        }),
      );

      // ── Brain pipeline ───────────────────────────────────────
      unlisteners.push(
        await listen("brain:thinking", () => {
          setPhase("thinking");
          setReplyText("");
          streamingRef.current = true;
        }),
      );

      unlisteners.push(
        await listen<string>("brain:token", (event) => {
          setPhase("speaking");
          setReplyText((prev) => prev + event.payload);
        }),
      );

      unlisteners.push(
        await listen<{
          text: string;
          tokens_per_sec: number;
          total_ms: number;
        }>("brain:done", (event) => {
          streamingRef.current = false;
          setPhase("done");
          const tps = event.payload.tokens_per_sec?.toFixed(0) ?? "?";
          const secs = ((event.payload.total_ms ?? 0) / 1000).toFixed(1);
          setMetric(`${tps} t/s · ${secs}s`);
          scheduleAutoHide(5000);
        }),
      );

      unlisteners.push(
        await listen<string>("brain:error", (event) => {
          streamingRef.current = false;
          setPhase("error");
          setReplyText(event.payload);
          scheduleAutoHide(3000);
        }),
      );

      // ── Mic level (avatar ears) ──────────────────────────────
      unlisteners.push(
        await listen<number[]>("mic-level", (event) => {
          const levels = event.payload;
          if (levels && levels.length > 0) {
            const max = Math.max(...levels);
            setMicLevel(max);
          }
        }),
      );

      // ── Keyboard: Esc → abort / dismiss ──────────────────────
      const handleKey = (e: KeyboardEvent) => {
        if (e.key === "Escape") {
          e.preventDefault();
          dismiss();
        }
      };
      window.addEventListener("keydown", handleKey);
      unlisteners.push(() => window.removeEventListener("keydown", handleKey));
    };

    setup();

    return () => {
      unlisteners.forEach((fn) => fn());
      if (autoHideRef.current) clearTimeout(autoHideRef.current);
    };
  }, [dismiss, scheduleAutoHide]);

  if (!isVisible) return null;

  return (
    <div
      dir={dir}
      style={{
        position: "relative",
        width: "100%",
        height: "100%",
        display: "flex",
        alignItems: "flex-start",
        gap: 12,
        padding: 12,
        pointerEvents: "none",
      }}
    >
      {/* 3D Avatar — rendered via Three.js, reacts to mic-level and phase */}
      <div style={{ flexShrink: 0 }}>
        <Avatar3D phase={phase} micLevel={micLevel} />
      </div>

      {/* Reply bubble + metric footer */}
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 4,
          flex: 1,
          maxWidth: 340,
        }}
      >
        {replyText && (
          <div
            style={{
              padding: "10px 14px",
              borderRadius: 14,
              background: "rgba(15, 15, 30, 0.85)",
              backdropFilter: "blur(16px)",
              border:
                phase === "error"
                  ? "1px solid rgba(248, 113, 113, 0.4)"
                  : "1px solid rgba(124, 58, 237, 0.3)",
              color: "#e2e8f0",
              fontSize: 13,
              lineHeight: 1.5,
              fontFamily:
                '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
              maxHeight: 280,
              overflowY: "auto",
            }}
          >
            {replyText}
            {phase === "thinking" && (
              <span
                style={{
                  display: "inline-block",
                  width: 6,
                  height: 13,
                  marginLeft: 2,
                  background: "#7c3aed",
                  animation: "blink 1s step-end infinite",
                }}
              />
            )}
          </div>
        )}

        {/* Metric chip */}
        {metric && (
          <div
            style={{
              alignSelf: "flex-end",
              padding: "2px 8px",
              borderRadius: 6,
              background: "rgba(124, 58, 237, 0.15)",
              color: "#a78bfa",
              fontSize: 10,
              fontFamily: "monospace",
            }}
          >
            {metric}
          </div>
        )}
      </div>

      <style>{`
        @keyframes blink {
          0%, 100% { opacity: 1; }
          50% { opacity: 0; }
        }
      `}</style>
    </div>
  );
};

export default BrainOverlayApp;
