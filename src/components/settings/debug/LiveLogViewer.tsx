import { createSignal, createEffect, For, Show } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { listen } from "@tauri-apps/api/event";
import { commands } from "@/bindings";
import { sessionToast as toast } from "@/lib/sessionToast";
import { SettingContainer } from "../../ui/SettingContainer";
import { Button } from "../../ui/Button";

// The console is a *console*: it shows every category all the time, keeps
// growing, and never clears itself. The only selector is the tag row below —
// owned by the console, no dropdown menus anywhere.

// Maximum number of lines kept in memory / rendered at once. Old lines fall
// off the top only when this hard cap is reached — a chatty app must not
// balloon memory, but the cap is far above what a session realistically
// produces, so in practice the log keeps growing for the whole sitting.
const MAX_LINES = 2000;
// How many lines the file poll asks the backend for. Fixed — there is no
// "last N lines" selector; the file is the ground truth and the cap above
// bounds what is kept.
const FILE_POLL_LINES = 2000;
// Incoming logs are buffered and flushed on this cadence so a burst of log
// activity can never trigger a render per line.
const FLUSH_INTERVAL_MS = 250;
// The file is re-read on this cadence. It reconciles anything the live stream
// missed (records emitted before this page attached, or while it was paused)
// and is what makes the panel's history independent of when it was opened.
const FILE_POLL_INTERVAL_MS = 2000;

// Payload emitted by tauri-plugin-log `Webview` target on the `log://log`
// event. `level` is the numeric LogLevel repr: Trace=1, Debug=2, Info=3,
// Warn=4, Error=5. `message` is the raw log message (no timestamp/target).
interface LogEventPayload {
  message: string;
  level: number;
}

type Tag = "ERR" | "WRN" | "INF" | "DBG" | "TRC";

interface LogLine {
  id: number;
  tag: Tag;
  time: string;
  message: string;
  live: boolean;
  raw: string;
}

const LIVE_LEVEL_TO_TAG: Record<number, Tag> = {
  1: "TRC",
  2: "DBG",
  3: "INF",
  4: "WRN",
  5: "ERR",
};

// File lines look like:
//   [2026-09-12][20:30:04][app_lib][DEBUG] message
// Continuation lines of a wrapped record do not match and land as INFO with
// the raw text intact — showing more text beats dropping a line.
const parseFileLine = (raw: string): LogLine => {
  const m = raw.match(/^\[([^\]]+)\]\[([^\]]+)\]\[[^\]]*\]\[(\w+)\]\s?(.*)$/);
  if (!m) {
    return {
      id: 0,
      tag: "INF",
      time: "",
      message: raw,
      live: false,
      raw,
    };
  }
  const tag = (
    m[4].toUpperCase().slice(0, 4) === "WARN"
      ? "WRN"
      : m[4].toUpperCase().slice(0, 4) === "ERRO"
        ? "ERR"
        : m[4].toUpperCase().slice(0, 4) === "TRAC"
          ? "TRC"
          : m[4].toUpperCase().slice(0, 4) === "DEBU"
            ? "DBG"
            : "INF"
  ) as Tag;
  return {
    id: 0,
    tag,
    time: m[2],
    message: m[5],
    live: false,
    raw,
  };
};

// Live records arrive with the app's log format already applied, so the
// message carries the file line's own `[date][time][target][LEVEL]` prefix —
// parse it back out instead of stacking a second timestamp on top. Returns
// null for records with no usable message: they are never rendered, so a
// malformed payload cannot fill the console with "undefined" lines.
const liveLineFrom = (payload: LogEventPayload, id: number): LogLine | null => {
  if (payload == null || payload.message == null) return null;
  const parsed = parseFileLine(String(payload.message));
  if (parsed.message.trim() === "") return null;
  return {
    id,
    tag: LIVE_LEVEL_TO_TAG[payload.level] ?? parsed.tag,
    time: parsed.time || formatTime(new Date()),
    message: parsed.message,
    live: true,
    raw: parsed.raw,
  };
};

// Level accents carry a light-theme color plus a brighter `dark:` variant, so
// they read on both the light code-block surface and the dark console surface.
const TAG_META: Record<Tag, { tagClass: string; msgClass: string }> = {
  TRC: { tagClass: "text-mid-gray", msgClass: "text-mid-gray" },
  DBG: {
    tagClass: "text-sky-600 dark:text-sky-400",
    msgClass: "text-text/80",
  },
  INF: {
    tagClass: "text-emerald-600 dark:text-emerald-400",
    msgClass: "text-text",
  },
  WRN: {
    tagClass: "text-amber-600 dark:text-amber-400",
    msgClass: "text-amber-700 dark:text-amber-300",
  },
  ERR: {
    tagClass: "text-red-600 dark:text-red-400",
    msgClass: "text-red-700 dark:text-red-300",
  },
};

const TAGS: readonly Tag[] = ["ERR", "WRN", "INF", "DBG", "TRC"];

