import { createSignal, createEffect, onCleanup, Show, For } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTranslation } from "@/i18n/useTranslation";
import { isLiveActive, useLiveModeStore } from "@/stores/liveModeStore";
import { OverlayScope } from "@/overlay/OverlayScope";
import {
  OVERLAY_SCOPE_DEFAULTS,
  type ResolvedOverlayScope,
} from "@/lib/overlayScope";
import { joinStreamText } from "@/lib/streamReveal";
import { commands, events, type StreamTextEvent } from "@/bindings";
import { Mic, Activity } from "@/components/icons/lucide";
import { useLiveCanvasMotionStore } from "@/stores/liveCanvasMotionStore";
import { liveCanvasTiming } from "@/lib/liveCanvasMotion";
import {
  CanvasTranscript,
  stepCanvasText,
  canvasCorrection,
} from "@/lib/liveCanvasTranscript";

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
// Serialize native changes across rapid exits and re-entries.
let windowUpdate: Promise<unknown> = Promise.resolve();
// Window chrome follows the canvas; a failure throws so the caller reports it.
const setCanvasWindow = async (active: boolean, recording: boolean) => {
  const result = await commands.setLiveCanvasActive(active, recording);
  if (result.status === "error") throw new Error(result.error);
};
// How long a burst of resize events is left to settle before the maximized
// state is read back (a drag-resize fires one per frame).
const MAXIMIZED_POLL_DEBOUNCE_MS = 150;

