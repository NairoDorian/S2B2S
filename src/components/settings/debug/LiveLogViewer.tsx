import { createSignal, createEffect, For, Show } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { commands } from "@/bindings";
import { sessionToast as toast } from "@/lib/sessionToast";
import { SettingContainer } from "../../ui/SettingContainer";
import { Button } from "../../ui/Button";
import { parseLogRecords, type LogLine, type Tag } from "./logParse";

// Capture all records in the backend file; poll only while this view exists.
// A single source avoids live/file deduplication deleting legitimate records.
const FILE_POLL_LINES = 2000;
const FILE_POLL_INTERVAL_MS = 500;
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

interface LiveLogViewerProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

export const LiveLogViewer = (props: LiveLogViewerProps) => {
  const { t } = useTranslation();
  const [logs, setLogs] = createSignal<LogLine[]>([]);
  const [hiddenTags, setHiddenTags] = createSignal<Set<Tag>>(new Set());
  const visible = (tag: Tag) => !hiddenTags().has(tag);
  const toggleTag = (tag: Tag) =>
    setHiddenTags((prev) => {
      const next = new Set(prev);
      if (next.has(tag)) next.delete(tag);
      else next.add(tag);
      return next;
    });
  const [paused, setPaused] = createSignal(false);
  const [copied, setCopied] = createSignal(false);

  let pinnedRef = true;
  let nextId = 0;
  let scrollEl: HTMLDivElement | null = null;

  createEffect(
    () => paused(),
    (isPaused) => {
      if (isPaused) return;
      let disposed = false;
      let timer: ReturnType<typeof setTimeout> | undefined;
      const refresh = async () => {
        try {
          const result = await commands.getRecentLogs(FILE_POLL_LINES);
          if (disposed) return;
          if (result.status === "ok") {
            const next = parseLogRecords(result.data);
            setLogs((previous) => {
              if (
                previous.length === next.length &&
                previous.every((line, i) => line.raw === next[i].raw)
              )
                return previous;
              const available = new Map<string, LogLine[]>();
              for (const line of previous) {
                const matches = available.get(line.raw) ?? [];
                matches.push(line);
                available.set(line.raw, matches);
              }
              return next.map((line) => {
                const existing = available.get(line.raw)?.shift();
                if (existing) return existing;
                line.id = nextId++;
                return line;
              });
            });
          } else {
            console.error("Failed to read log file:", result.error);
          }
        } catch (error) {
          console.error("Failed to read log file:", error);
        } finally {
          if (!disposed) timer = setTimeout(refresh, FILE_POLL_INTERVAL_MS);
        }
      };
      void refresh();
      return () => {
        disposed = true;
        clearTimeout(timer);
      };
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
    setLogs([]);
    pinnedRef = true;
    commands.clearLogs().catch((error) => {
      console.error("Failed to clear log file:", error);
      toast.error(String(error));
    });
  };

  const handleCopy = async () => {
    const text = logs()
      .filter((l) => visible(l.tag))
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
          onClick={() => setHiddenTags(new Set())}
          class={`px-2 py-0.5 rounded border text-[11px] font-mono cursor-pointer transition-colors ${
            hiddenTags().size === 0
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
              onClick={() => toggleTag(tag)}
              class={`px-2 py-0.5 rounded border text-[11px] font-mono cursor-pointer transition-colors ${
                visible(tag)
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
          when={logs().some((l) => visible(l.tag))}
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
              <Show when={visible(line.tag)}>
                <div class="flex gap-2">
                  <Show when={line.time}>
                    <span class="text-mid-gray/80 shrink-0 select-none tabular-nums">
                      {line.time}
                    </span>
                  </Show>
                  <Show when={line.target}>
                    <span class="text-mid-gray/50 shrink-0 select-none">
                      {line.target}
                    </span>
                  </Show>
                  {/* INF is the default severity: hide the chip so routine
                      records read as plain console text. WRN/ERR/DBG/TRC
                      keep a colored tag for scanning. */}
                  <Show when={line.tag !== "INF"}>
                    <span
                      class={`${TAG_META[line.tag].tagClass} shrink-0 select-none w-[3.5rem]`}
                    >
                      {line.tag}
                    </span>
                  </Show>
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
