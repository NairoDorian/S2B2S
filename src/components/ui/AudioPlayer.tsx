import {
  createContext,
  useContext,
  createSignal,
  createEffect,
  createMemo,
  onSettled,
  Show,
} from "solid-js";
import { Play, Pause } from "@/components/icons/lucide";
import type { JSX } from "@solidjs/web";

interface AudioPlayerProps {
  src?: string;
  onLoadRequest?: () => Promise<string | null>;
  class?: string;
  autoPlay?: boolean;
}

interface AudioPlayerGroupContextValue {
  requestPlayback: (audio: HTMLAudioElement) => void;
  releasePlayback: (audio: HTMLAudioElement) => void;
}

const AudioPlayerGroupContext =
  createContext<AudioPlayerGroupContextValue | null>(null);

export const AudioPlayerGroup = (props: {
  children?: JSX.Element;
}): JSX.Element => {
  let activeAudioRef: HTMLAudioElement | null = null;
  const value = createMemo<AudioPlayerGroupContextValue>(() => ({
    requestPlayback: (audio) => {
      if (activeAudioRef !== audio) activeAudioRef?.pause();
      activeAudioRef = audio;
    },
    releasePlayback: (audio) => {
      if (activeAudioRef === audio) activeAudioRef = null;
    },
  }));

  return (
    <AudioPlayerGroupContext value={value()}>
      {props.children}
    </AudioPlayerGroupContext>
  );
};

const formatTime = (time: number): string => {
  if (!isFinite(time)) return "0:00";
  const minutes = Math.floor(time / 60);
  const seconds = Math.floor(time % 60);
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
};