const formatTime = (date: Date): string => {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(
    date.getSeconds(),
  )}`;
};

interface LiveLogViewerProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

export const LiveLogViewer = (props: LiveLogViewerProps) => {
  const { t } = useTranslation();
  const [logs, setLogs] = createSignal<LogLine[]>([]);
  const [selectedTag, setSelectedTag] = createSignal<Tag | "ALL">("ALL");
  const [paused, setPaused] = createSignal(false);
  const [copied, setCopied] = createSignal(false);

  let pending: LogLine[] = [];
  let idCounter = 0;
  let pausedRef = false;
  let pinnedRef = true;
  let scrollEl: HTMLDivElement | null = null;

  createEffect(
    () => undefined,
    () => {
      pausedRef = paused();
    },
  );

  // Subscribe to the backend log stream. Lines land in a ref buffer rather
  // than state so high log volume never overwhelms Solid. Live lines append
  // unconditionally — they are real log emissions; the file poll below
  // reconciles any overlap.
  createEffect(
    () => undefined,
    () => {
      const unlisten = listen<LogEventPayload>("log://log", (event) => {
        const line = liveLineFrom(event.payload, idCounter++);
        if (!line) return;
        if (pausedRef) {
          // Pausing freezes the view, not the collection: lines buffer (and
          // the buffer is bounded) until resume replays them.
          pending.push(line);
          if (pending.length > MAX_LINES) {
            pending.splice(0, pending.length - MAX_LINES);
          }
          return;
        }
        pending.push(line);
      });

      return () => {
        unlisten.then((fn) => fn());
      };
    },
  );

  // Append the pending buffer into state on a fixed cadence to cap renders.
  createEffect(
    () => undefined,
    () => {
      const interval = setInterval(() => {
        if (pending.length === 0) return;
        const incoming = pending;
        pending = [];
        if (pausedRef) {
          pending = incoming;
          return;
        }
        setLogs((prev) => {
          const next = prev.concat(incoming);
          return next.length > MAX_LINES
            ? next.slice(next.length - MAX_LINES)
            : next;
        });
      }, FLUSH_INTERVAL_MS);

      return () => clearInterval(interval);
    },
  );

  // Merge a fresh batch of file lines into the console:
  //   1. Drop live-streamed lines whose on-disk counterpart arrived.
  //   2. Append file lines that are not already present (count-aware, so
  //      repeated identical lines survive).
  // Nothing here ever clears: the merge can only add, or replace a live line
  // with its durable file twin.
  const mergeFileLines = (fetched: LogLine[]) => {
    setLogs((prev) => {
      const rawCounts = new Map<string, number>();
      for (const l of prev) {
        rawCounts.set(l.raw, (rawCounts.get(l.raw) ?? 0) + 1);
      }
      const supersededLive = new Set<LogLine>();
      for (const l of prev) {
        if (!l.live) continue;
        if (
          fetched.some((f) => f.tag === l.tag && f.message.includes(l.message))
        ) {
          supersededLive.add(l);
        }
      }
      const out: LogLine[] = [];
      for (const l of prev) {
        if (!supersededLive.has(l)) out.push(l);
      }
      let id = idCounter;
      for (const f of fetched) {
        const c = rawCounts.get(f.raw) ?? 0;
        if (c > 0) {
          rawCounts.set(f.raw, c - 1);
          continue;
        }
        out.push({ ...f, id: id++ });
      }
      idCounter = id;
      return out.length > MAX_LINES ? out.slice(out.length - MAX_LINES) : out;
    });
  };

  const fetchFileTail = () => {
    commands
      .getRecentLogs(FILE_POLL_LINES)
      .then((res) => {
        if (res.status !== "ok") {
          console.error("Failed to read log file:", res.error);
          return;
        }
        const fetched = res.data
          .split("\n")
          .filter((l) => l.trim().length > 0)
          .map(parseFileLine);
        mergeFileLines(fetched);
      })
      .catch((err) => console.error("Failed to read log file:", err));
  };

  createEffect(
    () => undefined,
    () => {
      // Seed with the file's history immediately, then keep reconciling.
      fetchFileTail();
      const interval = setInterval(fetchFileTail, FILE_POLL_INTERVAL_MS);
      return () => clearInterval(interval);
    },
  );

  // Resume: replay the buffered lines, then resync with the file.
  createEffect(
    () => paused(),
    (isPaused, wasPaused) => {
      if (wasPaused !== undefined && !isPaused) {
        if (pending.length > 0) {
          const incoming = pending;
          pending = [];
          setLogs((prev) => {
            const next = prev.concat(incoming);
            return next.length > MAX_LINES
              ? next.slice(next.length - MAX_LINES)
              : next;
          });
        }
        fetchFileTail();
      }
    },
  );

  // Keep the view pinned to the latest line unless the user has scrolled up.
  createEffect(
    () => logs(),
    () => {
      if (pinnedRef && scrollEl) {
        scrollEl.scrollTop = scrollEl.scrollHeight;
      }
    },
  );

  const handleScroll = () => {
    const el = scrollEl;
    if (!el) return;
    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    pinnedRef = distanceFromBottom < 24;
  };

  // Explicit user action only — this is the one thing that empties the view,
  // and it empties the log file with it so the two stay the same.
  const handleClear = () => {
    pending = [];
    setLogs([]);
    pinnedRef = true;
    commands.clearLogs().catch((error) => {
      console.error("Failed to clear log file:", error);
      toast.error(String(error));
    });
  };

  const handleCopy = async () => {
    const text = logs()
      .filter((l) => selectedTag() === "ALL" || l.tag === selectedTag())
      .map((l) => `${l.time} ${l.tag} ${l.message}`)
      .join("\n");
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch (error) {
      console.error("Failed to copy logs:", error);
    }
  };

  const tagCount = (tag: Tag) => logs().filter((l) => l.tag === tag).length;

  return (
    <SettingContainer
      title={t("settings.debug.liveLogs.title")}
      description={t("settings.debug.liveLogs.description")}
      descriptionMode={props.descriptionMode}
      grouped={props.grouped ?? false}
      layout="stacked"
    >
      <div class="flex items-center justify-between mb-2 gap-2">
        <div class="flex items-center gap-2 text-xs text-mid-gray min-w-0">
          <span
            class={`inline-block w-2 h-2 rounded-full shrink-0 ${
              paused() ? "bg-mid-gray" : "bg-emerald-500 animate-pulse"
            }`}
          />
          <span class="shrink-0">
            {paused()
              ? t("settings.debug.liveLogs.paused")
              : t("settings.debug.liveLogs.live")}
          </span>
          <span class="shrink-0">·</span>
          <span class="truncate">
            {t("settings.debug.liveLogs.lineCount", { count: logs().length })}
          </span>
        </div>
        <div class="flex items-center gap-2 shrink-0">
          <Button
            variant="secondary"
            size="sm"
            onClick={() => setPaused((p) => !p)}
          >
            {paused()
              ? t("settings.debug.liveLogs.resume")
              : t("settings.debug.liveLogs.pause")}
          </Button>
          <Button
            variant="secondary"
            size="sm"
            onClick={handleCopy}
            disabled={logs().length === 0}
          >
            {copied() ? t("settings.debug.liveLogs.copied") : t("common.copy")}
          </Button>
          <Button
            variant="secondary"
            size="sm"
            onClick={handleClear}
            disabled={logs().length === 0}
          >
            {t("common.clear")}
          </Button>
        </div>
      </div>

      {/* The one selector the console owns: its own tag row. Inline chips,
          not a dropdown menu — every category is always one click away, and
          the counts show what is flowing even while filtered. */}
      <div class="flex flex-wrap items-center gap-1.5 mb-2">
        <button
          type="button"
          onClick={() => setSelectedTag("ALL")}
          class={`px-2 py-0.5 rounded border text-[11px] font-mono cursor-pointer transition-colors ${
            selectedTag() === "ALL"
              ? "bg-accent/20 border-accent text-text"
              : "border-mid-gray/20 text-mid-gray hover:border-mid-gray/50"
          }`}
        >
          ALL
        </button>
        <For each={TAGS}>
          {(tag) => (
            <button
              type="button"
              onClick={() => setSelectedTag(tag)}
              class={`px-2 py-0.5 rounded border text-[11px] font-mono cursor-pointer transition-colors ${
                selectedTag() === tag
                  ? "bg-accent/20 border-accent text-text"
                  : "border-mid-gray/20 text-mid-gray hover:border-mid-gray/50"
              }`}
            >
              {tag}
              <span class="ms-1 text-mid-gray/70">{tagCount(tag)}</span>
            </button>
          )}
        </For>
      </div>

      <div
        ref={(el) => {
          scrollEl = el;
        }}
        onScroll={handleScroll}
        class="h-80 overflow-y-auto rounded-lg border border-mid-gray/30 bg-[var(--color-log-surface)] p-3 font-mono text-xs leading-relaxed select-text"
      >
        <Show
          when={logs().some(
            (l) => selectedTag() === "ALL" || l.tag === selectedTag(),
          )}
          fallback={
            <div class="text-mid-gray select-none">
              {logs().length === 0
                ? t("settings.debug.liveLogs.empty")
                : t("settings.debug.liveLogs.noFilterMatch")}
            </div>
          }
        >
          <For each={logs()}>
            {(line) => (
              <Show
                when={selectedTag() === "ALL" || line.tag === selectedTag()}
              >
                <div class="flex gap-2">
                  <Show when={line.time}>
                    <span class="text-mid-gray/80 shrink-0 select-none tabular-nums">
                      {line.time}
                    </span>
                  </Show>
                  <span
                    class={`${TAG_META[line.tag].tagClass} shrink-0 select-none w-[3.5rem]`}
                  >
                    {line.tag}
                  </span>
                  <span
                    class={`${TAG_META[line.tag].msgClass} min-w-0 whitespace-pre-wrap break-words`}
                  >
                    {line.message}
                  </span>
                </div>
              </Show>
            )}
          </For>
        </Show>
      </div>
    </SettingContainer>
  );
};
