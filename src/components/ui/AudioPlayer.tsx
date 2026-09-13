import {
  createContext,
  useContext,
  createSignal,
  createEffect,
  createMemo,
  onCleanup,
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

export const AudioPlayer = (props: AudioPlayerProps): JSX.Element => {
  const {
    src: initialSrc,
    onLoadRequest,
    class: className = "",
    autoPlay = false,
  } = props;
  const group = useContext(AudioPlayerGroupContext);
  const [isPlaying, setIsPlaying] = createSignal(false);
  const [duration, setDuration] = createSignal(0);
  const [currentTime, setCurrentTime] = createSignal(0);
  const [isDragging, setIsDragging] = createSignal(false);
  const [loadedSrc, setLoadedSrc] = createSignal<string | null>(
    initialSrc ?? null,
  );
  const [isLoading, setIsLoading] = createSignal(false);

  let audioRef!: HTMLAudioElement;
  const src = loadedSrc();
  let animationRef: number | undefined = undefined;
  let dragTimeRef: number = 0;

  let isPlayingRef: boolean = false;
  let isDraggingRef: boolean = false;

  createEffect(
    () => undefined,
    () => {
      isPlayingRef = isPlaying();
    },
  );
  createEffect(
    () => undefined,
    () => {
      isDraggingRef = isDragging();
    },
  );

  const tick = () => {
    if (audioRef && !isDraggingRef) {
      const time = audioRef.currentTime;
      setCurrentTime(time);
    }
    if (isPlayingRef) {
      animationRef = requestAnimationFrame(tick);
    }
  };

  createEffect(
    () => undefined,
    () => {
      if (isPlaying() && !isDragging()) {
        if (!animationRef) animationRef = requestAnimationFrame(tick);
      } else {
        if (animationRef) {
          cancelAnimationFrame(animationRef);
          animationRef = undefined;
        }
      }
      onCleanup(() => {
        if (animationRef) {
          cancelAnimationFrame(animationRef);
          animationRef = undefined;
        }
      });
    },
  );

  createEffect(
    () => undefined,
    () => {
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
      onCleanup(() => {
        group?.releasePlayback(audio);
        audio.removeEventListener("loadedmetadata", handleLoadedMetadata);
        audio.removeEventListener("ended", handleEnded);
        audio.removeEventListener("play", handlePlay);
        audio.removeEventListener("pause", handlePause);
      });
    },
  );

  let prevLoadedSrc: string | null = null;
  createEffect(
    () => undefined,
    () => {
      const audio = audioRef;
      if (!audio) return;
      if (loadedSrc() && !prevLoadedSrc && onLoadRequest) {
        audio.play().catch((error) => {
          console.error("Auto-play failed:", error);
        });
      } else if (autoPlay && initialSrc && !prevLoadedSrc) {
        audio.play().catch((error) => {
          console.error("Auto-play failed:", error);
        });
      }
      prevLoadedSrc = loadedSrc();
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

  createEffect(
    () => undefined,
    () => {
      if (isDragging()) {
        document.addEventListener("mouseup", handleMouseUp);
        document.addEventListener("touchend", handleMouseUp);
        onCleanup(() => {
          document.removeEventListener("mouseup", handleMouseUp);
          document.removeEventListener("touchend", handleMouseUp);
        });
      }
    },
  );

  createEffect(
    () => undefined,
    () => {
      onCleanup(() => {
        const url = loadedSrc();
        if (url?.startsWith("blob:")) URL.revokeObjectURL(url);
      });
    },
  );

  const togglePlay = async () => {
    const audio = audioRef;
    if (!audio) return;
    if (isLoading()) return;
    try {
      if (isPlaying()) {
        audio.pause();
      } else {
        if (!src && onLoadRequest) {
          setIsLoading(true);
          const newSrc = await onLoadRequest();
          setIsLoading(false);
          if (newSrc) setLoadedSrc(newSrc);
        } else if (src) {
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

  const formatTime = (time: number): string => {
    if (!isFinite(time)) return "0:00";
    const minutes = Math.floor(time / 60);
    const seconds = Math.floor(time % 60);
    return `${minutes}:${seconds.toString().padStart(2, "0")}`;
  };

  const getProgressPercent = (): number => {
    if (duration() <= 0) return 0;
    if (duration() - currentTime() < 0.1) return 100;
    const percent = (currentTime() / duration()) * 100;
    return Math.min(100, Math.max(0, percent));
  };

  const progressPercent = getProgressPercent();

  return (
    <div class={`flex items-center gap-3 ${className}`}>
      <audio
        ref={(el) => {
          audioRef = el;
        }}
        src={src ?? undefined}
        preload="metadata"
      />
      <button
        onClick={togglePlay}
        disabled={isLoading()}
        class="transition-colors cursor-pointer text-text hover:text-accent disabled:opacity-50"
        aria-label={isPlaying() ? "Pause" : "Play"}
      >
        {isPlaying() ? (
          <Pause width={20} height={20} fill="currentColor" />
        ) : (
          <Play width={20} height={20} fill="currentColor" />
        )}
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
          class={`flex-1 h-1 rounded-lg appearance-none cursor-pointer focus:outline-none focus:ring-1 focus:ring-accent ${progressPercent >= 99.5 ? "[&::-webkit-slider-thumb]:translate-x-0.5 [&::-moz-range-thumb]:translate-x-0.5" : ""}`}
          style={{
            background: `linear-gradient(to right, var(--color-accent) 0%, var(--color-accent) ${progressPercent}%, rgba(128, 128, 128, 0.2) ${progressPercent}%, rgba(128, 128, 128, 0.2) 100%)`,
          }}
        />
        <span class="text-xs text-text/60 min-w-[30px] tabular-nums">
          {formatTime(duration())}
        </span>
      </div>
    </div>
  );
};
