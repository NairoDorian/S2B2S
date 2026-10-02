/* oxlint-disable i18next/no-literal-string */
import { createSignal, onCleanup, onSettled, Show } from "solid-js";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useTranslation } from "@/i18n/useTranslation";
import { sessionToast as toast } from "@/lib/sessionToast";
import { copyToClipboard } from "@/components/settings/history/clipboard";
import { useSettings } from "@/hooks/useSettings";
import { useLiveModeStore } from "@/stores/liveModeStore";
import { OverlayScope } from "@/overlay/OverlayScope";
import {
  OVERLAY_SCOPE_DEFAULTS,
  type ResolvedOverlayScope,
} from "@/lib/overlayScope";
import {
  joinStreamText,
  splitReveal,
  finishReveal,
  NO_REVEAL,
  type StreamReveal,
} from "@/lib/streamReveal";
import type { StreamTextEvent } from "@/bindings";
import {
  Copy,
  Trash2,
  Cog,
  ChevronUp,
  ChevronDown,
  Mic,
  Activity,
} from "@/components/icons/lucide";

interface LiveCanvasProps {
  onExit: () => void;
}

const LIVE_SCOPE_CONFIG: ResolvedOverlayScope = {
  ...OVERLAY_SCOPE_DEFAULTS,
  show_circular: true,
  circular_bars: true,
  circular_size: 260,
  wave_inside_circular: true,
  show_wave: true,
  show_spectrum: false,
  circular_background: false,
  circular_gain: 2.2,
  circular_floor: 0.1,
};

