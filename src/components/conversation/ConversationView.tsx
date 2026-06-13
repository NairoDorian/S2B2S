import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Button } from "../ui/Button";
import { Textarea } from "../ui/Textarea";
import { useSettings } from "../../hooks/useSettings";
import { commands } from "@/bindings";
import { Mic, Volume2, VolumeX, Eraser } from "lucide-react";

interface Message {
  role: "user" | "assistant";
  content: string;
  sttMs?: number;
  tokensPerSec?: number;
  totalMs?: number;
  ttsMs?: number;
}

/**
 * Live transcript of the Speech → Brain → Speech loop.
 *
 * Turns arrive either from the converse hotkey (speech) or the text input
 * below; the assistant reply streams in via brain:* events and is spoken
 * aloud when read-aloud is enabled.
 */
export const ConversationView: React.FC = () => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const [messages, setMessages] = useState<Message[]>([]);
  const [draft, setDraft] = useState("");
  const [streaming, setStreaming] = useState("");
  const [thinking, setThinking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [voiceMode, setVoiceMode] = useState(false);
  const [voiceStatus, setVoiceStatus] = useState<
    | "idle"
    | "listening"
    | "speech_started"
    | "speech_ended"
    | "thinking"
    | "speaking"
  >("idle");
  const bottomRef = useRef<HTMLDivElement>(null);

  const brainEnabled = settings?.brain?.enabled ?? false;
  const converseBinding = settings?.bindings?.converse?.current_binding;
  const [readAloud, setReadAloud] = useState(
    settings?.brain?.read_aloud ?? true,
  );
  const [latencyHud, setLatencyHud] = useState<Record<string, number> | null>(
    null,
  );

  // Toggle voice mode
  const toggleVoiceMode = useCallback(async () => {
    if (voiceMode) {
      const res = await commands.stopContinuousVoiceMode();
      if (res.status === "ok") {
        setVoiceMode(false);
        setVoiceStatus("idle");
      } else {
        setError(String(res.error));
      }
    } else {
      const res = await commands.startContinuousVoiceMode();
      if (res.status === "ok") {
        setVoiceMode(true);
        setVoiceStatus("listening");
      } else {
        setError(String(res.error));
      }
    }
  }, [voiceMode]);

  // Clean up continuous voice mode on unmount
  useEffect(() => {
    return () => {
      void commands.stopContinuousVoiceMode();
    };
  }, []);

  useEffect(() => {
    const setup = async () => {
      const unlistenThinking = await listen("brain:thinking", () => {
        setThinking(true);
        setStreaming("");
        setError(null);
        setVoiceStatus((prev) => (prev !== "idle" ? "thinking" : "idle"));
      });
      const unlistenToken = await listen<string>("brain:token", (event) => {
        setThinking(false);
        setStreaming((prev) => prev + event.payload);
      });
      const unlistenDone = await listen<{ text: string; tokens_per_sec?: number; total_ms?: number; predicted_ms?: number }>(
        "brain:done",
        (event) => {
          setThinking(false);
          setStreaming("");
          const payload = event.payload;
          // Use predicted_ms (generation time) as totalMs, fall back to total_ms
          const genMs = typeof payload === "object" ? (payload.predicted_ms ?? payload.total_ms) : undefined;
          setMessages((prev) => [
            ...prev,
            {
              role: "assistant",
              content: typeof payload === "string" ? payload : payload.text,
              tokensPerSec: typeof payload === "object" ? payload.tokens_per_sec : undefined,
              totalMs: genMs,
            },
          ]);
          setVoiceStatus((prev) => {
            if (prev === "thinking" || prev === "speech_ended") {
              return "listening";
            }
            return prev;
          });
        },
      );
      const unlistenError = await listen<string>("brain:error", (event) => {
        setThinking(false);
        setStreaming("");
        setError(event.payload);
        setVoiceStatus((prev) => (prev !== "idle" ? "listening" : "idle"));
      });
      // Speech turns: surface the transcribed question in the transcript.
      const unlistenAsked = await listen<{ text: string; stt_ms?: number }>(
        "brain:asked",
        (event) => {
          const payload = event.payload;
          setMessages((prev) => [
            ...prev,
            {
              role: "user",
              content: typeof payload === "string" ? payload : payload.text,
              sttMs: typeof payload === "object" ? payload.stt_ms : undefined,
            },
          ]);
        },
      );

      // Continuous Voice Mode Events
      const unlistenSpeechStarted = await listen(
        "continuous-voice:speech-started",
        () => {
          setVoiceStatus("speech_started");
        },
      );
      const unlistenSpeechEnded = await listen(
        "continuous-voice:speech-ended",
        () => {
          setVoiceStatus("speech_ended");
        },
      );
      const unlistenTtsPlaying = await listen<boolean>(
        "tts:playing-changed",
        (event) => {
          if (event.payload) {
            setVoiceStatus("speaking");
          } else {
            setVoiceStatus((prev) =>
              prev === "speaking" ? "listening" : prev,
            );
          }
        },
      );

      // TTS synthesis timing — update the latest assistant message
      const unlistenTtsSynthDone = await listen<{ ms?: number }>(
        "tts:synth-done",
        (event) => {
          const synthMs = event.payload?.ms;
          if (synthMs != null) {
            setMessages((prev) => {
              const idx = prev.length - 1;
              if (idx < 0 || prev[idx].role !== "assistant") return prev;
              const updated = [...prev];
              updated[idx] = { ...updated[idx], ttsMs: synthMs };
              return updated;
            });
          }
        },
      );

      // Latency HUD events
      const unlistenLatency = await listen<{ stage: string; ms: number }>(
        "brain:latency",
        (event) => {
          setLatencyHud((prev) => ({
            ...prev,
            [event.payload.stage]: event.payload.ms,
          }));
        },
      );

      return () => {
        unlistenThinking();
        unlistenToken();
        unlistenDone();
        unlistenError();
        unlistenAsked();
        unlistenSpeechStarted();
        unlistenSpeechEnded();
        unlistenTtsPlaying();
        unlistenTtsSynthDone();
        unlistenLatency();
      };
    };
    const cleanup = setup();
    return () => {
      void cleanup.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, streaming, thinking]);

  const send = useCallback(async () => {
    const text = draft.trim();
    if (!text) return;
    setDraft("");
    setError(null);
    // brain:asked covers speech turns; text turns are added locally.
    setMessages((prev) => [...prev, { role: "user", content: text }]);
    const result = await commands.brainAsk(text);
    if (result.status === "error") {
      setError(String(result.error));
    }
  }, [draft]);

  const clear = useCallback(async () => {
    await commands.brainClearHistory();
    setMessages([]);
    setStreaming("");
    setError(null);
  }, []);

  return (
    <div className="flex flex-col h-full min-h-[400px] space-y-3">
      {!brainEnabled && (
        <div className="px-4 py-3 rounded-lg border border-mid-gray/20 text-sm text-mid-gray">
          {t("conversation.disabledHint")}
        </div>
      )}
      {brainEnabled && (
        <div className="flex items-center justify-between px-2">
          {converseBinding && !voiceMode && (
            <div className="px-4 py-2 rounded-lg border border-mid-gray/20 text-xs text-mid-gray">
              {t("conversation.hotkeyHint", { hotkey: converseBinding })}
            </div>
          )}
          <div className="flex items-center gap-1 ms-auto">
            <button
              onClick={() => {
                const newVal = !readAloud;
                setReadAloud(newVal);
                if (settings && settings.brain) {
                  void commands.changeBrainConfig({
                    ...settings.brain,
                    read_aloud: newVal,
                  });
                }
              }}
              className="p-1.5 rounded-md text-mid-gray hover:text-foreground hover:bg-mid-gray/10 transition-colors"
              title={
                readAloud ? t("conversation.readAloudOn") : t("conversation.readAloudOff")
              }
            >
              {readAloud ? <Volume2 size={16} /> : <VolumeX size={16} />}
            </button>
          </div>
        </div>
      )}

      {voiceMode && (
        <div className="flex items-center justify-between px-4 py-3 rounded-xl border border-logo-primary/30 bg-logo-primary/5 shadow-sm transition-all duration-300">
          <div className="flex items-center space-x-3">
            <div className="relative flex h-3 w-3">
              {voiceStatus === "listening" && (
                <>
                  <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
                  <span className="relative inline-flex rounded-full h-3 w-3 bg-green-500"></span>
                </>
              )}
              {voiceStatus === "speech_started" && (
                <>
                  <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-red-400 opacity-75"></span>
                  <span className="relative inline-flex rounded-full h-3 w-3 bg-red-500"></span>
                </>
              )}
              {voiceStatus === "speech_ended" && (
                <>
                  <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-yellow-400 opacity-75"></span>
                  <span className="relative inline-flex rounded-full h-3 w-3 bg-yellow-500"></span>
                </>
              )}
              {voiceStatus === "thinking" && (
                <>
                  <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-purple-400 opacity-75"></span>
                  <span className="relative inline-flex rounded-full h-3 w-3 bg-purple-500"></span>
                </>
              )}
              {voiceStatus === "speaking" && (
                <>
                  <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-blue-400 opacity-75"></span>
                  <span className="relative inline-flex rounded-full h-3 w-3 bg-blue-500"></span>
                </>
              )}
            </div>
            <span className="text-sm font-medium text-logo-primary">
              {voiceStatus === "listening" && t("conversation.voiceListening")}
              {voiceStatus === "speech_started" &&
                t("conversation.voiceCapturing")}
              {voiceStatus === "speech_ended" &&
                t("conversation.voiceProcessing")}
              {voiceStatus === "thinking" && t("conversation.voiceThinking")}
              {voiceStatus === "speaking" && t("conversation.voiceSpeaking")}
            </span>
          </div>
          <button
            onClick={toggleVoiceMode}
            className="text-xs font-semibold px-2.5 py-1 rounded-md bg-logo-primary/10 text-logo-primary hover:bg-logo-primary/20 transition-colors"
          >
            {t("conversation.stopVoiceMode", "Exit Voice Mode")}
          </button>
        </div>
      )}

      <div className="flex-1 overflow-y-auto space-y-3 px-1">
        {messages.length === 0 && !streaming && !thinking && (
          <p className="text-sm text-mid-gray px-3 py-6 text-center">
            {t("conversation.empty")}
          </p>
        )}
        {messages.map((message, i) => (
          <div
            key={i}
            className={`max-w-[85%] rounded-lg px-3 py-2 text-sm whitespace-pre-wrap ${
              message.role === "user"
                ? "bg-logo-primary/20 ms-auto"
                : "bg-mid-gray/10"
            }`}
          >
            {message.content}
            {message.role === "user" && message.sttMs != null && (
              <div className="mt-1.5 pt-1.5 border-t border-text/5 text-[10px] text-text/30 font-mono">
                <span>🎤 {message.sttMs}ms</span>
              </div>
            )}
            {message.role === "assistant" && (
              <div className="mt-1.5 pt-1.5 border-t border-text/5 flex gap-3 text-[10px] text-text/30 font-mono">
                {message.tokensPerSec != null && (
                  <span>{message.tokensPerSec.toFixed(1)} t/s</span>
                )}
                {message.totalMs != null && (
                  <span>🧠 {message.totalMs}ms</span>
                )}
                {message.ttsMs != null && (
                  <span>🔊 {message.ttsMs}ms</span>
                )}
              </div>
            )}
          </div>
        ))}
        {thinking && (
          <div className="max-w-[85%] rounded-lg px-3 py-2 text-sm bg-mid-gray/10 animate-pulse">
            {t("conversation.thinking")}
          </div>
        )}
        {streaming && (
          <div className="max-w-[85%] rounded-lg px-3 py-2 text-sm bg-mid-gray/10 whitespace-pre-wrap">
            {streaming}
          </div>
        )}
        {error && (
          <div className="max-w-[85%] rounded-lg px-3 py-2 text-sm text-red-500 border border-red-500/30">
            {error}
          </div>
        )}
        <div ref={bottomRef} />
      </div>

      <div className="space-y-2">
        <Textarea
          variant="compact"
          rows={2}
          value={draft}
          placeholder={t("conversation.inputPlaceholder")}
          disabled={!brainEnabled}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              void send();
            }
          }}
        />
        <div className="flex gap-2">
          <Button
            variant="primary-soft"
            size="sm"
            disabled={!brainEnabled || !draft.trim()}
            onClick={() => void send()}
          >
            {t("conversation.send")}
          </Button>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void commands.brainAbort()}
          >
            {t("conversation.stop")}
          </Button>
          <Button variant="ghost" size="sm" onClick={() => void clear()} className="flex items-center gap-1">
            <Eraser size={13} />
            {t("conversation.newConversation")}
          </Button>
          <Button
            variant={voiceMode ? "primary" : "secondary"}
            size="sm"
            disabled={!brainEnabled}
            onClick={toggleVoiceMode}
            className="ml-auto flex items-center gap-1.5"
          >
            <Mic
              size={14}
              className={
                voiceStatus !== "idle" && voiceStatus !== "listening"
                  ? "animate-pulse"
                  : ""
              }
            />
            {voiceMode ? t("conversation.voiceModeOn") : t("conversation.voiceMode")}
          </Button>
        </div>
      </div>
    </div>
  );
};
