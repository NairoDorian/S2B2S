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

interface LocalTtsStatusPayload {
  engine: string;
  phase: string;
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
  const [localStatuses, setLocalStatuses] = useState<Record<string, string>>(
    {},
  );
  const [voiceCounts, setVoiceCounts] = useState<Record<string, number>>({});

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
    const unlistenPiper = listen<PiperStatusPayload>(
      "piper-status-changed",
      (event) => {
        setPiperStatus(event.payload);
      },
    );
    const unlistenLocal = listen<LocalTtsStatusPayload>(
      "local-tts-status-changed",
      (event) => {
        setLocalStatuses((prev) => ({
          ...prev,
          [event.payload.engine]: event.payload.phase,
        }));
      },
    );

    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      unlistenPiper.then((fn) => fn());
      unlistenLocal.then((fn) => fn());
    };
  }, []);

  // Fetch voice counts per engine
  useEffect(() => {
    const fetchCounts = async () => {
      const engines = [
        "piper",
        "kokoro",
        "kitten",
        "pocket",
        "qwen3",
        "sapi",
        "openai",
        "elevenlabs",
        "cartesia",
      ];
      const counts: Record<string, number> = {};
      for (const engine of engines) {
        try {
          const res = await commands.ttsGetVoices(engine as any);
          if (res.status === "ok") {
            counts[engine] = res.data.length;
          }
        } catch {
          /* ignore */
        }
      }
      setVoiceCounts(counts);
    };
    fetchCounts();
  }, [tts?.piper?.data_dir]);

  if (!tts) return null;

  const getLocalEngineStatus = (engine: string) => {
    return localStatuses[engine] || "stopped";
  };

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
        return tts.voice || "";
    }
  };

  const isLocalEngine = (engine: string) =>
    ["piper", "kokoro", "kitten", "pocket", "qwen3", "sapi"].includes(engine);

  const getEngineStatusColor = (engine: string) => {
    const phase =
      engine === "piper" ? piperStatus.phase : getLocalEngineStatus(engine);
    switch (phase) {
      case "ready":
        return "bg-green-400";
      case "warming_up":
        return "bg-orange-400 animate-pulse";
      case "loading":
        return "bg-yellow-400 animate-pulse";
      case "error":
        return "bg-red-400";
      default:
        return "bg-mid-gray/40";
    }
  };

  const getTtsStatusColor = () => {
    if (!tts.enabled) return "bg-mid-gray/40";
    if (!isLocalEngine(tts.engine)) return "bg-green-400";
    return getEngineStatusColor(tts.engine);
  };

  const getTtsDisplayText = () => {
    if (!tts.enabled) return t("settings.tts.disabled", "Speech Off");

    const voiceName = getActiveVoiceName();
    const voiceDisplay = voiceName ? ` (${voiceName})` : "";
    const engineLabel = getEngineLabel(tts.engine);

    if (isLocalEngine(tts.engine)) {
      const phase =
        tts.engine === "piper"
          ? piperStatus.phase
          : getLocalEngineStatus(tts.engine);
      switch (phase) {
        case "loading":
          return `${engineLabel} Loading...`;
        case "warming_up":
          return `${engineLabel} Warming Up...`;
        case "error":
          return `${engineLabel} Error`;
        case "stopped":
          return `${engineLabel} Stopped`;
        default:
          return `${engineLabel}${voiceDisplay}${tts.engine === "piper" && piperStatus.cuda ? " (CUDA)" : ""}`;
      }
    }

    return `${engineLabel}${voiceDisplay}`;
  };

  const getEngineLabel = (engine: string): string => {
    const labels: Record<string, string> = {
      piper: "Piper",
      kokoro: "Kokoro",
      kitten: "Kitten",
      pocket: "Pocket",
      qwen3: "Qwen3",
      sapi: "SAPI",
      openai: "OpenAI",
      elevenlabs: "ElevenLabs",
      cartesia: "Cartesia",
    };
    return labels[engine] || engine;
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
    { id: "kokoro", label: "Kokoro (Local)" },
    { id: "kitten", label: "Kitten (Local)" },
    { id: "pocket", label: "Pocket (Local)" },
    { id: "qwen3", label: "Qwen3 (Local)" },
    { id: "sapi", label: "SAPI (Local)" },
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
                  {`${getEngineLabel(tts.engine)}: ${getActiveVoiceName() || "Default"}${tts.engine === "piper" && piperStatus.cuda ? " (CUDA)" : ""}`}
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
                <div className="flex items-center justify-between w-full">
                  <span>{eng.label}</span>
                  {voiceCounts[eng.id] > 0 && (
                    <span className="text-[9px] text-text/30 ml-1">
                      {t("footer.voicesCount", { count: voiceCounts[eng.id] })}
                    </span>
                  )}
                </div>
                {tts.engine === eng.id && (
                  <div className="w-1.5 h-1.5 rounded-full bg-logo-primary ml-1 shrink-0" />
                )}
              </button>
            ))}
          </div>

          {tts.engine === "piper" && piperStatus.error && (
            <div className="mt-2 p-1.5 bg-red-500/10 text-red-400 border border-red-500/20 rounded text-[10px] break-words">
              {piperStatus.error}
            </div>
          )}
          {isLocalEngine(tts.engine) &&
            tts.engine !== "piper" &&
            getLocalEngineStatus(tts.engine) === "error" && (
              <div className="mt-2 p-1.5 bg-red-500/10 text-red-400 border border-red-500/20 rounded text-[10px] break-words">
                {`${getEngineLabel(tts.engine)} engine error`}
              </div>
            )}
        </div>
      )}
    </div>
  );
};

export default TtsSelector;
