import {
  createEffect,
  createMemo,
  createSignal,
  For,
  Match,
  Show,
  Switch,
  untrack,
} from "solid-js";
import type { JSX } from "@solidjs/web";
import { convertFileSrc } from "@tauri-apps/api/core";
import { readFile } from "@tauri-apps/plugin-fs";
import {
  Check,
  ChevronDown,
  ChevronUp,
  Copy,
  Cpu,
  FilePlus,
  FileText,
  FolderOpen,
  Layers,
  RotateCcw,
  Search,
  Sparkles,
  Star,
  Trash2,
  Volume2,
  Zap,
} from "@/components/icons/lucide";
import { useTranslation } from "@/i18n/useTranslation";
import { isMultiRecordingFileName } from "@/lib/appIdentity";
import { displayModelId } from "@/lib/modelId";
import { sessionToast as toast } from "@/lib/sessionToast";
import {
  commands,
  events,
  type HistoryEntry,
  type HistoryUpdatePayload,
} from "@/bindings";
import { useOsType } from "@/hooks/useOsType";
import { formatDateTime } from "@/utils/dateFormat";
import { AudioPlayer, AudioPlayerGroup } from "../../ui/AudioPlayer";
import { Button } from "../../ui/Button";
import { copyToClipboard } from "./clipboard";
import { Dialog } from "../../ui/Dialog";

interface IconButtonProps {
  onClick: () => void;
  title: string;
  disabled?: boolean;
  active?: boolean;
  class?: string;
  children: JSX.Element;
}

// Props are read through `props.*`, never destructured: a destructured prop is
// read once at mount, so `disabled` would never follow the row's retry state.
const IconButton = (props: IconButtonProps) => (
  <button
    onClick={() => props.onClick()}
    disabled={props.disabled}
    class={`p-1.5 rounded-md flex items-center justify-center transition-colors cursor-pointer disabled:cursor-not-allowed disabled:text-text/20 ${
      props.active
        ? "text-accent hover:text-accent/80 bg-accent/10"
        : "text-text/50 hover:text-accent hover:bg-mid-gray/10"
    } ${props.class ?? ""}`}
    title={props.title}
  >
    {props.children}
  </button>
);

const PAGE_SIZE = 30;

interface OpenRecordingsButtonProps {
  onClick: () => void;
  label: string;
}

const OpenRecordingsButton = (props: OpenRecordingsButtonProps) => (
  <Button
    onClick={() => props.onClick()}
    variant="secondary"
    size="sm"
    class="flex items-center gap-2"
    title={props.label}
  >
    <FolderOpen class="w-4 h-4" />
    <span>{props.label}</span>
  </Button>
);

interface DeleteRecordingsButtonProps {
  onClick: () => void;
  label: string;
  disabled?: boolean;
}

const DeleteRecordingsButton = (props: DeleteRecordingsButtonProps) => (
  <Button
    onClick={() => props.onClick()}
    variant="secondary"
    size="sm"
    class="flex items-center gap-2 text-red-400 hover:text-red-300 hover:border-red-500/40"
    title={props.label}
    disabled={props.disabled}
  >
    <Trash2 class="w-4 h-4" />
    <span>{props.label}</span>
  </Button>
);

type HistoryFilter = "all" | "saved" | "multi_stt" | "post_processed";

const retryHistoryEntry = async (id: number) => {
  const result = await commands.retryHistoryEntryTranscription(id);
  if (result.status !== "ok") {
    throw new Error(String(result.error));
  }
};

const postProcessHistoryEntry = async (id: number) => {
  const result = await commands.postProcessHistoryEntry(id);
  if (result.status !== "ok") {
    throw new Error(String(result.error));
  }
};

const multiSttHistoryEntry = async (id: number) => {
  const result = await commands.multiSttHistoryEntry(id);
  if (result.status !== "ok") {
    throw new Error(String(result.error));
  }
};

const openRecordingsFolder = async () => {
  try {
    const result = await commands.openRecordingsFolder();
    if (result.status !== "ok") {
      throw new Error(String(result.error));
    }
  } catch (error) {
    console.error("Failed to open recordings folder:", error);
  }
};

const formatDurationSec = (sec: number): string => {
  const rounded = Math.round(sec * 10) / 10;
  return Number.isInteger(rounded) ? `${rounded}s` : `${rounded.toFixed(1)}s`;
};