export const LiveCanvas = (props: LiveCanvasProps) => {
  const { t } = useTranslation();
  const store = useLiveModeStore();
  const motionStore = useLiveCanvasMotionStore();
  const active = () => isLiveActive(store.status);
  const [pending, setPending] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [visible, setVisible] = createSignal(!document.hidden);
  const [telemetryOpen, setTelemetryOpen] = createSignal(false);
  const [maximized, setMaximized] = createSignal(false);
  const [primaryText, setPrimaryText] = createSignal<StreamTextEvent>({
    committed: "",
    tentative: "",
  });
  const [streamText, setStreamText] = createSignal<StreamTextEvent>({
    committed: "",
    tentative: "",
  });
  const [clearedPrefix, setClearedPrefix] = createSignal("");
  const [arrival, setArrival] = createSignal<
    { letter: string; removing?: boolean }[]
  >([]);
  const [shownPrefix, setShownPrefix] = createSignal("");
  let surface: HTMLDivElement | undefined;
  let analyzer: HTMLDivElement | undefined;
  let particle: HTMLSpanElement | undefined;
  let arrivalElement: HTMLSpanElement | undefined;
  let paintFrame: number | null = null;
  let flight: Animation | null = null;
  let letterFade: Animation | null = null;
  let typewriterTimer: ReturnType<typeof setTimeout> | null = null;
  let targetText: StreamTextEvent = { committed: "", tentative: "" };
  let revealedText = "";
  let deleting = false;
  let correctionAnimations: Animation[] = [];
  let correctionGeneration = 0;
  let transcript = new CanvasTranscript();
  const scopeConfig = (): ResolvedOverlayScope => ({
    ...LIVE_SCOPE_CONFIG,
    circular_signal_scale: motionStore.motion.signal_scale,
    wave_signal_scale: motionStore.motion.signal_scale,
  });
  const wordsPerMinute = () => {
    const elapsed = store.status.elapsed_ms ?? 0;
    const settledCount = primaryText()
      .committed.slice(clearedPrefix().length)
      .trim()
      .split(/\s+/)
      .filter(Boolean).length;
    return elapsed > 0 ? Math.round((settledCount * 60000) / elapsed) : 0;
  };
  const fullTranscript = () =>
    joinStreamText(streamText().committed, streamText().tentative).trim();
  const wordCount = () => fullTranscript().split(/\s+/).filter(Boolean).length;
  const cancelArrival = () => {
    correctionGeneration++;
    for (const animation of correctionAnimations) animation.cancel();
    correctionAnimations = [];
    if (paintFrame !== null) cancelAnimationFrame(paintFrame);
    paintFrame = null;
    flight?.cancel();
    letterFade?.cancel();
    flight = null;
    letterFade = null;
    deleting = false;
  };
  const handleClear = () => {
    setClearedPrefix(
      joinStreamText(primaryText().committed, primaryText().tentative),
    );
    setStreamText({ committed: "", tentative: "" });
    targetText = { committed: "", tentative: "" };
    revealedText = "";
    cancelArrival();
    setShownPrefix("");
    setArrival([]);
    if (typewriterTimer) clearTimeout(typewriterTimer);
    typewriterTimer = null;
  };
  const toggleRecording = async () => {
    if (pending()) return;
    setPending(true);
    setError(null);
    try {
      if (!active()) {
        transcript = new CanvasTranscript();
        setClearedPrefix("");
        setPrimaryText({ committed: "", tentative: "" });
      }
      const failure = await (active() ? store.stop() : store.start());
      if (failure) setError(failure);
    } catch (failure) {
      setError(String(failure));
    } finally {
      setPending(false);
    }
  };
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      props.onExit();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "l") {
      e.preventDefault();
      handleClear();
    }
  };
  const visibilityChanged = () => setVisible(!document.hidden);
  // The canvas hides the native title bar, so it carries its own minimize and
  // maximize controls; this keeps their icon in step with the window (a
  // double-click on the drag strip maximizes too).
  const refreshMaximized = () =>
    getCurrentWindow()
      .isMaximized()
      .then(setMaximized)
      .catch(() => {});
  const minimizeWindow = () =>
    getCurrentWindow()
      .minimize()
      .catch((failure) => setError(String(failure)));
  const toggleMaximized = () =>
    getCurrentWindow()
      .toggleMaximize()
      .then(refreshMaximized)
      .catch((failure) => setError(String(failure)));
  createEffect(
    () => undefined,
    () => {
      let disposed = false;
      document.documentElement.setAttribute("data-live-canvas", "");
      window.addEventListener("keydown", handleKeyDown);
      document.addEventListener("visibilitychange", visibilityChanged);
      void refreshMaximized();
      let resizeSettle: ReturnType<typeof setTimeout> | null = null;
      const resized = getCurrentWindow().onResized(() => {
        if (resizeSettle) clearTimeout(resizeSettle);
        resizeSettle = setTimeout(() => {
          resizeSettle = null;
          if (!disposed) void refreshMaximized();
        }, MAXIMIZED_POLL_DEBOUNCE_MS);
      });
      void resized.catch(() => {});
      // Raw primary hypotheses only: no extra slots, file headers or LLM output.
      const subscription = events.streamTextEvent.listen(({ payload }) => {
        if (!disposed && (payload.slot == null || payload.slot === 0)) {
          setPrimaryText({
            committed: transcript.stream(payload.committed, payload.tentative),
            tentative: "",
          });
        }
      });
      const finalizedSubscription = events.liveModeTranscriptEvent.listen(
        ({ payload }) => {
          if (!disposed && !payload.reset && payload.stable_appended)
            setPrimaryText({
              committed: transcript.finish(payload.stable_appended),
              tentative: "",
            });
        },
      );
      void finalizedSubscription.catch((failure) => {
        if (!disposed) setError(String(failure));
      });
      void subscription.catch((failure) => {
        if (!disposed) setError(String(failure));
      });
      void store.initialize().catch((failure) => {
        if (!disposed) setError(String(failure));
      });
      return () => {
        disposed = true;
        if (resizeSettle) clearTimeout(resizeSettle);
        document.documentElement.removeAttribute("data-live-canvas");
        window.removeEventListener("keydown", handleKeyDown);
        document.removeEventListener("visibilitychange", visibilityChanged);
        void resized.then((unlisten) => unlisten()).catch(() => {});
        void subscription.then((unlisten) => unlisten()).catch(console.error);
        void finalizedSubscription
          .then((unlisten) => unlisten())
          .catch(console.error);
        windowUpdate = windowUpdate
          .then(() => setCanvasWindow(false, false))
          .catch(console.error);
      };
    },
  );
  createEffect(
    () => active() && visible(),
    (isRecording) => {
      windowUpdate = windowUpdate
        .then(() => setCanvasWindow(true, isRecording))
        .catch((failure) => setError(String(failure)));
    },
  );
  // One point, one arrival, no permanent animation loop. Read the actual glyph
  // bounds after Solid paints so wrapping and window scaling stay accurate.
  const animateArrival = () => {
    cancelArrival();
    paintFrame = requestAnimationFrame(() => {
      paintFrame = null;
      if (!surface || !analyzer || !particle || !arrivalElement) return;
      const root = surface.getBoundingClientRect();
      const source = analyzer.getBoundingClientRect();
      const destination = arrivalElement.getBoundingClientRect();
      const scale = root.width / surface.offsetWidth || 1;
      const x0 = (source.x + source.width / 2 - root.x) / scale - 2;
      const y0 = (source.y + source.height / 2 - root.y) / scale - 2;
      const x1 = (destination.x + destination.width / 2 - root.x) / scale - 2;
      const y1 = (destination.y + destination.height / 2 - root.y) / scale - 2;
      const timing = liveCanvasTiming(motionStore.motion);
      const { duration } = timing;
      const landing = timing.travel / duration;
      const disappeared = (timing.travel + timing.particleFade) / duration;
      flight = particle.animate(
        [
          {
            transform: `translate(${x0}px, ${y0}px) scale(1)`,
            opacity: 1,
            offset: 0,
          },
          {
            transform: `translate(${x0 + (x1 - x0) * 0.12}px, ${y0 + (y1 - y0) * 0.12}px) scale(1)`,
            opacity: 1,
            offset: landing * 0.12,
          },
          {
            transform: `translate(${x1}px, ${y1}px) scale(1)`,
            opacity: 1,
            offset: landing,
          },
          {
            transform: `translate(${x1}px, ${y1}px) scale(2.4)`,
            opacity: 0,
            offset: disappeared,
          },
          {
            transform: `translate(${x1}px, ${y1}px) scale(2.4)`,
            opacity: 0,
            offset: 1,
          },
        ],
        { duration, easing: "linear" },
      );
      letterFade = arrivalElement.animate([{ opacity: 0 }, { opacity: 1 }], {
        delay: timing.characterStart,
        duration: timing.characterFade,
        easing: "ease-out",
        fill: "both",
      });
      flight.onfinish = () => {
        flight = null;
        const latest = joinStreamText(
          targetText.committed,
          targetText.tentative,
        );
        if (typewriterTimer === null && revealedText !== latest) {
          typewriterTimer = setTimeout(tickReveal, 0);
        }
      };
    });
  };
  // Fade the glyphs at `indices` of the settled row; resolves when all finish.
  const fade = (
    indices: number[],
    from: number,
    to: number,
    duration: number,
  ) => {
    const glyphs =
      surface?.querySelectorAll(".live-canvas-prefix-letter") ?? [];
    correctionAnimations = indices.flatMap((index) =>
      glyphs[index]
        ? [
            glyphs[index].animate([{ opacity: from }, { opacity: to }], {
              duration,
              fill: "both",
              easing: "linear",
            }),
          ]
        : [],
    );
    return Promise.all(
      correctionAnimations.map((animation) => animation.finished),
    );
  };
  const animateCorrection = (
    plan: NonNullable<ReturnType<typeof canvasCorrection>>,
  ) => {
    cancelArrival();
    deleting = true;
    const generation = correctionGeneration;
    // Put the completed arrival into the normal glyph row before morphing.
    setShownPrefix(revealedText);
    setArrival([]);
    paintFrame = requestAnimationFrame(() => {
      paintFrame = null;
      const motion = motionStore.motion;
      void fade(plan.oldChanged, 1, 0, motion.correction_out_ms)
        .then(() => {
          if (generation !== correctionGeneration) return;
          for (const animation of correctionAnimations) animation.cancel();
          correctionAnimations = [];
          revealedText = plan.corrected;
          setShownPrefix(plan.corrected);
          setStreamText({ committed: plan.corrected, tentative: "" });
          paintFrame = requestAnimationFrame(() => {
            paintFrame = null;
            void fade(plan.newChanged, 0, 1, motion.correction_in_ms)
              .then(() => {
                if (generation !== correctionGeneration) return;
                for (const animation of correctionAnimations)
                  animation.cancel();
                correctionAnimations = [];
                deleting = false;
                typewriterTimer = setTimeout(tickReveal, 0);
              })
              .catch(() => {});
          });
        })
        .catch(() => {});
    });
  };
  // One timeout only while a new primary hypothesis is being revealed.
  const tickReveal = () => {
    typewriterTimer = null;
    // This canvas is an explicitly selected particle experience. OS reduced
    // motion must not silently replace its user-tuned reveal with a text dump.
    // Hidden windows pause the queue instead of flushing it in the background.
    if (!visible()) return;
    const full = joinStreamText(targetText.committed, targetText.tentative);
    const correction = canvasCorrection(revealedText, full);
    if (correction) {
      animateCorrection(correction);
      return;
    }
    const step = stepCanvasText(revealedText, full);
    if (!step) return;
    const previous = revealedText;
    revealedText = step.next;
    const { letter } = step;
    setStreamText({
      committed: step.next,
      tentative: "",
    });
    const moving = !!letter.trim();
    setShownPrefix(moving ? previous : step.next);
    setArrival(moving ? [{ letter }] : []);
    if (moving) animateArrival();
    else {
      cancelArrival();
      if (revealedText !== full) typewriterTimer = setTimeout(tickReveal, 24);
    }
  };
  createEffect(
    () => [primaryText(), clearedPrefix(), visible()] as const,
    ([primary, prefix]) => {
      const full = joinStreamText(primary.committed, primary.tentative);
      const offset = prefix && full.startsWith(prefix) ? prefix.length : 0;
      targetText = {
        committed: primary.committed.slice(
          Math.min(offset, primary.committed.length),
        ),
        tentative: primary.tentative.slice(
          Math.max(0, offset - primary.committed.length),
        ),
      };
      // Take the newest hypothesis on the next edit without interrupting
      // the current particle, glyph arrival or backspace fade.
      if (
        typewriterTimer === null &&
        paintFrame === null &&
        !deleting &&
        flight?.playState !== "running"
      )
        tickReveal();
    },
  );
  onCleanup(() => {
    if (typewriterTimer) clearTimeout(typewriterTimer);
    cancelArrival();
  });
  return (
    <div
      ref={(el) => (surface = el)}
      class="live-canvas fixed inset-0 z-50 text-[#e6f1f3] overflow-hidden select-none font-sans"
    >
      <span
        ref={(el) => (particle = el)}
        class="live-canvas-particle"
        aria-hidden="true"
      />
      <header data-tauri-drag-region class="live-canvas-drag" />
      <div class="live-canvas-exit-zone">
        <button
          type="button"
          onClick={minimizeWindow}
          class="live-canvas-exit"
          aria-label={t("settings.liveCanvas.window.minimize")}
          title={t("settings.liveCanvas.window.minimize")}
        >
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            aria-hidden="true"
          >
            <path d="M5 12h14" />
          </svg>
        </button>
        <button
          type="button"
          onClick={toggleMaximized}
          class="live-canvas-exit"
          aria-label={
            maximized()
              ? t("settings.liveCanvas.window.restore")
              : t("settings.liveCanvas.window.maximize")
          }
          title={
            maximized()
              ? t("settings.liveCanvas.window.restore")
              : t("settings.liveCanvas.window.maximize")
          }
        >
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <Show
              when={maximized()}
              fallback={<rect x="5" y="5" width="14" height="14" />}
            >
              <path d="M8 8V5h11v11h-3M5 8h11v11H5z" />
            </Show>
          </svg>
        </button>
        <button
          type="button"
          onClick={props.onExit}
          class="live-canvas-exit"
          aria-label={t("common.close")}
          title={`${t("common.close")} (Esc)`}
        >
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path d="M14 3h7v7M21 3l-9 9M10 3H4a1 1 0 0 0-1 1v16a1 1 0 0 0 1 1h16a1 1 0 0 0 1-1v-6" />
          </svg>
        </button>
      </div>
      <main>
        <div ref={(el) => (analyzer = el)} class="live-canvas-analyzer">
          <OverlayScope
            rateHz={30}
            ready={active() && visible()}
            quiet={!active()}
            config={scopeConfig()}
          />
          <button
            type="button"
            onClick={toggleRecording}
            class="live-canvas-microphone"
            aria-label={
              active()
                ? t("settings.liveMode.stop")
                : t("settings.liveMode.start")
            }
            aria-pressed={active() ? "true" : "false"}
            disabled={
              pending() ||
              store.status.phase === "starting" ||
              store.status.phase === "stopping"
            }
          >
            <Show when={!active()}>
              <Mic width={24} height={24} />
            </Show>
          </button>
        </div>
        <div class="live-canvas-transcript transcript-text select-text break-words">
          <Show when={fullTranscript()}>
            <div>
              <span>
                <For each={Array.from(shownPrefix())}>
                  {(letter) => (
                    <span class="live-canvas-prefix-letter">{letter}</span>
                  )}
                </For>
              </span>
              <For each={arrival()}>
                {(glyph) => (
                  <span
                    ref={(el) => (arrivalElement = el)}
                    class="live-canvas-arriving-letter"
                    style={{ opacity: 0 }}
                  >
                    {glyph.letter}
                  </span>
                )}
              </For>
            </div>
          </Show>
        </div>
      </main>
      <Show when={error() || store.status.error}>
        <p role="alert" class="live-canvas-error">
          {error() || store.status.error}
        </p>
      </Show>
      <footer>
        <div class="live-canvas-tools">
          <button
            type="button"
            onClick={handleClear}
            disabled={!fullTranscript()}
            aria-label={t("common.clear")}
            title={`${t("common.clear")} (Ctrl+L)`}
          >
            <svg
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.5"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <path d="M20.5 8A9 9 0 1 0 21 15M21 3v5h-5M9 9l6 6M15 9l-6 6" />
            </svg>
          </button>
          <button
            type="button"
            onClick={() => setTelemetryOpen(!telemetryOpen())}
            aria-label={t("settings.liveCanvas.telemetry.title")}
            title={t("settings.liveCanvas.telemetry.title")}
            aria-expanded={telemetryOpen() ? "true" : "false"}
            aria-controls="live-canvas-telemetry"
          >
            <Activity width={18} height={18} />
          </button>
        </div>
        <Show when={telemetryOpen()}>
          <div id="live-canvas-telemetry" class="live-canvas-telemetry">
            <div>
              <span>{t("settings.liveCanvas.telemetry.words")}</span>
              <strong>{wordCount()}</strong>
            </div>
            <div>
              <span>{t("settings.liveCanvas.telemetry.wpm")}</span>
              <strong>{wordsPerMinute()}</strong>
            </div>
            <div>
              <span>{t("settings.liveCanvas.telemetry.speech")}</span>
              <strong>
                {`${((store.status.current_chunk_speech_ms ?? 0) / 1000).toFixed(1)}s`}
              </strong>
            </div>
            <div>
              <span>{t("settings.liveCanvas.telemetry.elapsed")}</span>
              <strong>
                {`${Math.floor((store.status.elapsed_ms ?? 0) / 1000)}s`}
              </strong>
            </div>
          </div>
        </Show>
      </footer>
    </div>
  );
};
export default LiveCanvas;
