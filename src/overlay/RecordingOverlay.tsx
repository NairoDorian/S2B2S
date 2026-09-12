import { listen } from "@tauri-apps/api/event";
import React, { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import "./RecordingOverlay.css";
import { commands, events } from "@/bindings";
import type {
  StreamPhase,
  StreamPhaseEvent,
  StreamTextEvent,
  StreamWorkKind,
} from "@/bindings";
import i18n, { syncLanguageFromSettings } from "@/i18n";
import { getLanguageDirection } from "@/lib/utils/rtl";
import { OverlayScope } from "./OverlayScope";
import {
  OVERLAY_SCOPE_DEFAULTS,
  applyOverlayScopeCss,
  resolveOverlayScope,
  type ResolvedOverlayScope,
} from "@/lib/overlayScope";

type OverlayState = "recording" | "streaming" | "transcribing" | "processing";

// Ideographs, kana and halfwidth katakana. These scripts are written without
// spaces, so splitting on whitespace would score a whole Japanese sentence as
// one word; counting each character instead matches the characters-per-minute
// convention those languages actually use.
const CJK_CHARS =
  /[\u3040-\u30ff\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff\uff66-\uff9f]/gu;

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

const RecordingOverlay: React.FC = () => {
  const { t } = useTranslation();
  const [isVisible, setIsVisible] = useState(false);
  const [state, setState] = useState<OverlayState>("recording");
  // `Stream::play()` returning does not mean hardware callbacks are flowing.
  // Stay visually in an arming state until the backend processes the first
  // actual microphone sample chunk.
  const [captureReady, setCaptureReady] = useState(false);
  // Poll rate of the miniature analyser: the Live FFT page's update rate,
  // read with the other overlay settings on every show.
  const [scopeRate, setScopeRate] = useState(30);
  // The picture settings (`overlay_scope`): views, style, waveform window,
  // size. Read with the other overlay settings on every show; the geometry
  // is published as CSS variables before the card becomes visible.
  const [scopeConfig, setScopeConfig] = useState<ResolvedOverlayScope>(
    OVERLAY_SCOPE_DEFAULTS,
  );
  const [streamText, setStreamText] = useState<StreamTextEvent>({
    committed: "",
    tentative: "",
  });
  const [phase, setPhase] = useState<StreamPhase>("listening");
  const [workKind, setWorkKind] = useState<StreamWorkKind>("transcribing");
  const [elapsed, setElapsed] = useState(0);
  // Speech statistics, driven by the backend VAD (see SpeechActivityEvent).
  // `speechMs` counts only time the user was actually talking, so dividing the
  // transcribed word count by it gives a speaking rate rather than a
  // recording-length average.
  const [statsEnabled, setStatsEnabled] = useState(false);
  const [speaking, setSpeaking] = useState(false);
  const [speechMs, setSpeechMs] = useState(0);
  const [wordCount, setWordCount] = useState(0);
  // Bumped on each new streaming session so the Live card remounts fresh (replays
  // the pop-in, and never animates in from the previous panel's open size).
  const [session, setSession] = useState(0);
  // Overlay placement (top vs bottom of the screen). The Live panel grows downward
  // from a top overlay (oldest line under the pill) and upward from a bottom one.
  const [position, setPosition] = useState<"top" | "bottom">("bottom");
  // True once live text overflows the cap. A top overlay fades its top edge only
  // while overflowing, so the resting first line stays crisp flush under the pill.
  const [overflowing, setOverflowing] = useState(false);
  // Experimental Multi-STT streaming mode: the text on screen is the whole
  // session's, composed chunk by chunk, so the card grows with it instead of
  // hiding the earlier chunks behind the fade. `textCap` is the backend's answer
  // to a height report — the same number it sized the window with, past which
  // this card scrolls back (see `overlay_stream_text_height`).
  const [wholeSession, setWholeSession] = useState(false);
  const [failedChunks, setFailedChunks] = useState(0);
  const [textCap, setTextCap] = useState<number | null>(null);
  const reportedHeightRef = useRef(-1);

  // Live-text scroll-back: the text region "sticks" to the newest line while the
  // user is at the bottom; if they scroll up to read history, auto-follow pauses
  // until they scroll back down.
  const capRef = useRef<HTMLDivElement>(null);
  const pinnedRef = useRef(true);
  const direction = getLanguageDirection(i18n.language);

  const directModeRef = useRef(false);
  const directSpeedRef = useRef(30);
  const targetTextRef = useRef<StreamTextEvent>({
    committed: "",
    tentative: "",
  });
  const displayedTextRef = useRef<StreamTextEvent>({
    committed: "",
    tentative: "",
  });
  const typewriterTimerRef = useRef<ReturnType<typeof setInterval> | null>(
    null,
  );

  const stopTypewriter = () => {
    if (typewriterTimerRef.current !== null) {
      clearInterval(typewriterTimerRef.current);
      typewriterTimerRef.current = null;
    }
  };

  const flushTypewriter = () => {
    stopTypewriter();
    displayedTextRef.current = { ...targetTextRef.current };
    setStreamText({ ...targetTextRef.current });
  };

  const stepTypewriter = () => {
    const target = targetTextRef.current;
    const current = displayedTextRef.current;

    if (
      current.committed === target.committed &&
      current.tentative === target.tentative
    ) {
      stopTypewriter();
      return;
    }

    let nextCommitted = current.committed;
    let nextTentative = current.tentative;
    const threshold = Math.max(
      15,
      Math.round((directSpeedRef.current || 50) * 0.6),
    );

    if (current.committed !== target.committed) {
      if (target.committed.startsWith(current.committed)) {
        const remaining = target.committed.length - current.committed.length;
        const step =
          remaining > threshold * 2 ? 3 : remaining > threshold ? 2 : 1;
        nextCommitted = target.committed.slice(
          0,
          current.committed.length + step,
        );
      } else {
        nextCommitted = target.committed;
      }
    } else if (current.tentative !== target.tentative) {
      if (target.tentative.startsWith(current.tentative)) {
        const remaining = target.tentative.length - current.tentative.length;
        const step =
          remaining > threshold * 2 ? 3 : remaining > threshold ? 2 : 1;
        nextTentative = target.tentative.slice(
          0,
          current.tentative.length + step,
        );
      } else {
        let prefixLen = 0;
        while (
          prefixLen < current.tentative.length &&
          prefixLen < target.tentative.length &&
          current.tentative[prefixLen] === target.tentative[prefixLen]
        ) {
          prefixLen++;
        }
        if (current.tentative.length > prefixLen) {
          nextTentative = target.tentative.slice(0, prefixLen);
        } else {
          const remaining = target.tentative.length - prefixLen;
          const step =
            remaining > threshold * 2 ? 3 : remaining > threshold ? 2 : 1;
          nextTentative = target.tentative.slice(0, prefixLen + step);
        }
      }
    }

    displayedTextRef.current = {
      committed: nextCommitted,
      tentative: nextTentative,
    };
    setStreamText({ committed: nextCommitted, tentative: nextTentative });
  };

  const startTypewriterIfNeeded = () => {
    if (typewriterTimerRef.current === null) {
      const speed = directSpeedRef.current || 30;
      const intervalMs = Math.max(8, Math.min(100, Math.round(1000 / speed)));
      typewriterTimerRef.current = setInterval(stepTypewriter, intervalMs);
    }
  };

  useEffect(() => {
    const setupEventListeners = async () => {
      const unlistenShow = await listen("show-overlay", async (event) => {
        const overlayState = event.payload as OverlayState;
        // Reset synchronously before settings I/O. A fast microphone can emit
        // recording-ready while the awaits below are in flight; resetting after
        // them would overwrite that event and leave the overlay stuck arming.
        if (overlayState === "recording" || overlayState === "streaming") {
          setCaptureReady(false);
          stopTypewriter();
          targetTextRef.current = { committed: "", tentative: "" };
          displayedTextRef.current = { committed: "", tentative: "" };
          setStreamText({ committed: "", tentative: "" });
          setSpeaking(false);
          setSpeechMs(0);
          setWordCount(0);
          // A previous session's composed text must not size this card: the
          // mode announces itself with its first event, and the height with it.
          setWholeSession(false);
          setFailedChunks(0);
          setTextCap(null);
          reportedHeightRef.current = -1;
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
            directModeRef.current = settings.data.overlay_direct_mode ?? false;
            directSpeedRef.current = settings.data.overlay_direct_speed ?? 30;
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
        targetTextRef.current = event.payload;
        // Count from the backend text, not the typewriter's partial reveal, so
        // direct mode cannot make the speaking rate read artificially low. This
        // runs even in the minimal overlay, which never renders the text but
        // still receives it whenever the model streams.
        setWordCount(
          countWords(
            `${event.payload.committed} ${event.payload.tentative}`.trim(),
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
        if (!directModeRef.current || event.payload.whole_session) {
          stopTypewriter();
          displayedTextRef.current = event.payload;
          setStreamText(event.payload);
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

      return () => {
        stopTypewriter();
        unlistenShow();
        unlistenHide();
        unlistenReady();
        unlistenStream();
        unlistenSpeech();
        unlistenPhase();
      };
    };

    // Keep the unlisten handles: under React StrictMode (dev) the effect runs
    // twice, and without cleanup every event got two handlers — two session
    // bumps per show and two typewriter intervals in direct mode.
    let disposed = false;
    let cleanup: (() => void) | undefined;
    setupEventListeners().then((fn) => {
      if (disposed) {
        fn();
      } else {
        cleanup = fn;
      }
    });

    // Prime the stats setting before the first show, so the card opens at its
    // final width instead of visibly growing once the settings read lands.
    commands
      .getAppSettings()
      .then((settings) => {
        if (settings.status === "ok") {
          setStatsEnabled(settings.data.overlay_speech_stats ?? true);
        }
      })
      .catch(() => {
        // Keep the primed default (on) until the first show reads settings again.
      });

    return () => {
      disposed = true;
      cleanup?.();
    };
  }, []);

  // Elapsed capture timer starts only once microphone samples are flowing.
  useEffect(() => {
    if (state !== "streaming" || !isVisible || !captureReady) return;
    const id = setInterval(() => setElapsed((e) => e + 1), 1000);
    return () => clearInterval(id);
  }, [state, isVisible, captureReady]);

  // Stick to the bottom as text streams in — but only while pinned, so a user who
  // has scrolled up to read history isn't yanked back down by the next chunk.
  useLayoutEffect(() => {
    const el = capRef.current;
    if (!el) return;
    // Fade the top edge only once text actually overflows the cap.
    setOverflowing(el.scrollHeight > el.clientHeight + 1);
    if (pinnedRef.current) el.scrollTop = el.scrollHeight;
  }, [streamText]);

  // Grow the card with the session's text (experimental Multi-STT streaming mode)
  // and read back the height it may reach before scrolling. Measured after layout
  // so the report matches what is on screen, and rounded up to a step so this is
  // one call per line of text; the backend clamps to its own monitor-based cap and
  // returns that clamp, which is what bounds this card too.
  useLayoutEffect(() => {
    if (!wholeSession) return;
    const el = capRef.current;
    if (!el) return;
    const stepped =
      Math.ceil(el.scrollHeight / TEXT_HEIGHT_STEP_PX) * TEXT_HEIGHT_STEP_PX;
    if (stepped === reportedHeightRef.current) return;
    reportedHeightRef.current = stepped;
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
  }, [streamText, wholeSession]);

  // Each fresh streaming session starts pinned to the bottom, fade cleared.
  useEffect(() => {
    pinnedRef.current = true;
    setOverflowing(false);
  }, [session]);

  if (!isVisible) return null;

  // Re-pin when the user is within ~a line of the bottom; unpin otherwise.
  const handleStreamScroll = () => {
    const el = capRef.current;
    if (!el) return;
    pinnedRef.current = el.scrollHeight - el.scrollTop - el.clientHeight <= 16;
  };

  const fmtTime = (s: number) =>
    `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;

  // Speech stats ride along in both overlay forms. Rendered from the moment the
  // card appears rather than waiting for the first sample, so the Live card
  // steps its width once (pill -> panel) instead of twice.
  const showStats = statsEnabled;
  // "Nobody is talking right now." The arming case (microphone not started yet)
  // is handled separately below and takes visual precedence.
  const quiet = showStats && !speaking;
  const wpm =
    speechMs >= WPM_MIN_SPEECH_MS && wordCount >= WPM_MIN_WORDS
      ? Math.round(wordCount / (speechMs / 60000))
      : null;

  // ---- Shared building blocks (one visual language for every overlay form) ----
  // The miniature analyser: an area spectrum and the last 4096 samples as a
  // line, both computed with the Live FFT page's settings (OverlayScope).
  const waveform =
    scopeConfig.show_spectrum || scopeConfig.show_wave ? (
      <div
        className={`swave ${captureReady ? "ready" : "arming"} ${quiet ? "quiet" : ""}`}
      >
        <OverlayScope
          rateHz={scopeRate}
          ready={captureReady}
          quiet={quiet}
          config={scopeConfig}
        />
      </div>
    ) : null;

  const cancelBtn = (
    <button
      className="sx"
      aria-label="cancel"
      onClick={() => commands.cancelOperation()}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <path
          d="M4 4 L12 12 M12 4 L4 12"
          stroke="currentColor"
          strokeWidth="1.6"
          strokeLinecap="round"
        />
      </svg>
    </button>
  );

  // Speech-only clock and running words-per-minute. The clock freezes on
  // silence, which is the whole point — it measures talking, not recording.
  const statsCluster = (
    <span className={`sstats ${quiet ? "quiet" : ""}`}>
      <span className="sstat-speech">
        {fmtTime(Math.floor(speechMs / 1000))}
      </span>
      <span className="sstat-sep" aria-hidden="true" />
      <span className="sstat-wpm">
        {wpm === null ? (
          <span className="sstat-pending">{"—"}</span>
        ) : (
          `${wpm} ${t("overlay.wpm")}`
        )}
      </span>
    </span>
  );

  // The numeric readouts, in reading order: total elapsed, then the speech
  // clock and rate.
  const readouts = (showTimer: boolean) => (
    <>
      {showTimer && <span className="stimer">{fmtTime(elapsed)}</span>}
      {showStats && statsCluster}
    </>
  );

  // Without stats: dot (left) | waveform (center) | timer + cancel (right),
  // the original three-zone grid that keeps the waveform centered.
  //
  // With stats: dot, waveform and readouts run together as one evenly spaced
  // group, and only the cancel button is pinned to the right edge. Keeping the
  // readouts in the right-hand cluster instead would put every pixel of slack
  // between the waveform and the numbers — separating things that belong
  // together, by a gap that grows with the card.
  const listeningRow = (
    showTimer: boolean,
    showCancel: boolean,
    badge: React.ReactNode = null,
  ) => (
    <div className={`sbase ${showStats ? "has-stats" : ""}`}>
      <div className="sbase-l">
        <span
          className={`sdot ${
            !captureReady ? "arming" : quiet ? "silent" : "ready"
          }`}
        />
      </div>
      {waveform}
      {showStats && <div className="smeta">{readouts(showTimer)}</div>}
      <div className="sbase-r">
        {!showStats && readouts(showTimer)}
        {badge}
        {showCancel && cancelBtn}
      </div>
    </div>
  );

  // spinner (left) | label (center) | cancel (right) — same 3-zone grid as the
  // listening row, so the label is centered.
  const workingRow = (label: string, showCancel: boolean) => (
    <div className="sbase">
      <div className="sbase-l">
        <span className="sspinner" />
      </div>
      <span className="swork-label">{label}</span>
      <div className="sbase-r">{showCancel && cancelBtn}</div>
    </div>
  );

  // ---- Live overlay: a pill that sculpts open into a panel ----
  if (state === "streaming") {
    const hasText =
      streamText.committed.length > 0 || streamText.tentative.length > 0;
    const working = phase === "working";
    // Keep the panel open whenever there's text — even while finalizing — so the
    // transcript stays put under a working spinner instead of collapsing and
    // squishing the text mid-stream. Only fall back to the small working pill
    // when there was no text to preserve.
    const open = hasText;
    const collapsed = working && !hasText;

    // A chunk whose merge failed shows the extras' outputs concatenated — still
    // the session's text, just not the cleaned version. It is marked here rather
    // than in the text: with DirectStreaming the text is typed into the user's
    // document, and a marker inside it would be typed too.
    const failBadge =
      failedChunks > 0 ? (
        <span className="sfail" title={t("overlay.chunkFailedHint")}>
          {t("overlay.chunkFailed", { count: failedChunks })}
        </span>
      ) : null;

    return (
      <div
        dir={direction}
        className={`ov-stage ${position}`}
        style={
          textCap === null
            ? undefined
            : ({ "--ov-cap-max-h": `${textCap}px` } as React.CSSProperties)
        }
      >
        <div
          key={session}
          className={`scard ${open ? "open" : ""} ${collapsed ? "working" : ""} ${
            showStats && !open && !collapsed ? "has-stats" : ""
          } ${isVisible ? "" : "leaving"}`}
        >
          <div className="stext">
            <div className="stext-clip">
              <div
                className={`stext-cap ${overflowing ? "overflowing" : ""}`}
                ref={capRef}
                onScroll={handleStreamScroll}
              >
                <p>
                  <span className="committed">
                    {streamText.committed ? streamText.committed + " " : ""}
                  </span>
                  <span className="tentative">{streamText.tentative}</span>
                  {/* Drop the blinking caret once finalizing — it's no longer
                      capturing, and a static spinner conveys the work. */}
                  {!working && <span className="scaret" />}
                </p>
              </div>
            </div>
          </div>
          {working
            ? workingRow(
                workKind === "polishing"
                  ? t("overlay.processing")
                  : t("overlay.transcribing"),
                true,
              )
            : listeningRow(open, true, failBadge)}
        </div>
      </div>
    );
  }

  // ---- Minimal overlay: exactly one row at a time — waveform (recording), or a
  // spinner + label (transcribing / processing). Never both. The pill animates its
  // width between them; the cancel button is in both rows so it stays put.
  const working = state === "transcribing" || state === "processing";
  const workLabel =
    state === "processing"
      ? t("overlay.processing")
      : t("overlay.transcribing");

  return (
    <div
      dir={direction}
      className={`ov-stage ${position} ov-fade ${isVisible ? "show" : ""}`}
    >
      <div
        className={`scard compact ${working && isVisible ? "cworking" : ""} ${
          showStats && !working ? "has-stats" : ""
        }`}
      >
        {working ? workingRow(workLabel, true) : listeningRow(false, true)}
      </div>
    </div>
  );
};

export default RecordingOverlay;