export const HistorySettings = () => {
  const { t } = useTranslation();
  const osType = useOsType();
  const [entries, setEntries] = createSignal<HistoryEntry[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [hasMore, setHasMore] = createSignal(true);
  const [showDeleteConfirm, setShowDeleteConfirm] = createSignal(false);
  const [isDeleting, setIsDeleting] = createSignal(false);
  const [searchQuery, setSearchQuery] = createSignal("");
  const [activeFilter, setActiveFilter] = createSignal<HistoryFilter>("all");
  // A signal, not a plain ref: the sentinel only exists once the list has
  // rendered, and the infinite-scroll effect has to re-run when it appears.
  const [sentinel, setSentinel] = createSignal<HTMLDivElement | null>(null);
  let loadingRef = false;
  const initialFocusRef: { current: HTMLElement | null } = { current: null };

  const loadPage = async (cursor?: number) => {
    const isFirstPage = cursor === undefined;
    if (!isFirstPage && loadingRef) return;
    loadingRef = true;

    if (isFirstPage) setLoading(true);

    try {
      const result = await commands.getHistoryEntries(
        cursor ?? null,
        PAGE_SIZE,
      );
      if (result.status === "ok") {
        const { entries: newEntries, has_more } = result.data;
        setEntries((prev) =>
          isFirstPage ? newEntries : [...prev, ...newEntries],
        );
        setHasMore(has_more);
      } else {
        console.error("Failed to load history entries:", result.error);
        toast.error(result.error);
      }
    } catch (error) {
      console.error("Failed to load history entries:", error);
    } finally {
      setLoading(false);
      loadingRef = false;
    }
  };

  // Initial load
  createEffect(
    () => undefined,
    () => {
      loadPage();
    },
  );

  // Infinite scroll. The compute tracks every input: the apply runs untracked,
  // so reading `loading()` / `hasMore()` there would only ever see the mount
  // values (loading = true) and never create the observer. The entry count is
  // tracked too: a later page never touches `loading()`, and an observer only
  // fires on an intersection *change*, so a sentinel still in view after a
  // page landed would otherwise never ask for the next one.
  createEffect(
    () => [loading(), hasMore(), sentinel(), entries().length] as const,
    ([isLoading, more, el]) => {
      if (isLoading || !more || !el) return;

      const observer = new IntersectionObserver(
        (observerEntries) => {
          const first = observerEntries[0];
          if (first.isIntersecting) {
            const list = entries();
            const lastEntry = list[list.length - 1];
            if (lastEntry) {
              loadPage(lastEntry.id);
            }
          }
        },
        { threshold: 0 },
      );

      observer.observe(el);
      return () => observer.disconnect();
    },
  );

  // Real-time history update events
  createEffect(
    () => undefined,
    () => {
      const unlisten = events.historyUpdatePayload.listen((event) => {
        const payload: HistoryUpdatePayload = event.payload;
        if (payload.action === "added") {
          setEntries((prev) => [payload.entry, ...prev]);
        } else if (payload.action === "updated") {
          setEntries((prev) =>
            prev.map((e) => (e.id === payload.entry.id ? payload.entry : e)),
          );
        } else if (payload.action === "cleared") {
          setEntries([]);
          setHasMore(false);
        }
      });

      return () => {
        unlisten.then((fn) => fn());
      };
    },
  );

  const toggleSaved = async (id: number) => {
    setEntries((prev) =>
      prev.map((e) => (e.id === id ? { ...e, saved: !e.saved } : e)),
    );
    try {
      const result = await commands.toggleHistoryEntrySaved(id);
      if (result.status !== "ok") {
        setEntries((prev) =>
          prev.map((e) => (e.id === id ? { ...e, saved: !e.saved } : e)),
        );
      }
    } catch (error) {
      console.error("Failed to toggle saved status:", error);
      setEntries((prev) =>
        prev.map((e) => (e.id === id ? { ...e, saved: !e.saved } : e)),
      );
    }
  };

  const getAudioUrl = async (fileName: string) => {
    try {
      const result = await commands.getAudioFilePath(fileName);
      if (result.status === "ok") {
        if (osType === "linux") {
          const fileData = await readFile(result.data);
          const blob = new Blob([fileData], { type: "audio/wav" });
          return URL.createObjectURL(blob);
        }
        return convertFileSrc(result.data, "asset");
      }
      return null;
    } catch (error) {
      console.error("Failed to get audio file path:", error);
      return null;
    }
  };

  // Optimistic removal; on failure the list is reloaded and the error is
  // rethrown so the row's handler can report it.
  const deleteAudioEntry = async (id: number) => {
    setEntries((prev) => prev.filter((e) => e.id !== id));
    let result;
    try {
      result = await commands.deleteHistoryEntry(id);
    } catch (error) {
      void loadPage();
      throw error;
    }
    if (result.status !== "ok") {
      void loadPage();
      throw new Error(String(result.error));
    }
  };

  const handleDeleteAllRecordings = async () => {
    try {
      setIsDeleting(true);
      const result = await commands.deleteAllRecordings();
      if (result.status === "ok") {
        setEntries([]);
        setHasMore(false);
        setShowDeleteConfirm(false);
        toast.success(t("settings.history.deleteRecordingsSuccess"));
      } else {
        toast.error(t("settings.history.deleteRecordingsError"));
      }
    } catch (error) {
      console.error("Failed to delete recordings:", error);
      toast.error(t("settings.history.deleteRecordingsError"));
    } finally {
      setIsDeleting(false);
    }
  };

  const filteredEntries = createMemo(() => {
    return entries().filter((entry) => {
      // Filter tab check
      if (activeFilter() === "saved" && !entry.saved) return false;
      if (
        activeFilter() === "multi_stt" &&
        entry.mode !== "multi_stt" &&
        !isMultiRecordingFileName(entry.file_name)
      ) {
        return false;
      }
      if (activeFilter() === "post_processed" && !entry.post_processed_text) {
        return false;
      }

      // Search query check
      const q = searchQuery().trim().toLowerCase();
      if (!q) return true;
      const textMatch = entry.transcription_text.toLowerCase().includes(q);
      const postMatch =
        entry.post_processed_text?.toLowerCase().includes(q) ?? false;
      const modelMatch = entry.model_id?.toLowerCase().includes(q) ?? false;
      const titleMatch = entry.title.toLowerCase().includes(q);

      return textMatch || postMatch || modelMatch || titleMatch;
    });
  });

  // Which of the four bodies is shown. A memo of a string, so the list branch
  // is built once and its `<For>` keeps its rows (and a playing recording)
  // across entry updates; returning fresh JSX from a plain accessor rebuilt
  // every row on any change to `entries()`.
  const view = createMemo((): "loading" | "empty" | "noMatch" | "list" => {
    if (loading() && entries().length === 0) return "loading";
    if (entries().length === 0) return "empty";
    if (filteredEntries().length === 0) return "noMatch";
    return "list";
  });

  return (
    <div class="max-w-3xl w-full mx-auto space-y-5">
      <div class="space-y-3">
        {/* Header toolbar */}
        <div class="px-1 flex flex-col sm:flex-row gap-3 sm:items-center justify-between">
          <div>
            <h2 class="text-xs font-semibold text-mid-gray uppercase tracking-wider">
              {t("settings.history.title")}
            </h2>
          </div>
          <div class="flex items-center gap-2">
            {/* Gated on load/delete state only, never on `entries()`. The
                button clears the recordings directory and the history table on
                disk, which this paginated list is not a faithful proxy for:
                audio files outlive their rows, so an empty list does not mean
                an empty folder. Gating on the list greyed the button out in
                exactly the case that needed it, and `handleDeleteAllRecordings`
                itself sets hasMore(false), so one successful delete disabled it
                permanently. The confirmation dialog is the guard. */}
            <DeleteRecordingsButton
              onClick={() => setShowDeleteConfirm(true)}
              label={t("settings.history.deleteRecordings")}
              disabled={loading() || isDeleting()}
            />
            <OpenRecordingsButton
              onClick={openRecordingsFolder}
              label={t("settings.history.openFolder")}
            />
          </div>
        </div>

        {/* Search & Filter Bar */}
        <div class="flex flex-col sm:flex-row gap-2.5 items-stretch sm:items-center justify-between bg-card/60 border border-mid-gray/20 p-2 rounded-lg">
          {/* Search Input */}
          <div class="relative flex-1">
            <Search class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-mid-gray/60" />
            <input
              type="text"
              value={searchQuery()}
              onInput={(e) => setSearchQuery(e.currentTarget.value)}
              placeholder={t("settings.history.searchPlaceholder")}
              class="w-full pl-9 pr-3 py-1.5 text-xs rounded-md bg-background border border-mid-gray/30 text-text placeholder:text-text/40 focus:outline-none focus:border-accent/60 transition-colors"
            />
          </div>

          {/* Filter Pills */}
          <div class="flex items-center gap-1 overflow-x-auto pb-1 sm:pb-0">
            <For
              each={
                [
                  { id: "all", label: t("settings.history.filterAll") },
                  {
                    id: "saved",
                    label: `★ ${t("settings.history.filterSaved")}`,
                  },
                  {
                    id: "multi_stt",
                    label: `🎦 ${t("settings.history.filterMultiStt")}`,
                  },
                  {
                    id: "post_processed",
                    label: `✨ ${t("settings.history.filterPolished")}`,
                  },
                ] as const
              }
            >
              {(filter) => (
                <button
                  onClick={() => setActiveFilter(filter.id)}
                  class={`px-2.5 py-1 text-xs font-medium rounded-md whitespace-nowrap transition-colors cursor-pointer ${
                    activeFilter() === filter.id
                      ? "bg-accent text-white"
                      : "bg-background/80 text-text/70 hover:text-text hover:bg-mid-gray/20 border border-mid-gray/20"
                  }`}
                >
                  {filter.label}
                </button>
              )}
            </For>
          </div>
        </div>

        {/* Entries Container */}
        <div class="bg-background border border-mid-gray/20 rounded-lg overflow-visible shadow-sm">
          <Switch>
            <Match when={view() === "loading"}>
              <div class="px-4 py-8 text-center text-text/60">
                <div class="inline-block animate-spin rounded-full h-6 w-6 border-b-2 border-accent mb-2" />
                <p class="text-sm">{t("settings.history.loading")}</p>
              </div>
            </Match>
            <Match when={view() === "empty"}>
              <div class="px-4 py-12 text-center text-text/60">
                <FileText class="w-10 h-10 mx-auto mb-3 opacity-40 text-mid-gray" />
                <p class="text-sm font-medium">{t("settings.history.empty")}</p>
              </div>
            </Match>
            <Match when={view() === "noMatch"}>
              <div class="px-4 py-10 text-center text-text/60">
                <p class="text-sm font-medium">
                  {t("settings.history.noMatch")}
                </p>
              </div>
            </Match>
            <Match when={view() === "list"}>
              <AudioPlayerGroup>
                <div class="divide-y divide-mid-gray/20">
                  <For each={filteredEntries()}>
                    {(entry) => (
                      <HistoryEntryComponent
                        entry={entry}
                        onToggleSaved={() => toggleSaved(entry.id)}
                        onCopyText={copyToClipboard}
                        getAudioUrl={getAudioUrl}
                        deleteAudio={deleteAudioEntry}
                        retryTranscription={retryHistoryEntry}
                        postProcessTranscription={postProcessHistoryEntry}
                        multiSttTranscription={multiSttHistoryEntry}
                      />
                    )}
                  </For>
                </div>
              </AudioPlayerGroup>
              <div ref={setSentinel} class="h-1" />
            </Match>
          </Switch>
        </div>
      </div>

      {/* Delete confirmation dialog */}
      <Dialog
        open={showDeleteConfirm()}
        title={t("settings.history.deleteRecordingsConfirmTitle")}
        description={t("settings.history.deleteRecordingsConfirmMessage")}
        closeLabel={t("common.cancel")}
        initialFocusRef={initialFocusRef}
        onOpenChange={setShowDeleteConfirm}
        footer={
          <>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => setShowDeleteConfirm(false)}
              disabled={isDeleting()}
            >
              {t("common.cancel")}
            </Button>
            <Button
              variant="danger"
              size="sm"
              onClick={handleDeleteAllRecordings}
              disabled={isDeleting()}
              class="flex items-center gap-2"
            >
              <Trash2 class="w-4 h-4" />
              <span>
                {isDeleting()
                  ? t("settings.history.deleting")
                  : t("settings.history.deleteRecordingsConfirmButton")}
              </span>
            </Button>
          </>
        }
      >
        {/* The message is the dialog's `description` (which also wires
            aria-describedby); repeating it here showed it twice. */}
        {null}
      </Dialog>
    </div>
  );
};

