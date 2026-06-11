import React, { useState, useRef, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { commands } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";

interface PiperStatusPayload {
  phase: string;
  model: string | null;
  cuda: boolean;
  error: string | null;
}

const TtsSelector: React.FC = () => {
  const { t } = useTranslation();
  const { settings, updateSetting } = useSettings();
  const [isOpen, setIsOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  const [piperStatus, setPiperStatus] = useState<PiperStatusPayload>({
    phase: "stopped",
    model: null,
    cuda: false,
    error: null,
  });

  const tts = settings?.tts;

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (
        dropdownRef.current &&
        !dropdownRef.current.contains(event.target as Node)
      ) {
        setIsOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClickOutside);

    // Initial status fetch
    const fetchStatus = async () => {
      try {
        const result = await commands.getPiperServerStatus();
        if (result.status === "ok") {
          const status = result.data;
          setPiperStatus({
            phase: status.running
              ? status.ready
                ? "ready"
                : "loading"
              : "stopped",
            model: status.model,
            cuda: status.cuda,
            error: null,
          });
        }
      } catch (err) {
        console.error("Failed to fetch initial Piper status:", err);
      }
    };
    fetchStatus();

    // Listen to backend status events
    const unlisten = listen<PiperStatusPayload>(
      "piper-status-changed",
      (event) => {
        setPiperStatus(event.payload);
      },
    );

    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      unlisten.then((fn) => fn());
    };
  }, []);

  if (!tts) return null;

  const getActiveVoiceName = () => {
    switch (tts.engine) {
      case "piper":
        return tts.voice;
      case "openai":
        return tts.openai?.voice || "";
      case "elevenlabs":
        return tts.elevenlabs?.voice_name || tts.elevenlabs?.voice_id || "";
      case "cartesia":
        return tts.cartesia?.voice_name || tts.cartesia?.voice_id || "";
      default:
        return "";
    }
  };

  const getTtsStatusColor = () => {
    if (!tts.enabled) return "bg-mid-gray/40";
    if (tts.engine !== "piper") return "bg-green-400";

    switch (piperStatus.phase) {
      case "ready":
        return "bg-green-400";
      case "warming_up":
        return "bg-orange-400 animate-pulse";
      case "loading":
        return "bg-yellow-400 animate-pulse";
      case "error":
        return "bg-red-400";
      case "stopped":
      default:
        return "bg-mid-gray/40";
    }
  };

  const getTtsDisplayText = () => {
    if (!tts.enabled) return t("settings.tts.disabled", "Speech Off");

    const voiceName = getActiveVoiceName();
    const voiceDisplay = voiceName ? ` (${voiceName})` : "";

    if (tts.engine === "piper") {
      switch (piperStatus.phase) {
        case "loading":
          return "Piper Loading...";
        case "warming_up":
          return "Piper Warming Up...";
        case "error":
          return "Piper Error";
        case "stopped":
          return "Piper Stopped";
        case "ready":
        default:
          return `Piper${voiceDisplay}${piperStatus.cuda ? " (CUDA)" : ""}`;
      }
    }

    const engineLabels: Record<string, string> = {
      openai: "OpenAI",
      elevenlabs: "ElevenLabs",
      cartesia: "Cartesia",
    };
    return `${engineLabels[tts.engine] || tts.engine}${voiceDisplay}`;
  };

  const handleToggleEnabled = async () => {
    await updateSetting("tts", {
      ...tts,
      enabled: !tts.enabled,
    });
  };

  const handleEngineSelect = async (engineId: string) => {
    if (engineId === tts.engine) return;
    await updateSetting("tts", {
      ...tts,
      engine: engineId as any,
    });
    setIsOpen(false);
  };

  const ENGINES = [
    { id: "piper", label: "Piper (Local)" },
    { id: "openai", label: "OpenAI (Cloud)" },
    { id: "elevenlabs", label: "ElevenLabs (Cloud)" },
    { id: "cartesia", label: "Cartesia (Cloud)" },
  ];

  return (
    <div className="relative" ref={dropdownRef}>
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="flex items-center gap-1.5 hover:text-text/80 transition-colors cursor-pointer text-xs focus:outline-none"
        title={`TTS: ${getTtsDisplayText()}`}
      >
        <span className="flex items-center gap-1">
          {/* eslint-disable-next-line i18next/no-literal-string */}
          <span>🗣️</span>
          <span className="font-medium">{t("footer.tts")}</span>
        </span>
        <div
          className={`w-1.5 h-1.5 rounded-full transition-colors duration-300 ${getTtsStatusColor()}`}
        />
        <svg
          className={`w-3 h-3 transition-transform duration-200 ${isOpen ? "rotate-180" : ""}`}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M19 9l-7 7-7-7"
          />
        </svg>
      </button>

      {isOpen && (
        <div className="absolute bottom-full start-0 mb-2 w-64 max-h-[60vh] overflow-y-auto bg-background border border-mid-gray/20 rounded-lg shadow-lg py-2.5 px-3 z-50 text-xs">
          <div className="flex items-center justify-between pb-2 mb-2 border-b border-mid-gray/10">
            <div className="flex flex-col">
              <span className="font-semibold text-text/80">
                {t("footer.ttsTitle")}
              </span>
              {tts.enabled && (
                <span className="text-[10px] text-text/50 font-normal truncate max-w-44">
                  {tts.engine === "piper"
                    ? `Piper: ${getActiveVoiceName() || "Default"}${piperStatus.cuda ? " (CUDA)" : ""}`
                    : `${tts.engine.toUpperCase()}: ${getActiveVoiceName() || "Default"}`}
                </span>
              )}
            </div>
            <label className="relative inline-flex items-center cursor-pointer">
              <input
                type="checkbox"
                checked={tts.enabled}
                onChange={handleToggleEnabled}
                className="sr-only peer"
              />
              <div className="w-7 h-4 bg-mid-gray/20 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:start-[2px] after:bg-text/70 after:border-gray-300 after:border after:rounded-full after:h-3 after:w-3 after:transition-all peer-checked:bg-logo-primary peer-checked:after:bg-white"></div>
            </label>
          </div>

          <div className="space-y-1">
            <div className="text-[10px] text-text/40 uppercase font-medium tracking-wider mb-1 px-1">
              {t("footer.ttsEngine")}
            </div>
            {ENGINES.map((eng) => (
              <button
                key={eng.id}
                onClick={() => handleEngineSelect(eng.id)}
                className={`w-full px-2 py-1.5 rounded text-start flex items-center justify-between hover:bg-mid-gray/10 transition-colors cursor-pointer ${
                  tts.engine === eng.id
                    ? "bg-logo-primary/10 text-logo-primary font-medium"
                    : "text-text/70"
                }`}
              >
                <span>{eng.label}</span>
                {tts.engine === eng.id && (
                  <div className="w-1.5 h-1.5 rounded-full bg-logo-primary" />
                )}
              </button>
            ))}
          </div>

          {tts.engine === "piper" && piperStatus.error && (
            <div className="mt-2 p-1.5 bg-red-500/10 text-red-400 border border-red-500/20 rounded text-[10px] break-words">
              {piperStatus.error}
            </div>
          )}
        </div>
      )}
    </div>
  );
};

export default TtsSelector;