const fmtTime = (s: number) =>
  `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;

export const LiveCanvas = (props: LiveCanvasProps) => {
  const { t } = useTranslation();
  const { settings } = useSettings();

  // Transcript state
  const [streamText, setStreamText] = createSignal<StreamTextEvent>({
    committed: "",
    tentative: "",
  });
  const [historyText, setHistoryText] = createSignal("");
  const [speaking, setSpeaking] = createSignal(false);
  const [elapsed, setElapsed] = createSignal(0);
  const [wordCount, setWordCount] = createSignal(0);
  const [speechMs, setSpeechMs] = createSignal(0);
  const [recording, setRecording] = createSignal(false);

  // UI interaction state
  const [telemetryOpen, setTelemetryOpen] = createSignal(false);
  const [hudVisible, setHudVisible] = createSignal(true);
  const [copied, setCopied] = createSignal(false);

  let hudTimeout: ReturnType<typeof setTimeout> | null = null;
  let typewriterTimer: ReturnType<typeof setInterval> | null = null;
  let targetText: StreamTextEvent = { committed: "", tentative: "" };
  let reveal: StreamReveal = NO_REVEAL;

  const fullTranscript = () => {
    const live = joinStreamText(
      streamText().committed,
      streamText().tentative,
    ).trim();
    if (historyText() && live) {
      return `${historyText()}\n${live}`;
    }
    return historyText() || live;
  };

  const wpm = () => {
    if (speechMs() >= 2500 && wordCount() >= 3) {
      return Math.round(wordCount() / (speechMs() / 60000));
    }
    return null;
  };

  // Reset or bump HUD idle timer on mouse movement
  const bumpUserActivity = () => {
    setHudVisible(true);
    if (hudTimeout) clearTimeout(hudTimeout);
    hudTimeout = setTimeout(() => {
      // Keep HUD hidden when speaking or idle unless hovered
      if (speaking()) {
        setHudVisible(false);
      }
    }, 3500);
  };

  const handleCopy = async () => {
    const text = fullTranscript();
    if (!text) return;
    const ok = await copyToClipboard(text);
    if (ok) {
      setCopied(true);
      toast.success(t("history.copied"));
      setTimeout(() => setCopied(false), 2000);
    }
  };

  const handleClear = () => {
    setStreamText({ committed: "", tentative: "" });
    setHistoryText("");
    setWordCount(0);
    setSpeechMs(0);
    setElapsed(0);
    targetText = { committed: "", tentative: "" };
    reveal = NO_REVEAL;
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      props.onExit();
    } else if (
      (e.ctrlKey || e.metaKey) &&
      e.key.toLowerCase() === "c" &&
      !window.getSelection()?.toString()
    ) {
      e.preventDefault();
      void handleCopy();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "l") {
      e.preventDefault();
      handleClear();
    }
  };

  onSettled(() => {
    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("mousemove", bumpUserActivity);

    let unlistens: UnlistenFn[] = [];

    const setupListeners = async () => {
      // 1. Live stream text events from backend engine
      const u1 = await listen<StreamTextEvent>("stream-text", (event) => {
        targetText = event.payload;
        // Direct reveal
        reveal = finishReveal(
          joinStreamText(targetText.committed, targetText.tentative),
        );
        setStreamText(splitReveal(reveal, targetText.committed.length));
        bumpUserActivity();
      });

      // 2. Speech stats
      const u2 = await listen<{
        speaking: boolean;
        speech_ms: number;
        word_count: number;
      }>("speech-stats", (event) => {
        setSpeaking(event.payload.speaking);
        setSpeechMs(event.payload.speech_ms);
        setWordCount(event.payload.word_count);
      });

      // 3. Recording lifecycle
      const u3 = await listen("show-overlay", () => {
        setRecording(true);
      });

      const u4 = await listen("recording-stopped", () => {
        setRecording(false);
        setSpeaking(false);
        // Commit current text to history accumulator
        const current = joinStreamText(
          streamText().committed,
          streamText().tentative,
        ).trim();
        if (current) {
          setHistoryText((prev) => (prev ? `${prev} ${current}` : current));
          setStreamText({ committed: "", tentative: "" });
        }
      });

      unlistens = [u1, u2, u3, u4];
    };

    void setupListeners();

    // Elapsed session counter
    const timer = setInterval(() => {
      if (
        recording() ||
        speaking() ||
        useLiveModeStore().status.phase !== "idle"
      ) {
        setElapsed((e) => e + 1);
      }
    }, 1000);

    onCleanup(() => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("mousemove", bumpUserActivity);
      if (hudTimeout) clearTimeout(hudTimeout);
      if (typewriterTimer) clearInterval(typewriterTimer);
      clearInterval(timer);
      for (const u of unlistens) u();
    });
  });

  const activeModel = () =>
    settings()?.selected_model?.split("/").pop()?.replace(".gguf", "") ||
    "Auto";

  return (
    <div
      class="fixed inset-0 z-50 bg-[#0b0e11] text-[#e6f1f3] flex flex-col justify-between overflow-hidden select-none font-sans"
      onMouseMove={bumpUserActivity}
    >
      {/* Top HUD bar (auto-fades during speech or idle) */}
      <header
        class={`flex items-center justify-between px-6 py-4 border-b border-[#e6f1f3]/10 transition-opacity duration-200 z-10 ${
          hudVisible() ? "opacity-100" : "opacity-0 pointer-events-none"
        }`}
      >
        {/* Left status cluster */}
        <div class="flex items-center gap-3">
          <div class="flex items-center gap-2 px-2.5 py-1 bg-[#1fe0ff]/10 border border-[#1fe0ff]/30 text-xs font-mono">
            <span
              class={`w-2 h-2 ${
                speaking() || recording()
                  ? "bg-[#1fe0ff] animate-pulse"
                  : "bg-mid-gray/60"
              }`}
            />
            <span class="text-[#e6f1f3] uppercase font-semibold">
              {speaking()
                ? t("overlay.listening")
                : recording()
                  ? t("overlay.processing")
                  : "READY"}
            </span>
          </div>
          <span class="text-xs font-mono text-mid-gray">
            {fmtTime(elapsed())}
          </span>
          <span class="text-xs font-mono text-mid-gray/70 border-s border-mid-gray/20 ps-3">
            {activeModel()}
          </span>
        </div>

        {/* Right action controls */}
        <div class="flex items-center gap-2">
          <button
            type="button"
            onClick={handleCopy}
            title={`${t("common.copy")} (Ctrl+C)`}
            class="flex items-center gap-1.5 px-3 py-1 text-xs border border-mid-gray/30 bg-mid-gray/10 hover:border-[#1fe0ff] hover:text-[#1fe0ff] transition-colors cursor-pointer"
          >
            <Copy width={13} height={13} />
            <span>{copied() ? t("history.copied") : t("common.copy")}</span>
          </button>

          <button
            type="button"
            onClick={handleClear}
            title={`${t("common.clear")} (Ctrl+L)`}
            class="flex items-center gap-1.5 px-3 py-1 text-xs border border-mid-gray/30 bg-mid-gray/10 hover:border-red-500/60 hover:text-red-400 transition-colors cursor-pointer"
          >
            <Trash2 width={13} height={13} />
            <span>{t("common.clear")}</span>
          </button>

          <button
            type="button"
            onClick={props.onExit}
            title={`${t("settings.title")} (Esc)`}
            class="flex items-center gap-1.5 px-3 py-1 text-xs border border-mid-gray/30 bg-mid-gray/10 hover:border-[#1fe0ff] hover:text-[#1fe0ff] transition-colors cursor-pointer ms-2"
          >
            <Cog width={13} height={13} />
            <span>ESC</span>
          </button>
        </div>
      </header>

      {/* Center Stage: Circular FFT Scope & Sound Geometry */}
      <main class="flex-1 flex flex-col items-center justify-center relative px-6">
        <div class="relative flex items-center justify-center">
          {/* Circular Scope Canvas */}
          <OverlayScope
            rateHz={30}
            ready={true}
            quiet={!speaking()}
            config={LIVE_SCOPE_CONFIG}
          />

          {/* Center microphone glyph when quiet */}
          <Show when={!speaking()}>
            <div class="absolute inset-0 flex items-center justify-center pointer-events-none opacity-20 text-[#1fe0ff]">
              <Mic width={32} height={32} />
            </div>
          </Show>
        </div>

        {/* Live Transcription Display Area */}
        <div class="w-full max-w-4xl mx-auto mt-8 px-6 text-center z-10">
          <div class="min-h-[100px] flex flex-col items-center justify-center">
            <Show
              when={fullTranscript().length > 0}
              fallback={
                <p class="text-lg md:text-xl text-mid-gray/50 italic font-light tracking-wide">
                  {t("overlay.awaitingText")}
                </p>
              }
            >
              <div class="text-xl md:text-2xl font-sans tracking-wide leading-relaxed text-[#e6f1f3]">
                <Show when={historyText()}>
                  <span class="text-[#e6f1f3]/70 me-2">{historyText()}</span>
                </Show>
                <span class="text-[#e6f1f3] font-medium">
                  {streamText().committed}
                </span>
                <span class="text-[#1fe0ff] ps-0.5">
                  {streamText().tentative}
                </span>
                <span class="inline-block w-2 h-5 bg-[#1fe0ff] ms-1 align-middle animate-pulse" />
              </div>
            </Show>
          </div>
        </div>
      </main>

      {/* Collapsible Telemetry Drawer at Bottom */}
      <footer class="flex flex-col items-center border-t border-[#e6f1f3]/10 bg-[#0b0e11] z-10">
        {/* Drawer toggle handle */}
        <button
          type="button"
          onClick={() => setTelemetryOpen(!telemetryOpen())}
          class="flex items-center gap-1.5 px-4 py-1 text-[11px] font-mono text-mid-gray hover:text-[#1fe0ff] transition-colors cursor-pointer border-x border-b border-mid-gray/20 bg-mid-gray/5 -mt-[1px]"
        >
          <Activity width={11} height={11} />
          <span>TELEMETRY</span>
          <Show
            when={telemetryOpen()}
            fallback={<ChevronUp width={12} height={12} />}
          >
            <ChevronDown width={12} height={12} />
          </Show>
        </button>

        {/* Telemetry drawer content */}
        <Show when={telemetryOpen()}>
          <div class="w-full max-w-4xl px-8 py-3 grid grid-cols-2 sm:grid-cols-4 gap-4 text-xs font-mono text-mid-gray">
            <div class="flex flex-col">
              <span class="text-[10px] text-mid-gray/70 uppercase">Rate</span>
              <span class="text-[#e6f1f3] text-sm font-semibold">
                {wpm() ? `${wpm()} WPM` : "—"}
              </span>
            </div>

            <div class="flex flex-col">
              <span class="text-[10px] text-mid-gray/70 uppercase">Words</span>
              <span class="text-[#e6f1f3] text-sm font-semibold">
                {wordCount()}
              </span>
            </div>

            <div class="flex flex-col">
              <span class="text-[10px] text-mid-gray/70 uppercase">Speech</span>
              <span class="text-[#e6f1f3] text-sm font-semibold">
                {speechMs() > 0 ? `${(speechMs() / 1000).toFixed(1)}s` : "0.0s"}
              </span>
            </div>

            <div class="flex flex-col">
              <span class="text-[10px] text-mid-gray/70 uppercase">Status</span>
              <span
                class={`text-sm font-semibold ${
                  speaking() ? "text-[#1fe0ff]" : "text-mid-gray"
                }`}
              >
                {speaking() ? "ACTIVE" : "SILENCE"}
              </span>
            </div>
          </div>
        </Show>
      </footer>
    </div>
  );
};

export default LiveCanvas;