interface MultiSttHistoryModel {
  slot: number;
  model_id: string;
  text: string;
}

interface MultiSttHistoryBrain {
  provider_id: string;
  provider_label: string;
  model_name: string;
  prompt_name?: string | null;
  latency_ms?: number | null;
  raw_output: string;
  cleaned_output: string;
}

interface MultiSttHistoryMeta {
  version: number;
  models: MultiSttHistoryModel[];
  brain?: MultiSttHistoryBrain | null;
  final_merged_text: string;
}

const MULTI_STT_METADATA_RE = /\n?<!--MULTI_STT_METADATA:(.+?)-->$/s;

const parseMultiSttMeta = (text: string): MultiSttHistoryMeta | null => {
  const match = text.match(MULTI_STT_METADATA_RE);
  if (!match) return null;
  try {
    return JSON.parse(match[1]) as MultiSttHistoryMeta;
  } catch {
    return null;
  }
};

/** A Multi-STT entry's raw log: its text without the trailing metadata block. */
const stripMultiSttMetadata = (text: string): string =>
  text.replace(MULTI_STT_METADATA_RE, "");

interface HistoryEntryProps {
  entry: HistoryEntry;
  onToggleSaved: () => void;
  onCopyText: (text: string) => Promise<boolean>;
  getAudioUrl: (fileName: string) => Promise<string | null>;
  deleteAudio: (id: number) => Promise<void>;
  retryTranscription: (id: number) => Promise<void>;
  postProcessTranscription: (id: number) => Promise<void>;
  multiSttTranscription: (id: number) => Promise<void>;
}