export const AudioPlayer = (props: AudioPlayerProps): JSX.Element => {
  const group = useContext(AudioPlayerGroupContext);
  const [isPlaying, setIsPlaying] = createSignal(false);
  const [duration, setDuration] = createSignal(0);
  const [currentTime, setCurrentTime] = createSignal(0);
  const [isDragging, setIsDragging] = createSignal(false);
  // `props.src` seeds the initial value once; everything after reads the
  // signal so a newly loaded recording swaps the element's source.
  const [loadedSrc, setLoadedSrc] = createSignal<string | null>(
    props.src ?? null,
  );
  const [isLoading, setIsLoading] = createSignal(false);

  let audioRef!: HTMLAudioElement;
  let animationRef: number | undefined = undefined;
  let dragTimeRef: number = 0;

  // The tick loop runs outside any tracking scope (a rAF callback), so the
  // reads below are untracked on purpose — they only need the current value,
  // and the loop re-schedules itself while playing.
  const tick = () => {
    if (audioRef && !isDragging()) {
      setCurrentTime(audioRef.currentTime);
    }
    if (isPlaying()) {
      animationRef = requestAnimationFrame(tick);
    }
  };

  // The animation loop follows the play/drag state. The reads are in the
  // compute phase so the effect re-runs on every flip; the rAF handle is
  // cancelled in the returned teardown (re-runs replace it, disposal ends it).
  createEffect(
    () => [isPlaying(), isDragging()] as const,
    ([playing, dragging]) => {
      if (playing && !dragging) {
        if (!animationRef) animationRef = requestAnimationFrame(tick);
      } else {
        if (animationRef) {
          cancelAnimationFrame(animationRef);
          animationRef = undefined;
        }
      }
      return () => {
        if (animationRef) {
          cancelAnimationFrame(animationRef);
          animationRef = undefined;
        }
      };
    },
  );

  // One-time setup: element event listeners. The ref is assigned during
  // render, so it is present by the time onSettled runs.
  onSettled(() => {
    const audio = audioRef;
    if (!audio) return;
    const handleLoadedMetadata = () => {
      setDuration(audio.duration || 0);
      setCurrentTime(0);
    };
    const handleEnded = () => {
      group?.releasePlayback(audio);
      setIsPlaying(false);
      setCurrentTime(audio.duration || 0);
    };
    const handlePlay = () => {
      group?.requestPlayback(audio);
      setIsPlaying(true);
    };
    const handlePause = () => {
      group?.releasePlayback(audio);
      setIsPlaying(false);
    };

    audio.addEventListener("loadedmetadata", handleLoadedMetadata);
    audio.addEventListener("ended", handleEnded);
    audio.addEventListener("play", handlePlay);
    audio.addEventListener("pause", handlePause);
    return () => {
      group?.releasePlayback(audio);
      audio.removeEventListener("loadedmetadata", handleLoadedMetadata);
      audio.removeEventListener("ended", handleEnded);
      audio.removeEventListener("play", handlePlay);
      audio.removeEventListener("pause", handlePause);
    };
  });

  // Auto-play when a recording is first loaded (either the initial src or an
  // onLoadRequest fetch). Tracking loadedSrc is the point: the previous
  // mount-once form ran with the initial null and could never fire again.
  let prevLoadedSrc: string | null = null;
  createEffect(
    () => loadedSrc(),
    (current) => {
      const audio = audioRef;
      if (!audio) return;
      if (current && !prevLoadedSrc && props.onLoadRequest) {
        audio.play().catch((error) => {
          console.error("Auto-play failed:", error);
        });
      } else if (props.autoPlay && props.src && !prevLoadedSrc) {
        audio.play().catch((error) => {
          console.error("Auto-play failed:", error);
        });
      }
      prevLoadedSrc = current;
    },
  );

  const handleMouseUp = () => {
    if (isDragging()) {
      setIsDragging(false);
      if (audioRef) {
        audioRef.currentTime = dragTimeRef;
        setCurrentTime(dragTimeRef);
      }
    }
  };

  // Global mouseup/touchend while the slider is being dragged. Keyed on the
  // drag signal: the listeners attach when the drag starts and the returned
  // teardown removes them when it ends.
  createEffect(
    () => isDragging(),
    (dragging) => {
      if (!dragging) return;
      document.addEventListener("mouseup", handleMouseUp);
      document.addEventListener("touchend", handleMouseUp);
      return () => {
        document.removeEventListener("mouseup", handleMouseUp);
        document.removeEventListener("touchend", handleMouseUp);
      };
    },
  );

  // Blob URLs must be revoked when the player goes away; read the current
  // source at disposal time, not at mount.
  onSettled(() => {
    return () => {
      const url = loadedSrc();
      if (url?.startsWith("blob:")) URL.revokeObjectURL(url);
    };
  });

  const togglePlay = async () => {
    const audio = audioRef;
    if (!audio) return;
    if (isLoading()) return;
    try {
      if (isPlaying()) {
        audio.pause();
      } else {
        if (!loadedSrc() && props.onLoadRequest) {
          setIsLoading(true);
          const newSrc = await props.onLoadRequest();
          setIsLoading(false);
          if (newSrc) setLoadedSrc(newSrc);
        } else if (loadedSrc()) {
          await audio.play();
        }
      }
    } catch (error) {
      console.error("Playback failed:", error);
    }
  };

  const handleSeek = (e: Event) => {
    const newTime = parseFloat((e.target as HTMLInputElement).value);
    dragTimeRef = newTime;
    setCurrentTime(newTime);
    if (!isDragging() && audioRef) audioRef.currentTime = newTime;
  };

  const handleSliderMouseDown = () => setIsDragging(true);
  const handleSliderTouchStart = () => setIsDragging(true);

  const getProgressPercent = (): number => {
    if (duration() <= 0) return 0;
    if (duration() - currentTime() < 0.1) return 100;
    const percent = (currentTime() / duration()) * 100;
    return Math.min(100, Math.max(0, percent));
  };

  return (
    <div class={`flex items-center gap-3 ${props.class ?? ""}`}>
      <audio
        ref={(el) => {
          audioRef = el;
        }}
        src={loadedSrc() ?? undefined}
        preload="metadata"
      />
      <button
        onClick={togglePlay}
        disabled={isLoading()}
        class="transition-colors cursor-pointer text-text hover:text-accent disabled:opacity-50"
        aria-label={isPlaying() ? "Pause" : "Play"}
      >
        <Show
          when={isPlaying()}
          fallback={<Play width={20} height={20} fill="currentColor" />}
        >
          <Pause width={20} height={20} fill="currentColor" />
        </Show>
      </button>
      <div class="flex-1 flex items-center gap-2">
        <span class="text-xs text-text/60 min-w-[30px] tabular-nums">
          {formatTime(currentTime())}
        </span>
        <input
          type="range"
          min="0"
          max={duration() || 0}
          step="0.01"
          value={currentTime()}
          onInput={handleSeek}
          onMouseDown={handleSliderMouseDown}
          onTouchStart={handleSliderTouchStart}
          class={`flex-1 h-1 rounded-lg appearance-none cursor-pointer focus:outline-none focus:ring-1 focus:ring-accent ${getProgressPercent() >= 99.5 ? "[&::-webkit-slider-thumb]:translate-x-0.5 [&::-moz-range-thumb]:translate-x-0.5" : ""}`}
          style={{
            background: `linear-gradient(to right, var(--color-accent) 0%, var(--color-accent) ${getProgressPercent()}%, rgba(128, 128, 128, 0.2) ${getProgressPercent()}%, rgba(128, 128, 128, 0.2) 100%)`,
          }}
        />
        <span class="text-xs text-text/60 min-w-[30px] tabular-nums">
          {formatTime(duration())}
        </span>
      </div>
    </div>
  );
};
