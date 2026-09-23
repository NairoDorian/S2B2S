// The React → Solid 2 migration (see CHANGELOG) made this Solid, and the port
// is shaped by three things Solid does differently from React — each one a
// place where the mechanical translation would have been wrong:
//
// - **The component body runs once.** Every value derived from state is a
//   function (see `hasText`, `quiet`, `wpm`), never a `const`, and every one of
//   them is read *inside a JSX binding* rather than at the top of a helper. A
//   read at a helper's top level would make the calling expression depend on it
//   and rebuild that whole subtree — which for `Waveform` means recreating
//   `OverlayScope`'s canvases and restarting its poll loop on every speech
//   transition.
// - **Effects split tracking from the work.** `createEffect(compute, apply)`
//   discovers its dependencies from what the *compute* reads, so React's
//   dependency array becomes the compute's return value. The apply is then free
//   to touch the DOM and write state; the diagnostics name it "the effect
//   phase", and it is the sanctioned place for a write.
// - **A ref is called once and never nulled.** `capEl` below is a plain
//   variable, and it is deliberately not guarded the way a React ref would be:
//   it is only ever assigned, and the effects that read it ask the DOM what is
//   on screen rather than asking whether it was unmounted.
import { createEffect, createSignal, For, onSettled, Show } from "solid-js";
import type { JSX } from "@solidjs/web";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LogicalPosition } from "@tauri-apps/api/dpi";
import { currentLanguage, useTranslation } from "@/i18n/useTranslationSolid";
import "./RecordingOverlay.css";
import { commands, events } from "@/bindings";
import type {
  StreamPhase,
  StreamPhaseEvent,
  StreamTextEvent,
  StreamWorkKind,
} from "@/bindings";
import { syncLanguageFromSettings } from "@/i18n";
import { getLanguageDirection } from "@/lib/utils/rtl";
import { OverlayScope } from "./OverlayScope";
import {
  OVERLAY_SCOPE_DEFAULTS,
  applyOverlayScopeCss,
  resolveOverlayScope,
  type ResolvedOverlayScope,
} from "@/lib/overlayScope";
import {
  NO_REVEAL,
  advanceReveal,
  finishReveal,
  joinStreamText,
  revealStride,
  splitReveal,
  type StreamReveal,
} from "@/lib/streamReveal";
import type { OverlayScopeSettings } from "@/bindings";

type OverlayState = "recording" | "streaming" | "transcribing" | "processing";

// Ideographs, kana and halfwidth katakana. These scripts are written without
// spaces, so splitting on whitespace would score a whole Japanese sentence as
// one word; counting each character instead matches the characters-per-minute
// convention those languages actually use.
const CJK_CHARS = /[぀-ヿ㐀-䶿一-鿿豈-﫿ｦ-ﾟ]/gu;

const countWords = (text: string): number => {
  const ideographs = text.match(CJK_CHARS)?.length ?? 0;
  const spaced = text.replace(CJK_CHARS, " ").trim();
  return ideographs + (spaced === "" ? 0 : spaced.split(/\s+/).length);
};

// Below this much speech the words-per-minute ratio swings wildly on a single
// word, and the streaming model's own decode lag dominates it. Show a placeholder
// until there is enough signal for the average to mean something.
const WPM_MIN_SPEECH_MS = 2500;
const WPM_MIN_WORDS = 3;

// The experimental Multi-STT streaming mode shows the whole session's text, so
// the card has to grow with it. Its height is reported to the backend in these
// steps — about one call per line of text rather than one per character, and the
// step itself supplies the slack for borders and padding rounding.
const TEXT_HEIGHT_STEP_PX = 24;

const direction = () => getLanguageDirection(currentLanguage());