// The destructured `entry` (and the body-level values derived from it) is a
// mount-time snapshot on purpose: the list's `<For>` keys rows by reference,
// and every history update replaces the entry object, so an updated entry
// remounts its row. The other props are stable callbacks.
const HistoryEntryComponent = ({
  entry,
  onToggleSaved,
  onCopyText,
  getAudioUrl,
  deleteAudio,
  retryTranscription,
  postProcessTranscription,
  multiSttTranscription,
}: HistoryEntryProps) => {
  const { t, i18n } = useTranslation();
  const [showCopied, setShowCopied] = createSignal(false);
  const [retrying, setRetrying] = createSignal<
    "standard" | "post_process" | "multi_stt" | null
  >(null);
  const hasTranscription = entry.transcription_text.trim().length > 0;
  const isMultiStt =
    entry.mode === "multi_stt" || isMultiRecordingFileName(entry.file_name);

  const multiSttMeta = createMemo(() =>
    isMultiStt ? parseMultiSttMeta(entry.transcription_text) : null,
  );
  // The transcript as stored, minus a Multi-STT entry's metadata block.
  const rawText = isMultiStt
    ? stripMultiSttMetadata(entry.transcription_text)
    : entry.transcription_text;

  // The details tab only has something to show when the metadata parsed; an
  // entry without it (older or truncated) opens on its text instead.
  const [activeTab, setActiveTab] = createSignal<
    "polished" | "raw" | "multi_stt_details"
  >(
    untrack(multiSttMeta)
      ? "multi_stt_details"
      : entry.post_processed_text
        ? "polished"
        : "raw",
  );
  const [showPromptDetails, setShowPromptDetails] = createSignal(false);
  const [showBrainRaw, setShowBrainRaw] = createSignal(false);

  const multiSttCleanedText = createMemo(() => {
    if (!isMultiStt) return null;
    if (entry.post_processed_text) return entry.post_processed_text;
    const meta = multiSttMeta();
    if (meta) return meta.final_merged_text;
    return null;
  });

  const handleLoadAudio = () => getAudioUrl(entry.file_name);

  // What the active tab shows (and what copy and Save to Recall take). The
  // Raw tab is always the stored transcript, a Multi-STT entry's included.
  const textToDisplay = createMemo(() => {
    if (activeTab() === "raw") return rawText;
    const cleaned = multiSttCleanedText();
    if (isMultiStt && cleaned) {
      return cleaned;
    }
    if (activeTab() === "polished" && entry.post_processed_text) {
      return entry.post_processed_text;
    }
    return rawText;
  });

  const handleCopyText = async () => {
    const text = textToDisplay();
    if (!text || !text.trim()) return;
    const copied = await onCopyText(text);
    if (!copied) {
      toast.error(t("settings.history.copyError"));
      return;
    }
    setShowCopied(true);
    setTimeout(() => setShowCopied(false), 2000);
  };

  // File the displayed text (polished when that tab is active) as a new note
  // in the Recall vault; the recording stays where it is and is referenced
  // by file name, never copied.
  const handleSaveToRecall = async () => {
    const text = textToDisplay();
    if (!text || !text.trim()) return;
    const result = await commands.recallSaveTranscription(
      text,
      entry.title || null,
      entry.model_id ?? null,
      entry.file_name || null,
      [],
    );
    if (result.status === "error") {
      toast.error(result.error);
      return;
    }
    toast.success(t("settings.history.savedToRecall"));
  };

  const handleDeleteEntry = async () => {
    try {
      await deleteAudio(entry.id);
    } catch (error) {
      console.error("Failed to delete entry:", error);
      toast.error(t("settings.history.deleteError"));
    }
  };

  const handleRetranscribe = async () => {
    try {
      setRetrying("standard");
      await retryTranscription(entry.id);
      toast.success(t("settings.history.retranscribeSuccess"));
    } catch (error) {
      console.error("Failed to re-transcribe:", error);
      toast.error(t("settings.history.retranscribeError"));
    } finally {
      setRetrying(null);
    }
  };

  const handlePostProcess = async () => {
    try {
      setRetrying("post_process");
      await postProcessTranscription(entry.id);
      setActiveTab("polished");
      toast.success(t("settings.history.postProcessSuccess"));
    } catch (error) {
      console.error("Failed to post-process:", error);
      toast.error(t("settings.history.postProcessError"), {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setRetrying(null);
    }
  };

  const handleMultiStt = async () => {
    try {
      setRetrying("multi_stt");
      await multiSttTranscription(entry.id);
      setActiveTab("polished");
      toast.success(t("settings.history.multiSttSuccess"));
    } catch (error) {
      console.error("Failed to Multi-STT re-transcribe:", error);
      toast.error(t("settings.history.multiSttError"), {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setRetrying(null);
    }
  };

  const [loadedAudioDurationSec, setLoadedAudioDurationSec] = createSignal<
    number | null
  >(null);

  createEffect(
    () => undefined,
    () => {
      if (entry.audio_duration_ms == null && entry.file_name) {
        getAudioUrl(entry.file_name).then((url) => {
          if (!url) return;
          const audio = new Audio(url);
          // On Linux the URL is an object URL over the whole WAV; release it
          // once the metadata (or an error) is in.
          const release = () => {
            if (url.startsWith("blob:")) URL.revokeObjectURL(url);
          };
          audio.addEventListener(
            "loadedmetadata",
            () => {
              if (
                audio.duration &&
                !isNaN(audio.duration) &&
                isFinite(audio.duration)
              ) {
                setLoadedAudioDurationSec(audio.duration);
              }
              release();
            },
            { once: true },
          );
          audio.addEventListener("error", release, { once: true });
        });
      }
    },
  );

  // Metric calculations. `totalDurationSec` is an accessor: without a stored
  // duration it waits for the metadata load above.
  const totalDurationSec = () =>
    entry.audio_duration_ms
      ? entry.audio_duration_ms / 1000
      : loadedAudioDurationSec();
  const speechDurationSec = entry.speech_duration_ms
    ? entry.speech_duration_ms / 1000
    : null;

  const silenceCutPercent = createMemo(() => {
    const total = totalDurationSec();
    if (total && speechDurationSec && total > speechDurationSec) {
      return Math.max(
        0,
        Math.round(((total - speechDurationSec) / total) * 100),
      );
    }
    return 0;
  });

  const wordCount = createMemo(() => {
    if (entry.word_count !== null && entry.word_count !== undefined) {
      return entry.word_count;
    }
    const text = entry.post_processed_text || entry.transcription_text;
    return text.trim() ? text.trim().split(/\s+/).length : 0;
  });

  const metrics = createMemo(() => {
    // Always calculate WPM from the silence-removed speech duration
    const total = totalDurationSec();
    const silenceRemovedSec =
      speechDurationSec && speechDurationSec > 0
        ? speechDurationSec
        : total && total > 0
          ? total
          : 0;

    if (silenceRemovedSec <= 0 || wordCount() <= 0) {
      return { wpm: 0, speedRating: null };
    }
    const calculatedWpm = Math.round((wordCount() / silenceRemovedSec) * 60);

    let rating = {
      label: t("settings.history.speedConversational"),
      color: "bg-blue-500/10 text-blue-400 border-blue-500/20",
    };
    if (calculatedWpm < 110) {
      rating = {
        label: t("settings.history.speedSlow"),
        color: "bg-gray-500/10 text-gray-400 border-gray-500/20",
      };
    } else if (calculatedWpm > 210) {
      rating = {
        label: t("settings.history.speedVeryFast"),
        color: "bg-amber-500/10 text-amber-400 border-amber-500/20",
      };
    } else if (calculatedWpm > 160) {
      rating = {
        label: t("settings.history.speedFast"),
        color: "bg-emerald-500/10 text-emerald-400 border-emerald-500/20",
      };
    }

    return { wpm: calculatedWpm, speedRating: rating };
  });

  const wpm = () => metrics().wpm;
  const speedRating = () => metrics().speedRating;

  const formattedDate = formatDateTime(String(entry.timestamp), i18n.language);

  const formatLatency = (ms: number) =>
    ms >= 1000
      ? t("settings.statistics.units.seconds", {
          value: (ms / 1000).toFixed(1),
        })
      : t("settings.statistics.units.milliseconds", { value: Math.round(ms) });

  return (
    <div class="p-4 flex flex-col gap-3 hover:bg-card/30 transition-colors">
      {/* Header Row: Title & Action Toolbar */}
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div class="flex items-center gap-2 flex-wrap">
          <p class="text-xs font-semibold text-text">{formattedDate}</p>

          {/* Model Badge */}
          {entry.model_id && (
            <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] font-medium bg-mid-gray/15 text-text/80 border border-mid-gray/20">
              <Cpu class="w-3 h-3 text-accent" />
              <span>
                {isMultiStt
                  ? t("settings.history.multiSttModelsBadge", {
                      count: (entry.extra_models?.length ?? 0) + 1,
                    })
                  : displayModelId(entry.model_id)}
              </span>
            </span>
          )}

          {/* Mode Pill */}
          {entry.mode && entry.mode !== "single" && (
            <span
              class={`px-1.5 py-0.5 rounded text-[10px] font-semibold uppercase tracking-wider ${
                isMultiStt
                  ? "bg-purple-500/10 text-purple-400 border border-purple-500/20"
                  : "bg-teal-500/10 text-teal-400 border border-teal-500/20"
              }`}
            >
              {entry.mode === "multi_stt"
                ? t("settings.history.filterMultiStt")
                : entry.mode}
            </span>
          )}
        </div>

        {/* Action Buttons */}
        <div class="flex items-center gap-0.5 bg-background/80 border border-mid-gray/20 p-0.5 rounded-lg">
          <IconButton
            onClick={handleCopyText}
            disabled={!hasTranscription || retrying() !== null}
            title={t("settings.history.copyToClipboard")}
          >
            {showCopied() ? (
              <Check width={15} height={15} class="text-green-400" />
            ) : (
              <Copy width={15} height={15} />
            )}
          </IconButton>

          <IconButton
            onClick={onToggleSaved}
            disabled={retrying() !== null}
            active={entry.saved}
            title={
              entry.saved
                ? t("settings.history.unsave")
                : t("settings.history.save")
            }
          >
            <Star
              width={15}
              height={15}
              fill={entry.saved ? "currentColor" : "none"}
            />
          </IconButton>

          {/* Re-transcribe with Primary Model */}
          <IconButton
            onClick={handleRetranscribe}
            disabled={retrying() !== null}
            title={t("settings.history.retranscribe")}
          >
            <RotateCcw
              width={15}
              height={15}
              class={
                retrying() === "standard" ? "animate-spin text-accent" : ""
              }
            />
          </IconButton>

          {/* Post-Process / Polish with LLM */}
          <IconButton
            onClick={handlePostProcess}
            disabled={retrying() !== null || !hasTranscription}
            title={t("settings.history.postProcess")}
          >
            <Sparkles
              width={15}
              height={15}
              class={
                retrying() === "post_process"
                  ? "animate-pulse text-amber-400"
                  : ""
              }
            />
          </IconButton>

          {/* Multi-STT Parallel Re-transcribe */}
          <IconButton
            onClick={handleMultiStt}
            disabled={retrying() !== null}
            title={t("settings.history.multiSttRetranscribe")}
          >
            <Layers
              width={15}
              height={15}
              class={
                retrying() === "multi_stt"
                  ? "animate-pulse text-purple-400"
                  : ""
              }
            />
          </IconButton>

          {/* File as a note in the Recall vault */}
          <IconButton
            onClick={handleSaveToRecall}
            disabled={retrying() !== null || !hasTranscription}
            title={t("settings.history.saveToRecall")}
          >
            <FilePlus width={15} height={15} />
          </IconButton>

          <IconButton
            onClick={handleDeleteEntry}
            disabled={retrying() !== null}
            title={t("settings.history.delete")}
            class="hover:text-red-400 hover:bg-red-500/10"
          >
            <Trash2 width={15} height={15} />
          </IconButton>
        </div>
      </div>

      {/* Intelligence & Metrics Bar */}
      <div class="flex flex-wrap items-center gap-2 text-xs">
        {/* Audio Duration & Silence Suppression */}
        <Show when={totalDurationSec()}>
          {(total) => (
            <span class="inline-flex items-center gap-1.5 px-2 py-0.5 rounded bg-mid-gray/10 text-text/80 border border-mid-gray/15">
              <Volume2 class="w-3.5 h-3.5 text-mid-gray" />
              <span>
                {speechDurationSec !== null
                  ? silenceCutPercent() > 0
                    ? t("settings.history.audioDetails", {
                        total: formatDurationSec(total()),
                        speech: formatDurationSec(speechDurationSec),
                        silence: `${silenceCutPercent()}%`,
                      })
                    : t("settings.history.audioDetailsNoCut", {
                        total: formatDurationSec(total()),
                        speech: formatDurationSec(speechDurationSec),
                      })
                  : t("settings.history.audioTotalOnly", {
                      total: formatDurationSec(total()),
                    })}
              </span>
            </span>
          )}
        </Show>

        {/* Word Count */}
        {wordCount() > 0 && (
          <span class="inline-flex items-center gap-1.5 px-2 py-0.5 rounded bg-mid-gray/10 text-text/80 border border-mid-gray/15">
            <FileText class="w-3.5 h-3.5 text-mid-gray" />
            <span>
              {t("settings.history.wordCount", { count: wordCount() })}
            </span>
          </span>
        )}

        {/* WPM Speech Rate */}
        {wpm() > 0 && (
          <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded bg-mid-gray/10 text-text/80 border border-mid-gray/15 font-medium">
            <Zap class="w-3 h-3 text-amber-400" />
            <span>{t("settings.history.wpmRating", { wpm: wpm() })}</span>
          </span>
        )}

        {/* WPM Speed Rating Badge */}
        <Show when={wpm() > 0 ? speedRating() : null}>
          {(rating) => (
            <span
              class={`px-2 py-0.5 rounded text-[11px] font-semibold border ${rating().color}`}
            >
              {rating().label}
            </span>
          )}
        </Show>

        {/* STT Latency */}
        {entry.transcription_latency_ms !== null &&
          entry.transcription_latency_ms !== undefined && (
            <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded bg-blue-500/10 text-blue-400 border border-blue-500/20 text-[11px] font-medium">
              <Zap class="w-3 h-3" />
              <span>
                {t("settings.history.latencyStt", {
                  duration: formatLatency(entry.transcription_latency_ms),
                })}
              </span>
            </span>
          )}

        {/* LLM Post-Processing Latency */}
        {entry.post_processing_latency_ms !== null &&
          entry.post_processing_latency_ms !== undefined && (
            <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded bg-amber-500/10 text-amber-400 border border-amber-500/20 text-[11px] font-medium">
              <Sparkles class="w-3 h-3" />
              <span>
                {t("settings.history.latencyLlm", {
                  duration: formatLatency(entry.post_processing_latency_ms),
                })}
              </span>
            </span>
          )}

        {/* Audio Format */}
        {entry.sample_rate_hz && (
          <span class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-mid-gray/10 text-text/60 text-[10px]">
            <span>{`${Math.round(entry.sample_rate_hz / 1000)} kHz`}</span>
          </span>
        )}
      </div>

      {/* View Switcher Tabs */}
      {(entry.post_processed_text || isMultiStt) && (
        <div class="flex items-center gap-1.5 pt-0.5 flex-wrap">
          {/* For Multi-STT: show Details / Cleaned / Raw tabs */}
          {isMultiStt && (
            <>
              <button
                onClick={() => setActiveTab("multi_stt_details")}
                class={`px-2.5 py-1 text-xs font-medium rounded-md transition-colors flex items-center gap-1.5 cursor-pointer ${
                  activeTab() === "multi_stt_details"
                    ? "bg-purple-500/20 text-purple-300 border border-purple-500/40"
                    : "bg-mid-gray/10 text-text/60 hover:text-text"
                }`}
              >
                <Layers class="w-3 h-3" />
                <span>{t("settings.history.multiSttDetailsTab")}</span>
              </button>
              <button
                onClick={() => setActiveTab("polished")}
                class={`px-2.5 py-1 text-xs font-medium rounded-md transition-colors flex items-center gap-1.5 cursor-pointer ${
                  activeTab() === "polished"
                    ? "bg-accent/20 text-accent border border-accent/40"
                    : "bg-mid-gray/10 text-text/60 hover:text-text"
                }`}
              >
                <Sparkles class="w-3 h-3" />
                <span>{t("settings.history.multiSttCleanedOutputTab")}</span>
              </button>
              <button
                onClick={() => setActiveTab("raw")}
                class={`px-2.5 py-1 text-xs font-medium rounded-md transition-colors flex items-center gap-1.5 cursor-pointer ${
                  activeTab() === "raw"
                    ? "bg-accent/20 text-accent border border-accent/40"
                    : "bg-mid-gray/10 text-text/60 hover:text-text"
                }`}
              >
                <FileText class="w-3 h-3" />
                <span>{t("settings.history.multiSttRawLogTab")}</span>
              </button>
            </>
          )}
          {/* Non-multi-STT tabs */}
          {!isMultiStt && entry.post_processed_text && (
            <>
              <button
                onClick={() => setActiveTab("polished")}
                class={`px-2.5 py-1 text-xs font-medium rounded-md transition-colors flex items-center gap-1.5 cursor-pointer ${
                  activeTab() === "polished"
                    ? "bg-accent/20 text-accent border border-accent/40"
                    : "bg-mid-gray/10 text-text/60 hover:text-text"
                }`}
              >
                <Sparkles class="w-3 h-3" />
                <span>{t("settings.history.polishedTextTab")}</span>
              </button>
              <button
                onClick={() => setActiveTab("raw")}
                class={`px-2.5 py-1 text-xs font-medium rounded-md transition-colors flex items-center gap-1.5 cursor-pointer ${
                  activeTab() === "raw"
                    ? "bg-accent/20 text-accent border border-accent/40"
                    : "bg-mid-gray/10 text-text/60 hover:text-text"
                }`}
              >
                <FileText class="w-3 h-3" />
                <span>{t("settings.history.rawTranscriptTab")}</span>
              </button>
            </>
          )}
        </div>
      )}

      {/* Multi-STT Details Panel */}
      <Show
        when={
          isMultiStt && activeTab() === "multi_stt_details"
            ? multiSttMeta()
            : null
        }
      >
        {(meta) => (
          <div class="space-y-3">
            {/* 4 Model Cards */}
            <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
              <For each={meta().models}>
                {(m) => (
                  <div class="rounded-lg border border-mid-gray/20 bg-card/40 p-3 flex flex-col gap-2">
                    <div class="flex items-center justify-between gap-2">
                      <div class="flex items-center gap-1.5 min-w-0">
                        <span class="px-1.5 py-0.5 rounded text-[10px] font-bold bg-purple-500/15 text-purple-300 border border-purple-500/25 shrink-0">
                          {t("settings.history.multiSttModelSlot", {
                            slot: m.slot,
                          })}
                        </span>
                        <span
                          class="text-[11px] font-mono text-text/70 truncate"
                          title={m.model_id}
                        >
                          {displayModelId(m.model_id)}
                        </span>
                      </div>
                      <button
                        onClick={() => {
                          if (m.text.trim()) onCopyText(m.text);
                        }}
                        disabled={!m.text.trim()}
                        title={t("settings.history.multiSttCopyModelOutput")}
                        class="p-1 rounded text-text/40 hover:text-accent hover:bg-mid-gray/10 transition-colors disabled:opacity-30 shrink-0"
                      >
                        <Copy class="w-3 h-3" />
                      </button>
                    </div>
                    <p
                      class={`text-xs select-text cursor-text whitespace-pre-wrap break-words rounded p-2 bg-background/60 border border-mid-gray/15 min-h-[2.5rem] ${
                        m.text.trim() ? "text-text/90" : "text-text/35 italic"
                      }`}
                    >
                      {m.text.trim() || t("settings.history.multiSttNoOutput")}
                    </p>
                  </div>
                )}
              </For>
            </div>

            {/* Brain LLM Card */}
            <Show when={meta().brain}>
              {(brain) => (
                <div class="rounded-lg border border-amber-500/30 bg-amber-500/5 p-3 flex flex-col gap-2">
                  <div class="flex items-center justify-between gap-2 flex-wrap">
                    <div class="flex items-center gap-1.5 flex-wrap">
                      <span class="px-1.5 py-0.5 rounded text-[10px] font-bold bg-amber-500/20 text-amber-300 border border-amber-500/30">
                        {t("settings.history.multiSttBrainLabel")}
                      </span>
                      <span class="text-[11px] font-mono text-text/75 font-medium">
                        {brain().model_name}
                      </span>
                      <span class="text-[10px] text-text/50 bg-mid-gray/10 px-1.5 py-0.5 rounded border border-mid-gray/20">
                        {brain().provider_label} ({brain().provider_id})
                      </span>
                      {brain().prompt_name && (
                        <span class="text-[10px] text-text/50 bg-mid-gray/10 px-1.5 py-0.5 rounded border border-mid-gray/20">
                          {t("settings.history.multiSttBrainPromptLabel", {
                            name: brain().prompt_name,
                          })}
                        </span>
                      )}
                      {brain().latency_ms != null && (
                        <span class="text-[10px] text-amber-400 bg-amber-500/10 px-1.5 py-0.5 rounded border border-amber-500/20">
                          {t("settings.history.latencyLlm", {
                            duration: formatLatency(brain().latency_ms ?? 0),
                          })}
                        </span>
                      )}
                    </div>
                    <button
                      onClick={() => {
                        if (brain().cleaned_output.trim())
                          onCopyText(brain().cleaned_output);
                      }}
                      disabled={!brain().cleaned_output.trim()}
                      title={t("settings.history.multiSttCopyBrainOutput")}
                      class="p-1 rounded text-text/40 hover:text-accent hover:bg-mid-gray/10 transition-colors disabled:opacity-30 shrink-0"
                    >
                      <Copy class="w-3 h-3" />
                    </button>
                  </div>

                  {/* Cleaned output */}
                  <div>
                    <p class="text-[10px] text-text/50 mb-1 font-medium uppercase tracking-wider">
                      {t("settings.history.multiSttBrainCleanedOutput")}
                    </p>
                    <p class="text-xs select-text cursor-text whitespace-pre-wrap break-words rounded p-2 bg-background/60 border border-amber-500/15 text-text/90">
                      {brain().cleaned_output ||
                        t("settings.history.multiSttNoOutput")}
                    </p>
                  </div>

                  {/* Raw output toggle */}
                  {brain().raw_output && (
                    <div>
                      <button
                        onClick={() => setShowBrainRaw((v) => !v)}
                        class="text-[10px] text-text/50 hover:text-amber-400 flex items-center gap-1 transition-colors font-medium uppercase tracking-wider"
                      >
                        {showBrainRaw() ? (
                          <ChevronUp class="w-3 h-3" />
                        ) : (
                          <ChevronDown class="w-3 h-3" />
                        )}
                        {t("settings.history.multiSttBrainRawToggle")}
                      </button>
                      {showBrainRaw() && (
                        <pre class="mt-1 p-2 rounded bg-background/90 text-[10px] text-text/65 border border-amber-500/20 whitespace-pre-wrap font-mono overflow-x-auto max-h-56 overflow-y-auto">
                          {brain().raw_output}
                        </pre>
                      )}
                    </div>
                  )}
                </div>
              )}
            </Show>
          </div>
        )}
      </Show>

      {/* Transcript Text Display (non-details tabs) */}
      {activeTab() !== "multi_stt_details" && (
        <div class="relative">
          <p
            class={`text-sm select-text cursor-text whitespace-pre-wrap break-words rounded-md p-2.5 bg-card/40 border border-mid-gray/15 ${
              retrying() !== null
                ? "text-text/40 animate-pulse"
                : hasTranscription
                  ? "text-text/95"
                  : "text-text/40 italic"
            }`}
          >
            {retrying() === "standard"
              ? t("settings.history.transcribing")
              : retrying() === "post_process"
                ? t("settings.history.postProcessing")
                : retrying() === "multi_stt"
                  ? t("settings.history.multiSttTranscribing")
                  : hasTranscription
                    ? textToDisplay()
                    : t("settings.history.transcriptionFailed")}
          </p>

          {/* Expandable prompt details */}
          {entry.post_process_prompt && (
            <div class="mt-1">
              <button
                onClick={() => setShowPromptDetails(!showPromptDetails())}
                class="text-[11px] text-text/50 hover:text-accent flex items-center gap-1 transition-colors"
              >
                {showPromptDetails() ? (
                  <ChevronUp class="w-3 h-3" />
                ) : (
                  <ChevronDown class="w-3 h-3" />
                )}
                <span>{t("settings.history.promptUsed")}</span>
              </button>
              {showPromptDetails() && (
                <pre class="mt-1 p-2 rounded bg-background/90 text-[11px] text-text/70 border border-mid-gray/20 whitespace-pre-wrap font-mono">
                  {entry.post_process_prompt}
                </pre>
              )}
            </div>
          )}
        </div>
      )}

      {/* Audio Waveform Player */}
      <AudioPlayer onLoadRequest={handleLoadAudio} class="w-full mt-1" />
    </div>
  );
};
