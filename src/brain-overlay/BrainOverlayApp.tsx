import React, { useState, useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { getLanguageDirection } from "@/lib/utils/rtl";
import { syncLanguageFromSettings } from "@/i18n";

type OverlayPhase =
  | "idle"
  | "listening"
  | "thinking"
  | "seeing"
  | "speaking"
  | "done"
  | "error"
  | "hidden";

interface CursorPosition {
  x: number;
  y: number;
  monitor_id: string | null;
}

interface BubbleAppend {
  text: string;
  is_final: boolean;
}

const BrainOverlayApp: React.FC = () => {
  const { t } = useTranslation();
  const dir = getLanguageDirection("en"); // will be synced on events

  const [phase, setPhase] = useState<OverlayPhase>("hidden");
  const [replyText, setReplyText] = useState("");
  const [isVisible, setIsVisible] = useState(false);

  useEffect(() => {
    const unlisteners: Array<() => void> = [];

    const setup = async () => {
      unlisteners.push(
        await listen<{ phase: OverlayPhase }>("overlay:state", async (event) => {
          await syncLanguageFromSettings();
          setPhase(event.payload.phase);
          setIsVisible(event.payload.phase !== "hidden");
        }),
      );

      unlisteners.push(
        await listen<BubbleAppend>("overlay:append", (event) => {
          setReplyText((prev) => {
            if (event.payload.is_final) return prev;
            return prev + event.payload.text;
          });
        }),
      );

      unlisteners.push(
        await listen("overlay:clear", () => {
          setReplyText("");
        }),
      );
    };

    setup();

    return () => {
      unlisteners.forEach((fn) => fn());
    };
  }, []);

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
      {/* Avatar placeholder — will be replaced with Three.js 3D avatar */}
      <div
        style={{
          width: 72,
          height: 72,
          borderRadius: "50%",
          background: "rgba(124, 58, 237, 0.25)",
          border: "2px solid rgba(124, 58, 237, 0.4)",
          backdropFilter: "blur(12px)",
          flexShrink: 0,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          color: "#a78bfa",
          fontSize: 10,
          fontWeight: 600,
          textTransform: "uppercase",
        }}
      >
        {phase === "listening" ? "🎤" : phase === "thinking" ? "🧠" : phase === "speaking" ? "🔊" : "S2B"}
      </div>

      {/* Reply bubble */}
      {replyText && (
        <div
          style={{
            maxWidth: 340,
            padding: "10px 14px",
            borderRadius: 14,
            background: "rgba(15, 15, 30, 0.85)",
            backdropFilter: "blur(16px)",
            border: "1px solid rgba(124, 58, 237, 0.3)",
            color: "#e2e8f0",
            fontSize: 13,
            lineHeight: 1.5,
            fontFamily:
              '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
            flex: 1,
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
    </div>
  );
};

export default BrainOverlayApp;