const fmtTime = (s: number) =>
  `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;

const CancelButton = () => {
  const { t } = useTranslation();
  return (
    <button
      class="sx"
      aria-label={t("common.cancel")}
      onClick={() => commands.cancelOperation()}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <path
          d="M4 4 L12 12 M12 4 L4 12"
          stroke="currentColor"
          stroke-width="1.6"
          stroke-linecap="round"
        />
      </svg>
    </button>
  );
};

const WorkingRow = (props: { label: string; showCancel: boolean }) => (
  <div class="sbase">
    <div class="sbase-l">
      <span class="sspinner" />
    </div>
    <span class="swork-label">{props.label}</span>
    <div class="sbase-r">
      <Show when={props.showCancel}>
        <CancelButton />
      </Show>
    </div>
  </div>
);

const RecordingOverlay = () => {
  const { t } = useTranslation();
  const [isVisible, setIsVisible] = createSignal(false);
  const [state, setState] = createSignal<OverlayState>("recording");
  // `Stream::play()` returning does not mean hardware callbacks are flowing.
  // Stay visually in an arming state until the backend processes the first
  // actual microphone sample chunk.
  const [captureReady, setCaptureReady] = createSignal(false);
  // Poll rate of the miniature analyser: the Live FFT page's update rate,
  // read with the other overlay settings on every show.
  const [scopeRate, setScopeRate] = createSignal(30);
  // The picture settings (`overlay_scope`): views, style, waveform window,
  // size. Read with the other overlay settings on every show; the geometry
  // is published as CSS variables before the card becomes visible.
  const [scopeConfig, setScopeConfig] = createSignal<ResolvedOverlayScope>(
    OVERLAY_SCOPE_DEFAULTS,
  );
  const [streamText, setStreamText] = createSignal<StreamTextEvent>({
    committed: "",
    tentative: "",
  });
  // The experimental Multi Streaming STT mode's other models: the debug view's
  // columns 2, 3 and 4, keyed by stream slot. A record rather than a fixed pair
  // because the mode runs *two or more* — the backend opens one live stream per
  // Multi-STT slot that can stream, so there are as many columns as the user's
  // own list holds. Its own signals, and never a second half of `streamText`:
  // every column updates independently.
  const [extraTexts, setExtraTexts] = createSignal<
    Record<number, StreamTextEvent>
  >({});
  // The debug view's third block, below the columns: the merged-and-cleaned text
  // the session's pauses produce. It arrives on a slot of its own — the one no
  // model can occupy — so the card can tell it apart from a model's column
  // without being told which model is which; see `MERGE_BLOCK_SLOT` and
  // `emit_composed_stream_text` in the backend's `multi_stt_stream`.
  const [mergeText, setMergeText] = createSignal<StreamTextEvent>({
    committed: "",
    tentative: "",
  });
  const [phase, setPhase] = createSignal<StreamPhase>("listening");
  const [workKind, setWorkKind] = createSignal<StreamWorkKind>("transcribing");
  const [elapsed, setElapsed] = createSignal(0);
  // Speech statistics, driven by the backend VAD (see SpeechActivityEvent).
  // `speechMs` counts only time the user was actually talking, so dividing the
  // transcribed word count by it gives a speaking rate rather than a
  // recording-length average.
  const [statsEnabled, setStatsEnabled] = createSignal(false);
  const [speaking, setSpeaking] = createSignal(false);
  const [speechMs, setSpeechMs] = createSignal(0);
  const [wordCount, setWordCount] = createSignal(0);
  // Bumped on each new streaming session so the Live card remounts fresh (replays
  // the pop-in, and never animates in from the previous panel's open size).
  const [session, setSession] = createSignal(0);
  // Overlay placement (top vs bottom of the screen). The Live panel grows downward
  // from a top overlay (oldest line under the pill) and upward from a bottom one.
  const [position, setPosition] = createSignal<"top" | "bottom">("bottom");
  // True once live text overflows the cap. A top overlay fades its top edge only
  // while overflowing, so the resting first line stays crisp flush under the pill.
  const [overflowing, setOverflowing] = createSignal(false);
  // Experimental Multi-STT streaming mode: the text on screen is the whole
  // session's, composed chunk by chunk, so the card grows with it instead of
  // hiding the earlier chunks behind the fade. `textCap` is the backend's answer
  // to a height report — the same number it sized the window with, past which
  // this card scrolls back (see `overlay_stream_text_height`).
  const [wholeSession, setWholeSession] = createSignal(false);
  const [failedChunks, setFailedChunks] = createSignal(0);
  const [textCap, setTextCap] = createSignal<number | null>(null);

  // The mutable boxes React kept in refs. A Solid component body runs once and
  // has no re-render, so these are just variables — and none of them needs a
  // `.current` mirror to be readable from a callback, which is what those
  // `propsRef`/`stateRef` boxes in the React tree existed for. They are not
  // reactive on purpose: nothing renders from them, they only carry state
  // between a timer tick and the next read.
  let reportedHeight = -1;
  let capEl: HTMLDivElement | undefined;
  let pinned = true;
  let directMode = false;
  let directSpeed = 30;
  // Whether the typewriter may rewrite text it has already revealed. Off by
  // default, and read per session in the `show-overlay` handler beside
  // `directMode`. See `overlay_back_correction` in settings.rs: a model that
  // re-attends over its whole audio context (R2T2) replaces its volatile tail on
  // every decode, so with this off the preview simply shows each revision as a
  // block instead of rewinding the revealed characters and retyping them.
  let backCorrection = false;
  let targetText: StreamTextEvent = { committed: "", tentative: "" };
  // The reveal grows one string, not two, and is re-derived rather than
  // accumulated — see `streamReveal.ts`, which carries the reasoning and the
  // tests.
  let reveal: StreamReveal = NO_REVEAL;
  // The split the signal is currently holding, so that re-deriving an unchanged
  // one does not re-signal it.
  let published: StreamTextEvent = { committed: "", tentative: "" };
  // The extra columns' watermarks, one per slot, for the same reason column 1
  // has its own: the streams arrive interleaved with no ordering between them, so
  // one watermark shared between them would swallow whichever column another had
  // just moved.
  const publishedExtras = new Map<number, StreamTextEvent>();
  // The merged block's watermark, on the same terms.
  let publishedMerge: StreamTextEvent = { committed: "", tentative: "" };
  let typewriterTimer: ReturnType<typeof setInterval> | null = null;

  // Drag-grip state (see AIVORelay's recording-overlay position memory pattern).
  // These are plain variables — not reactive — because nothing renders from them
  // directly; they only carry state between pointer events and the onMoved listener.
  const windowRef = getCurrentWindow();
  let dragGripArmed = false;
  let dragGripSawMove = false;
  let dragGripLastPosition: { x: number; y: number } | null = null;
  let dragGripSaveTimer: ReturnType<typeof setTimeout> | null = null;
  // Manual-drag fallback state — used when startDragging() fails (Windows
  // non-focusable overlay window: WS_EX_NOACTIVATE blocks the native drag).
  let manualDragActive = false;
  let manualDragStart: {
    clientX: number;
    clientY: number;
    winX: number;
    winY: number;
  } = {
    clientX: 0,
    clientY: 0,
    winX: 0,
    winY: 0,
  };
  let manualDragMoveHandler: ((e: PointerEvent) => void) | null = null;
  let manualDragUpHandler: (() => void) | null = null;
  let dragGripFallbackTimer: ReturnType<typeof setTimeout> | null = null;

  // Publish through here so an unchanged split is not re-signalled: a Solid
  // signal fires on every `set` with a fresh object, and the typewriter ticks
  // far faster than the text changes.
  const applyStreamText = (next: StreamTextEvent) => {
    if (
      next.committed === published.committed &&
      next.tentative === published.tentative
    ) {
      return;
    }
    published = next;
    setStreamText(next);
  };

  // Each extra column leans on the same rule as column 1, against its own
  // watermark. The record is replaced rather than mutated so the signal — not the
  // object inside it — is what the columns subscribe to.
  const applyExtraText = (slot: number, next: StreamTextEvent) => {
    const previous = publishedExtras.get(slot);
    if (
      previous !== undefined &&
      next.committed === previous.committed &&
      next.tentative === previous.tentative
    ) {
      return;
    }
    publishedExtras.set(slot, next);
    setExtraTexts((current) => ({ ...current, [slot]: next }));
  };

  // The merged block, on the same terms as the columns above.
  const applyMergeText = (next: StreamTextEvent) => {
    if (
      next.committed === publishedMerge.committed &&
      next.tentative === publishedMerge.tentative
    ) {
      return;
    }
    publishedMerge = next;
    setMergeText(next);
  };

  // A new session starts with no columns but the first, and no merged block.
  const resetStreamTexts = () => {
    publishedExtras.clear();
    setExtraTexts({});
    applyMergeText({ committed: "", tentative: "" });
  };

  // Show the reveal as the target splits it. The seam comes from the target, so
  // it advances with the model's own boundary instead of being typed.
  const publishReveal = () => {
    applyStreamText(splitReveal(reveal, targetText.committed.length));
  };

  const stopTypewriter = () => {
    if (typewriterTimer !== null) {
      clearInterval(typewriterTimer);
      typewriterTimer = null;
    }
  };

  const flushTypewriter = () => {
    stopTypewriter();
    reveal = finishReveal(
      joinStreamText(targetText.committed, targetText.tentative),
    );
    publishReveal();
  };

  const stepTypewriter = () => {
    const full = joinStreamText(targetText.committed, targetText.tentative);
    // Caught up is the common resting state: publish anyway (a commit can move
    // the seam without changing a byte of the text) and stop ticking.
    if (reveal.text.slice(0, reveal.revealed) === full) {
      publishReveal();
      stopTypewriter();
      return;
    }
    reveal = advanceReveal(
      reveal,
      full,
      revealStride(full.length - reveal.revealed, directSpeed),
      backCorrection,
    );
    publishReveal();
  };

  const startTypewriterIfNeeded = () => {
    if (typewriterTimer === null) {
      const speed = directSpeed || 30;
      const intervalMs = Math.max(8, Math.min(100, Math.round(1000 / speed)));
      typewriterTimer = setInterval(stepTypewriter, intervalMs);
    }
  };

  // `onSettled` is Solid's `onMount`, and it is where this belongs rather than in
  // an effect: it registers listeners once and returns their teardown. Every
  // write below happens either in a listener callback or in a promise
  // continuation — both outside any owner — so none of them trips the
  // owned-scope write guard the way a write in this setup body would.
  onSettled(() => {
    const setupEventListeners = async () => {
      const unlistenShow = await listen("show-overlay", async (event) => {
        const overlayState = event.payload as OverlayState;
        // Reset synchronously before settings I/O. A fast microphone can emit
        // recording-ready while the awaits below are in flight; resetting after
        // them would overwrite that event and leave the overlay stuck arming.
        if (overlayState === "recording" || overlayState === "streaming") {
          setCaptureReady(false);
          stopTypewriter();
          targetText = { committed: "", tentative: "" };
          reveal = NO_REVEAL;
          applyStreamText({ committed: "", tentative: "" });
          resetStreamTexts();
          setSpeaking(false);
          setSpeechMs(0);
          setWordCount(0);
          // A previous session's composed text must not size this card: the
          // mode announces itself with its first event, and the height with it.
          setWholeSession(false);
          setFailedChunks(0);
          setTextCap(null);
          reportedHeight = -1;
        }

        await syncLanguageFromSettings();
        // The Live panel flows downward from a top overlay and upward from a
        // bottom one; read the placement so the layout can flip to match.
        try {
          const settings = await commands.getAppSettings();
          if (settings.status === "ok") {
            setPosition(
              settings.data.overlay_position === "top" ? "top" : "bottom",
            );
            directMode = settings.data.overlay_direct_mode ?? false;
            directSpeed = settings.data.overlay_direct_speed ?? 30;
            backCorrection = settings.data.overlay_back_correction ?? false;
            setStatsEnabled(settings.data.overlay_speech_stats ?? true);
            setScopeRate(settings.data.live_fft?.update_rate_hz ?? 30);
            const scope = resolveOverlayScope(settings.data.overlay_scope);
            applyOverlayScopeCss(scope);
            setScopeConfig(scope);
          }
        } catch {
          // Keep the previous/default placement if settings can't be read.
        }
        setState(overlayState);
        if (overlayState === "streaming") {
          setPhase("listening");
          setWorkKind("transcribing");
          setElapsed(0);
          setSession((s) => s + 1); // remount the card fresh for this session
        }
        setIsVisible(true);
      });

      const unlistenHide = await listen("hide-overlay", () => {
        setIsVisible(false);
        setCaptureReady(false);
        stopTypewriter();
      });

      const unlistenReady = await listen("recording-ready", () => {
        setElapsed(0);
        setCaptureReady(true);
      });

      const unlistenStream = await events.streamTextEvent.listen((event) => {
        // A numbered slot is one of the mode's other streams, and it is handled
        // here and nowhere else: it is applied whole (another model's text was
        // never typed, so there is no reveal to keep) and then this handler
        // returns, so nothing below — the typewriter's target, the word count,
        // the session flags — is ever driven by another model's stream.
        //
        // Every composed event rides a numbered slot as well, and it is the only
        // thing that sets `whole_session`: the merged block is the one numbered
        // slot that is *not* a model's column, and the flag is what tells them
        // apart. (The production view's single block is composed too, but it
        // arrives unnumbered, on the primary's own path below.)
        if (typeof event.payload.slot === "number") {
          if (event.payload.whole_session) {
            // The session's own text, so the card grows with it and the failed
            // chunks are the session's, exactly as on the production path.
            setWholeSession(true);
            setFailedChunks(event.payload.failed_chunks ?? 0);
            applyMergeText(event.payload);
          } else {
            applyExtraText(event.payload.slot, event.payload);
          }
          return;
        }
        targetText = event.payload;
        // Count from the backend text, not the typewriter's partial reveal, so
        // direct mode cannot make the speaking rate read artificially low. This
        // runs even in the minimal overlay, which never renders the text but
        // still receives it whenever the model streams.
        // The two halves are one text and the model supplies the separators
        // (see `splitReveal`), so join them verbatim: a space here splits a
        // word in Chinese and counts a stray token in English.
        setWordCount(
          countWords(
            joinStreamText(
              event.payload.committed,
              event.payload.tentative,
            ).trim(),
          ),
        );
        // The experimental Multi-STT streaming mode composes the whole session's
        // text and flags its failed chunks (the failure is never in the text
        // itself — with DirectStreaming that text lands in the user's document).
        if (event.payload.whole_session) {
          setWholeSession(true);
          setFailedChunks(event.payload.failed_chunks ?? 0);
        }
        // The experimental Multi-STT streaming mode composes the whole session's
        // text itself, and a merge replaces whole chunks that are already on
        // screen. There is nothing to reveal, so the update is applied as one
        // block: the typewriter could only retype its way back to every
        // correction, one to three characters per tick. It stays the rule for
        // normal dictation, which is what the setting is for.
        if (!directMode || event.payload.whole_session) {
          stopTypewriter();
          // The backend composed this text, so show its own split as it is;
          // the reveal is simply complete.
          reveal = finishReveal(
            joinStreamText(event.payload.committed, event.payload.tentative),
          );
          applyStreamText(event.payload);
        } else {
          startTypewriterIfNeeded();
        }
      });

      const unlistenSpeech = await events.speechActivityEvent.listen(
        (event) => {
          setSpeaking(event.payload.speaking);
          setSpeechMs(event.payload.speech_ms);
        },
      );

      const unlistenPhase = await events.streamPhaseEvent.listen((event) => {
        const payload: StreamPhaseEvent = event.payload;
        setPhase(payload.phase);
        if (payload.kind) setWorkKind(payload.kind);
        if (payload.phase === "working") {
          flushTypewriter();
        }
      });

      // Track overlay moves so the drag-grip can save the final position once
      // the user lets go. A debounced save avoids flooding the backend with
      // updates while the window is still in motion.
      const unlistenMoved = await windowRef.onMoved(({ payload }) => {
        if (!dragGripArmed) return;
        // Native drag is producing movement — cancel the manual-drag fallback
        // timer so we don't double up with setPosition.
        if (dragGripFallbackTimer !== null) {
          clearTimeout(dragGripFallbackTimer);
          dragGripFallbackTimer = null;
        }
        dragGripSawMove = true;
        dragGripLastPosition = { x: payload.x, y: payload.y };
        if (dragGripSaveTimer !== null) {
          clearTimeout(dragGripSaveTimer);
        }
        dragGripSaveTimer = setTimeout(() => {
          const positionToSave = dragGripLastPosition;
          dragGripSaveTimer = null;
          if (!positionToSave || !dragGripArmed) return;
          saveOverlayPosition(
            Math.round(positionToSave.x),
            Math.round(positionToSave.y),
          );
        }, 500);
      });

      // Live corner-radius updates — the setting's backend handler emits this
      // on every change, so a running overlay preview re-rounds its card
      // without a stop/start cycle.
      const unlistenRadius = await listen(
        "overlay-corner-radius",
        (event: { payload: number }) => {
          document.documentElement.style.setProperty(
            "--ov-corner-radius",
            `${event.payload}px`,
          );
        },
      );

      // Live scope-picture updates — circular/linear spectrum toggles, size,
      // style, mirror, waveform window. The backend already re-purposes the FFT
      // manager; this refreshes the card's CSS geometry and the `<OverlayScope>`
      // config so the preview re-renders in place.
      const unlistenScope = await listen(
        "overlay-scope-changed",
        (event: { payload: OverlayScopeSettings }) => {
          const resolved = resolveOverlayScope(event.payload);
          applyOverlayScopeCss(resolved);
          setScopeConfig(resolved);
        },
      );

      // Live speech-stats toggle — show/hide the readout row in place.
      const unlistenStats = await listen(
        "overlay-speech-stats",
        (event: { payload: boolean }) => {
          setStatsEnabled(event.payload);
        },
      );

      // Live direct-mode / direct-speed — re-arm the typewriter behavior.
      const unlistenDirectMode = await listen(
        "overlay-direct-mode",
        (event: { payload: boolean }) => {
          directMode = event.payload;
        },
      );
      const unlistenDirectSpeed = await listen(
        "overlay-direct-speed",
        (event: { payload: number }) => {
          directSpeed = event.payload;
        },
      );
      // Live back-correction toggle. Unlike the two above it does not disturb
      // the reveal already on screen — it only decides what the *next* revision
      // does, so a change mid-recording takes effect on the next update rather
      // than snapping the displayed text to its target.
      const unlistenBackCorrection = await listen(
        "overlay-back-correction",
        (event: { payload: boolean }) => {
          backCorrection = event.payload;
        },
      );

      // Live overlay-position change — the backend has already cleared any
      // drag-grip position and re-anchored the window; flip the panel's growth
      // direction to match.
      const unlistenPos = await listen(
        "overlay-position-changed",
        (event: { payload: string }) => {
          setPosition(event.payload === "top" ? "top" : "bottom");
        },
      );

      return () => {
        stopTypewriter();
        unlistenShow();
        unlistenHide();
        unlistenReady();
        unlistenStream();
        unlistenSpeech();
        unlistenPhase();
        unlistenMoved();
        unlistenRadius();
        unlistenScope();
        unlistenStats();
        unlistenDirectMode();
        unlistenDirectSpeed();
        unlistenBackCorrection();
        unlistenPos();
      };
    };

    // The unlisten handles only exist once the awaits above resolve, so the
    // teardown cannot be the handle set itself: a component disposed during that
    // gap has to unlisten the moment the handles arrive, or the listeners
    // outlive it. React needed this guard for StrictMode's double-invoke too;
    // StrictMode is gone, the gap is not.
    let disposed = false;
    let cleanup: (() => void) | undefined;
    setupEventListeners().then((fn) => {
      if (disposed) {
        fn();
      } else {
        cleanup = fn;
      }
    });

    // Prime the stats setting and corner-radius CSS variable before the first
    // show, so the card opens at its final width instead of visibly growing
    // once the settings read lands.
    commands
      .getAppSettings()
      .then((settings) => {
        if (settings.status === "ok") {
          setStatsEnabled(settings.data.overlay_speech_stats ?? true);
          const radius = settings.data.overlay_window_corner_radius ?? 0;
          document.documentElement.style.setProperty(
            "--ov-corner-radius",
            `${radius}px`,
          );
        }
      })
      .catch(() => {
        // Keep the primed default (on) until the first show reads settings again.
      });

    return () => {
      disposed = true;
      cleanup?.();
    };
  });

  // Elapsed capture timer starts only once microphone samples are flowing.
  // The compute reads exactly the three values React listed in its dependency
  // array, which is how an effect learns what to re-run on; the apply returns its
  // own teardown, so re-running replaces the interval rather than stacking one.
  createEffect(
    () => [state(), isVisible(), captureReady()] as const,
    ([current, visible, ready]) => {
      if (current !== "streaming" || !visible || !ready) return;
      const id = setInterval(() => {
        setElapsed((e) => e + 1);
      }, 1000);
      return () => clearInterval(id);
    },
  );

  // Stick to the bottom as text streams in — but only while pinned, so a user who
  // has scrolled up to read history isn't yanked back down by the next chunk.
  // An effect rather than a render effect: Solid applies effects after the DOM
  // updates of the same flush, and that flush is a microtask, so this still runs
  // before the browser paints. The apply writes `overflowing` — the effect phase
  // is the sanctioned place for that.
  createEffect(
    () => [streamText(), extraTexts(), mergeText()] as const,
    () => {
      const el = capEl;
      if (!el) return;
      // Fade the top edge only once text actually overflows the cap.
      setOverflowing(el.scrollHeight > el.clientHeight + 1);
      if (pinned) el.scrollTop = el.scrollHeight;
    },
  );

  // Grow the card with the session's text (experimental Multi-STT streaming mode)
  // and read back the height it may reach before scrolling. Measured after layout
  // so the report matches what is on screen, and rounded up to a step so this is
  // one call per line of text; the backend clamps to its own monitor-based cap and
  // returns that clamp, which is what bounds this card too.
  // Both columns are inside the one `capEl`, so either one growing is the same
  // measurement reported by the same call — each extra column needs a trigger
  // here, not a second measurement of its own. The merged block is inside it too,
  // and it grows a whole block at a time.
  createEffect(
    () => [streamText(), extraTexts(), mergeText(), wholeSession()] as const,
    ([, , whole]) => {
      if (!whole) return;
      const el = capEl;
      if (!el) return;
      const stepped =
        Math.ceil(el.scrollHeight / TEXT_HEIGHT_STEP_PX) * TEXT_HEIGHT_STEP_PX;
      if (stepped === reportedHeight) return;
      reportedHeight = stepped;
      commands
        .overlayStreamTextHeight(stepped)
        .then((max) => {
          // 0 means the backend has no streaming card on screen (the overlay is
          // fading out): keep the cap we already have rather than collapsing.
          if (typeof max === "number" && max > 0) setTextCap(max);
        })
        .catch(() => {
          // No cap from the backend: the card keeps the default and scrolls.
        });
    },
  );

  // Each fresh streaming session starts pinned to the bottom, fade cleared.
  createEffect(
    () => session(),
    () => {
      pinned = true;
      setOverflowing(false);
    },
  );

  // Re-pin when the user is within ~a line of the bottom; unpin otherwise.
  const handleStreamScroll = () => {
    const el = capEl;
    if (!el) return;
    pinned = el.scrollHeight - el.scrollTop - el.clientHeight <= 16;
  };

  // Drag-grip handlers — allow the user to grab the overlay and reposition it,
  // persisting the new position so it stays there across recordings (learned
  // from AIVORelay's recording-overlay position memory).
  const saveOverlayPosition = (xPx: number, yPx: number) => {
    if (dragGripSaveTimer !== null) {
      clearTimeout(dragGripSaveTimer);
      dragGripSaveTimer = null;
    }
    commands.rememberRecordingOverlayWindowPosition(xPx, yPx).catch((error) => {
      console.error("Failed to remember recording overlay position:", error);
    });
  };

  const handleDragGripPointerDown = (event: PointerEvent) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();

    dragGripArmed = true;
    dragGripSawMove = false;
    dragGripLastPosition = null;
    manualDragActive = false;
    if (dragGripSaveTimer !== null) {
      clearTimeout(dragGripSaveTimer);
      dragGripSaveTimer = null;
    }
    if (dragGripFallbackTimer !== null) {
      clearTimeout(dragGripFallbackTimer);
      dragGripFallbackTimer = null;
    }
    // Remove any leftover manual-drag listeners from a previous session.
    if (manualDragMoveHandler) {
      window.removeEventListener("pointermove", manualDragMoveHandler);
      manualDragMoveHandler = null;
    }
    if (manualDragUpHandler) {
      window.removeEventListener("pointerup", manualDragUpHandler);
      manualDragUpHandler = null;
    }

    // Try native drag first. On some platforms (Windows with focusable(false)
    // → WS_EX_NOACTIVATE) startDragging() either rejects or resolves without
    // moving the window. A 50ms fallback timer detects the latter: if no
    // onMoved event has fired by then, switch to manual dragging.
    void windowRef.startDragging().then(
      () => {
        if (!dragGripArmed) return;
        dragGripFallbackTimer = setTimeout(() => {
          if (!dragGripSawMove && dragGripArmed) {
            // Native drag didn't produce movement — go manual.
            startManualDrag(event);
          }
        }, 50);
      },
      () => {
        // Native drag rejected — go manual immediately.
        startManualDrag(event);
      },
    );
  };

  /**
   * Manual drag fallback: tracks pointermove on the global window and moves the
   * overlay via setPosition() instead of the native drag loop. This works even
   * when startDragging() can't activate the window (WS_EX_NOACTIVATE on Windows).
   */
  const startManualDrag = (event: PointerEvent) => {
    // `outerPosition()` is physical pixels while the pointer deltas below are
    // CSS (logical) pixels, so the start is converted once here and every move
    // is a `LogicalPosition`; mixing the two jumped the window at any display
    // scale other than 100 %.
    void Promise.all([windowRef.outerPosition(), windowRef.scaleFactor()]).then(
      ([pos, scale]) => {
        if (!dragGripArmed) return;
        manualDragActive = true;
        manualDragStart = {
          clientX: event.clientX,
          clientY: event.clientY,
          winX: pos.x / scale,
          winY: pos.y / scale,
        };
      },
    );

    manualDragMoveHandler = (e: PointerEvent) => {
      if (!manualDragActive) return;
      const dx = e.clientX - manualDragStart.clientX;
      const dy = e.clientY - manualDragStart.clientY;
      void windowRef.setPosition(
        new LogicalPosition(
          manualDragStart.winX + dx,
          manualDragStart.winY + dy,
        ),
      );
      dragGripSawMove = true;
    };

    manualDragUpHandler = () => {
      const wasMoved = dragGripSawMove;
      manualDragActive = false;
      if (manualDragMoveHandler) {
        window.removeEventListener("pointermove", manualDragMoveHandler);
        manualDragMoveHandler = null;
      }
      if (manualDragUpHandler) {
        window.removeEventListener("pointerup", manualDragUpHandler);
        manualDragUpHandler = null;
      }
      if (dragGripFallbackTimer !== null) {
        clearTimeout(dragGripFallbackTimer);
        dragGripFallbackTimer = null;
      }
      // For manual drag the onMoved event may not have fired for the final
      // setPosition() call, so save the position directly. `outerPosition()`
      // is already physical pixels — what the backend stores, and what the
      // native path saves from `onMoved` — so it is saved unscaled.
      if (wasMoved) {
        void windowRef.outerPosition().then((pos) => {
          saveOverlayPosition(Math.round(pos.x), Math.round(pos.y));
        });
      }
      dragGripArmed = false;
      dragGripSawMove = false;
      dragGripLastPosition = null;
    };

    window.addEventListener("pointermove", manualDragMoveHandler);
    window.addEventListener("pointerup", manualDragUpHandler);
  };

  const handleDragGripPointerUp = () => {
    // Manual drag cleanup is handled by the window-level pointerup listener
    // (manualDragUpHandler), which saves the position directly. Returning
    // early here prevents the native-drag save path from clobbering it.
    if (manualDragActive || manualDragUpHandler !== null) {
      return;
    }

    const positionToSave = dragGripSawMove ? dragGripLastPosition : null;
    dragGripArmed = false;
    dragGripSawMove = false;
    dragGripLastPosition = null;

    if (positionToSave) {
      saveOverlayPosition(
        Math.round(positionToSave.x),
        Math.round(positionToSave.y),
      );
    }
  };

  // Speech stats ride along in both overlay forms. Rendered from the moment the
  // card appears rather than waiting for the first sample, so the Live card
  // steps its width once (pill -> panel) instead of twice.
  const showStats = () => statsEnabled();
  // "Nobody is talking right now." The arming case (microphone not started yet)
  // is handled separately below and takes visual precedence.
  const quiet = () => showStats() && !speaking();
  const wpm = () =>
    speechMs() >= WPM_MIN_SPEECH_MS && wordCount() >= WPM_MIN_WORDS
      ? Math.round(wordCount() / (speechMs() / 60000))
      : null;

  const hasText = () =>
    streamText().committed.length > 0 ||
    streamText().tentative.length > 0 ||
    hasExtraTexts() ||
    hasMergeText();
  // The mode's other models, in slot order, as `[slot, text]` pairs. Sorted
  // rather than taken as the record's own order: the slots are numbers and an
  // object's keys would otherwise read 1, 10, 2 — and a column that swapped
  // places mid-recording would be the one thing this debug view must not do.
  //
  // A slot is in here from the moment its stream goes live, which the backend
  // announces with an empty-text event, so the record answers "which models are
  // running" and not only "which have spoken".
  const extraColumns = () =>
    Object.entries(extraTexts())
      .map(([slot, text]) => [Number(slot), text] as const)
      .toSorted(([a], [b]) => a - b);
  // Whether any of them has actually said anything. Kept apart from
  // `extraColumns()` on purpose: this one decides whether the card has text to
  // open for, and a running-but-silent model must not open it on its own.
  const hasExtraTexts = () =>
    extraColumns().some(
      ([, text]) => text.committed.length > 0 || text.tentative.length > 0,
    );
  // The merged-and-cleaned block, once a pause has produced one.
  const hasMergeText = () =>
    mergeText().committed.length > 0 || mergeText().tentative.length > 0;
  // The detailed view: the models' own live texts side by side, with the merged
  // result below them. In the production view neither ever arrives — the session
  // suppresses the extra streams' raw events and composes its single block onto
  // the primary's path — so this is exactly "which of the mode's two views is on
  // screen", answered by what the backend actually sent rather than by a setting
  // the overlay would have to read and keep in step. A numbered slot is the
  // whole test, because it is the one thing only the detailed view receives;
  // an announced column counts before it has text, which is what keeps the
  // view's first seconds from looking like a model that never started.
  const debugStream = () => extraColumns().length > 0 || hasMergeText();

  const working = () => phase() === "working";
  // Keep the panel open whenever there's text — even while finalizing — so the
  // transcript stays put under a working spinner instead of collapsing and
  // squishing the text mid-stream. Only fall back to the small working pill
  // when there was no text to preserve.
  const open = () => hasText();
  const collapsed = () => working() && !hasText();

  // A chunk whose merge failed shows the extras' outputs concatenated — still
  // the session's text, just not the cleaned version. It is marked here rather
  // than in the text: with DirectStreaming the text is typed into the user's
  // document, and a marker inside it would be typed too.
  const failBadge = () =>
    failedChunks() > 0 ? (
      <span class="sfail" title={t("overlay.chunkFailedHint")}>
        {t("overlay.chunkFailed", { count: failedChunks() })}
      </span>
    ) : null;

  // React keyed the card on `session` so a new streaming session remounted it,
  // replaying the pop-in. The Solid counterpart is a keyed `<Show>`, with one
  // wrinkle: `session` starts at 0, which `Show` reads as absent, so the key is
  // wrapped in a fresh object whose identity changes exactly when `session` does.
  const sessionKey = () => ({ session: session() });

  // ---- Shared building blocks (one visual language for every overlay form) ----
  // These are components rather than element constants for a reason that is not
  // stylistic: React's `const cancelBtn = <button …/>` is a *description*, but a
  // Solid JSX expression is already a DOM node. Reusing one node in two places
  // would move it, not copy it. As components they are built where they are used,
  // and their props are read lazily, so a read inside one of them subscribes only
  // the binding that needs it.

  // Speech-only clock and running words-per-minute. The clock freezes on
  // silence, which is the whole point — it measures talking, not recording.
  const StatsCluster = () => (
    <span class={["sstats", { quiet: quiet() }]}>
      <span class="sstat-speech">{fmtTime(Math.floor(speechMs() / 1000))}</span>
      <span class="sstat-sep" aria-hidden="true" />
      <span class="sstat-wpm">
        {wpm() === null ? (
          <span class="sstat-pending">{"—"}</span>
        ) : (
          `${wpm()} ${t("overlay.wpm")}`
        )}
      </span>
    </span>
  );

  // The numeric readouts, in reading order: total elapsed, then the speech
  // clock and rate.
  const Readouts = (props: { showTimer: boolean }) => (
    <>
      <Show when={props.showTimer}>
        <span class="stimer">{fmtTime(elapsed())}</span>
      </Show>
      <Show when={showStats()}>
        <StatsCluster />
      </Show>
    </>
  );

  // The miniature analyser: an area spectrum and the last 4096 samples as a
  // line, both computed with the Live FFT page's settings (OverlayScope), plus
  // the circular view when it sits in the block rather than behind the card.
  // The condition matches the views `overlayScopeBlockWidth` reserves room for,
  // so a circular-only picture is drawn instead of leaving an empty gap.
  const Waveform = () => (
    <Show
      when={
        scopeConfig().show_spectrum ||
        scopeConfig().show_wave ||
        (scopeConfig().show_circular && !scopeConfig().circular_background)
      }
    >
      <div
        class={[
          "swave",
          captureReady() ? "ready" : "arming",
          { quiet: quiet() },
        ]}
      >
        <OverlayScope
          rateHz={scopeRate()}
          ready={captureReady()}
          quiet={quiet()}
          config={scopeConfig()}
        />
      </div>
    </Show>
  );

  // Without stats: dot (left) | waveform (center) | timer + cancel (right),
  // the original three-zone grid that keeps the waveform centered.
  //
  // With stats: dot, waveform and readouts run together as one evenly spaced
  // group, and only the cancel button is pinned to the right edge. Keeping the
  // readouts in the right-hand cluster instead would put every pixel of slack
  // between the waveform and the numbers — separating things that belong
  // together, by a gap that grows with the card.
  const ListeningRow = (props: {
    showTimer: boolean;
    showCancel: boolean;
    badge?: JSX.Element;
  }) => (
    <div class={["sbase", { "has-stats": showStats() }]}>
      <div class="sbase-l">
        <span
          class={[
            "sdot",
            !captureReady() ? "arming" : quiet() ? "silent" : "ready",
          ]}
        />
      </div>
      <Waveform />
      <Show when={showStats()}>
        <div class="smeta">
          <Readouts showTimer={props.showTimer} />
        </div>
      </Show>
      <div class="sbase-r">
        <Show when={!showStats()}>
          <Readouts showTimer={props.showTimer} />
        </Show>
        {props.badge}
        <Show when={props.showCancel}>
          <CancelButton />
        </Show>
      </div>
    </div>
  );

  const workLabel = () =>
    state() === "processing"
      ? t("overlay.processing")
      : t("overlay.transcribing");

  // ---- Live overlay: a pill that sculpts open into a panel ----
  const LiveCard = () => (
    <div
      dir={direction()}
      class={["ov-stage", position()]}
      style={
        textCap() === null ? undefined : { "--ov-cap-max-h": `${textCap()}px` }
      }
    >
      <Show when={sessionKey()} keyed>
        {/* `leaving` mirrors React's class list; the card only exists while the
            overlay is visible, so it is always false in practice. */}
        <div
          class={[
            "scard",
            {
              open: open(),
              working: collapsed(),
              "has-stats": showStats() && !open() && !collapsed(),
              leaving: !isVisible(),
            },
          ]}
        >
          <div class="stext">
            <div class="stext-clip">
              <div
                class={["stext-cap", { overflowing: overflowing() }]}
                ref={(el: HTMLDivElement) => {
                  capEl = el;
                }}
                onScroll={handleStreamScroll}
              >
                <div class="stext-cols">
                  <div class="stext-col">
                    <Show when={debugStream()}>
                      {/* The model's own list position, so the columns below can
                          be read against the Multi-STT settings' own numbering
                          (slot n is model n + 1). */}
                      <span class="smark">{"1"}</span>
                    </Show>
                    <p>
                      {/* Nothing between the two spans: they are one text cut in
                          two and the model carries its own spacing (see
                          `splitReveal`). A separator here lands inside a word —
                          which for R2T2's Chinese is most of them. */}
                      <span class="committed">{streamText().committed}</span>
                      <span class="tentative">{streamText().tentative}</span>
                      {/* Drop the blinking caret once finalizing — it's no longer
                          capturing, and a static spinner conveys the work. */}
                      <Show when={!working()}>
                        <span class="scaret" />
                      </Show>
                    </p>
                  </div>
                  {/* Every other model's column, present for as long as that
                      model is streaming — including before it has said
                      anything, and including when it never does. An empty
                      column is a finding, not an absence: it says the model is
                      running and producing nothing, which is most often a
                      setting that does not match the speech (a wrong language
                      hint on a prompt-conditioned model). No caret either: the
                      caret tracks the reveal, and these columns have none. */}
                  <For each={extraColumns()}>
                    {([slot, text]) => (
                      <div class="stext-col">
                        <span class="smark">{`${slot + 1}`}</span>
                        <Show
                          when={
                            text.committed.length > 0 ||
                            text.tentative.length > 0
                          }
                          fallback={
                            <p>
                              <span class="swait">
                                {t("overlay.awaitingText")}
                              </span>
                            </p>
                          }
                        >
                          {/* The same rule as column 1, and it matters more
                              here: these spans are one text cut in two as
                              well. */}
                          <p>
                            <span class="committed">{text.committed}</span>
                            <span class="tentative">{text.tentative}</span>
                          </p>
                        </Show>
                      </div>
                    )}
                  </For>
                </div>
                {/* The merged-and-cleaned result of the session's last pause,
                    under the models it was merged from — the third block of the
                    detailed view, and the only one that is not a model's own
                    text. The badge below reports the whole session either way. */}
                <Show when={debugStream()}>
                  <div class="smerged">
                    {/* Spelled out, unlike the numerals above it: this block is
                        not a model, and a letter read against them ("M" beside
                        "1" and "2") reads as one more model whose name starts
                        with M. The label is the settings' own name for what the
                        mode does at a pause, so what it is needs no legend. */}
                    <span class="smark smerge-mark">
                      {t("overlay.mergeAndCleaned")}
                    </span>
                    <p>
                      <span class="committed">{mergeText().committed}</span>
                      <span class="tentative">{mergeText().tentative}</span>
                    </p>
                  </div>
                </Show>
              </div>
            </div>
          </div>
          <Show
            when={working()}
            fallback={
              <ListeningRow
                showTimer={open()}
                showCancel={true}
                badge={failBadge()}
              />
            }
          >
            <WorkingRow
              label={
                workKind() === "polishing"
                  ? t("overlay.processing")
                  : t("overlay.transcribing")
              }
              showCancel={true}
            />
          </Show>
        </div>
      </Show>
      <DragGrip />
    </div>
  );

  // ---- Minimal overlay: exactly one row at a time — waveform (recording), or a
  // spinner + label (transcribing / processing). Never both. The pill animates its
  // width between them; the cancel button is in both rows so it stays put.
  const MinimalCard = () => (
    <div
      dir={direction()}
      class={["ov-stage", position(), "ov-fade", { show: isVisible() }]}
    >
      <div
        class={[
          "scard",
          "compact",
          {
            cworking:
              state() !== "streaming" && state() !== "recording" && isVisible(),
            "has-stats":
              showStats() &&
              (state() === "streaming" || state() === "recording"),
          },
        ]}
      >
        <Show
          when={state() === "transcribing" || state() === "processing"}
          fallback={<ListeningRow showTimer={false} showCancel={true} />}
        >
          <WorkingRow label={workLabel()} showCancel={true} />
        </Show>
      </div>
      <DragGrip />
    </div>
  );

  // Drag grip — a tiny grab handle at the bottom of the overlay that lets the
  // user reposition it. Clicking and dragging calls Tauri's startDragging()
  // (native window move); releasing saves the physical position so it sticks.
  // Always present so the feature is discoverable, learned from AIVORelay's
  // recording-overlay position memory.
  const DragGrip = () => (
    <button
      class="ov-drag-grip"
      aria-label={t("overlay.dragGripTooltip")}
      title={t("overlay.dragGripTooltip")}
      onPointerDown={handleDragGripPointerDown}
      onPointerUp={handleDragGripPointerUp}
      onPointerCancel={handleDragGripPointerUp}
    >
      <span class="ov-drag-grip-dot" />
      <span class="ov-drag-grip-dot" />
      <span class="ov-drag-grip-dot" />
    </button>
  );

  // `show` on the stage and `leaving` on the card are both constant while the
  // branch is mounted (React returned `null` rather than rendering an invisible
  // card). They are kept as expressions mirroring React's class list, which is
  // why they read as `isVisible()` and `!isVisible()` inside this `Show`.
  return (
    <Show when={isVisible()}>
      <Show
        when={scopeConfig().show_circular && scopeConfig().circular_background}
      >
        {/* The circular spectrum as a full-window background layer behind the
            card. Its own OverlayScope instance: it polls the same frame
            command, but draws only the circular view, across the window. */}
        <OverlayScope
          rateHz={scopeRate()}
          ready={captureReady()}
          quiet={quiet()}
          config={scopeConfig()}
          background
        />
      </Show>
      <Show when={state() === "streaming"} fallback={<MinimalCard />}>
        <LiveCard />
      </Show>
    </Show>
  );
};

export default RecordingOverlay;
