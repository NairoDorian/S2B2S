import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { readFile } from "@tauri-apps/plugin-fs";
import {
  Check,
  ChevronDown,
  ChevronUp,
  Copy,
  Cpu,
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
} from "lucide-react";
import { useTranslation } from "react-i18next";
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
import { Dialog } from "../../ui/Dialog";

const IconButton: React.FC<{
  onClick: () => void;
  title: string;
  disabled?: boolean;
  active?: boolean;
  className?: string;
  children: React.ReactNode;
}> = ({ onClick, title, disabled, active, className, children }) => (
  <button
    onClick={onClick}
    disabled={disabled}
    className={`p-1.5 rounded-md flex items-center justify-center transition-colors cursor-pointer disabled:cursor-not-allowed disabled:text-text/20 ${
      active
        ? "text-accent hover:text-accent/80 bg-accent/10"
        : "text-text/50 hover:text-accent hover:bg-mid-gray/10"
    } ${className ?? ""}`}
    title={title}
  >
    {children}
  </button>
);

const PAGE_SIZE = 30;

interface OpenRecordingsButtonProps {
  onClick: () => void;
  label: string;
}

const OpenRecordingsButton: React.FC<OpenRecordingsButtonProps> = ({
  onClick,
  label,
}) => (
  <Button
    onClick={onClick}
    variant="secondary"
    size="sm"
    className="flex items-center gap-2"
    title={label}
  >
    <FolderOpen className="w-4 h-4" />
    <span>{label}</span>
  </Button>
);

interface DeleteRecordingsButtonProps {
  onClick: () => void;
  label: string;
  disabled?: boolean;
}

const DeleteRecordingsButton: React.FC<DeleteRecordingsButtonProps> = ({
  onClick,
  label,
  disabled,
}) => (
  <Button
    onClick={onClick}
    variant="secondary"
    size="sm"
    className="flex items-center gap-2 text-red-400 hover:text-red-300 hover:border-red-500/40"
    title={label}
    disabled={disabled}
  >
    <Trash2 className="w-4 h-4" />
    <span>{label}</span>
  </Button>
);

type HistoryFilter = "all" | "saved" | "multi_stt" | "post_processed";

export const HistorySettings: React.FC = () => {
  const { t } = useTranslation();
  const osType = useOsType();
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [hasMore, setHasMore] = useState(true);
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [activeFilter, setActiveFilter] = useState<HistoryFilter>("all");

  const sentinelRef = useRef<HTMLDivElement>(null);
  const entriesRef = useRef<HistoryEntry[]>([]);
  const loadingRef = useRef(false);

  useEffect(() => {
    entriesRef.current = entries;
  }, [entries]);

  const loadPage = useCallback(async (cursor?: number) => {
    const isFirstPage = cursor === undefined;
    if (!isFirstPage && loadingRef.current) return;
    loadingRef.current = true;

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
      }
    } catch (error) {
      console.error("Failed to load history entries:", error);
    } finally {
      setLoading(false);
      loadingRef.current = false;
    }
  }, []);

  // Initial load
  useEffect(() => {
    loadPage();
  }, [loadPage]);

  // Infinite scroll
  useEffect(() => {
    if (loading) return;

    const sentinel = sentinelRef.current;
    if (!sentinel || !hasMore) return;

    const observer = new IntersectionObserver(
      (observerEntries) => {
        const first = observerEntries[0];
        if (first.isIntersecting) {
          const lastEntry = entriesRef.current[entriesRef.current.length - 1];
          if (lastEntry) {
            loadPage(lastEntry.id);
          }
        }
      },
      { threshold: 0 },
    );

    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [loading, hasMore, loadPage]);

  // Real-time history update events
  useEffect(() => {
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
  }, []);

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

  const copyToClipboard = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      toast.success(t("settings.history.copyToClipboard"));
    } catch (error) {
      console.error("Failed to copy to clipboard:", error);
    }
  };

  const getAudioUrl = useCallback(
    async (fileName: string) => {
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
    },
    [osType],
  );

  const deleteAudioEntry = async (id: number) => {
    setEntries((prev) => prev.filter((e) => e.id !== id));
    try {
      const result = await commands.deleteHistoryEntry(id);
      if (result.status !== "ok") {
        loadPage();
      }
    } catch (error) {
      console.error("Failed to delete entry:", error);
      loadPage();
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

  const filteredEntries = useMemo(() => {
    return entries.filter((entry) => {
      // Filter tab check
      if (activeFilter === "saved" && !entry.saved) return false;
      if (
        activeFilter === "multi_stt" &&
        entry.mode !== "multi_stt" &&
        !isMultiRecordingFileName(entry.file_name)
      ) {
        return false;
      }
      if (activeFilter === "post_processed" && !entry.post_processed_text) {
        return false;
      }

      // Search query check
      if (!searchQuery.trim()) return true;
      const q = searchQuery.toLowerCase();
      const textMatch = entry.transcription_text.toLowerCase().includes(q);
      const postMatch =
        entry.post_processed_text?.toLowerCase().includes(q) ?? false;
      const modelMatch = entry.model_id?.toLowerCase().includes(q) ?? false;
      const titleMatch = entry.title.toLowerCase().includes(q);

      return textMatch || postMatch || modelMatch || titleMatch;
    });
  }, [entries, activeFilter, searchQuery]);

  let content: React.ReactNode;

  if (loading && entries.length === 0) {
    content = (
      <div className="px-4 py-8 text-center text-text/60">
        <div className="inline-block animate-spin rounded-full h-6 w-6 border-b-2 border-accent mb-2" />
        <p className="text-sm">{t("settings.history.loading")}</p>
      </div>
    );
  } else if (entries.length === 0) {
    content = (
      <div className="px-4 py-12 text-center text-text/60">
        <FileText className="w-10 h-10 mx-auto mb-3 opacity-40 text-mid-gray" />
        <p className="text-sm font-medium">{t("settings.history.empty")}</p>
      </div>
    );
  } else if (filteredEntries.length === 0) {
    content = (
      <div className="px-4 py-10 text-center text-text/60">
        <p className="text-sm font-medium">{t("settings.history.noMatch")}</p>
      </div>
    );
  } else {
    content = (
      <>
        <AudioPlayerGroup>
          <div className="divide-y divide-mid-gray/20">
            {filteredEntries.map((entry) => (
              <HistoryEntryComponent
                key={entry.id}
                entry={entry}
                onToggleSaved={() => toggleSaved(entry.id)}
                onCopyText={copyToClipboard}
                getAudioUrl={getAudioUrl}
                deleteAudio={deleteAudioEntry}
                retryTranscription={retryHistoryEntry}
                postProcessTranscription={postProcessHistoryEntry}
                multiSttTranscription={multiSttHistoryEntry}
              />
            ))}
          </div>
        </AudioPlayerGroup>
        <div ref={sentinelRef} className="h-1" />
      </>
    );
  }

  return (
    <div className="max-w-3xl w-full mx-auto space-y-5">
      <div className="space-y-3">
        {/* Header toolbar */}
        <div className="px-1 flex flex-col sm:flex-row gap-3 sm:items-center justify-between">
          <div>
            <h2 className="text-xs font-semibold text-mid-gray uppercase tracking-wider">
              {t("settings.history.title")}
            </h2>
          </div>
          <div className="flex items-center gap-2">
            <DeleteRecordingsButton
              onClick={() => setShowDeleteConfirm(true)}
              label={t("settings.history.deleteRecordings")}
              disabled={loading || (entries.length === 0 && !hasMore)}
            />
            <OpenRecordingsButton
              onClick={openRecordingsFolder}
              label={t("settings.history.openFolder")}
            />
          </div>
        </div>

        {/* Search & Filter Bar */}
        <div className="flex flex-col sm:flex-row gap-2.5 items-stretch sm:items-center justify-between bg-card/60 border border-mid-gray/20 p-2 rounded-lg">
          {/* Search Input */}
          <div className="relative flex-1">
            <Search className="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-mid-gray/60" />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder={t("settings.history.searchPlaceholder")}
              className="w-full pl-9 pr-3 py-1.5 text-xs rounded-md bg-background border border-mid-gray/30 text-text placeholder:text-text/40 focus:outline-none focus:border-accent/60 transition-colors"
            />
          </div>

          {/* Filter Pills */}
          <div className="flex items-center gap-1 overflow-x-auto pb-1 sm:pb-0">
            {(
              [
                { id: "all", label: t("settings.history.filterAll") },
                {
                  id: "saved",
                  label: `⭐ ${t("settings.history.filterSaved")}`,
                },
                {
                  id: "multi_stt",
                  label: `🎙️ ${t("settings.history.filterMultiStt")}`,
                },
                {
                  id: "post_processed",
                  label: `✨ ${t("settings.history.filterPolished")}`,
                },
              ] as const
            ).map((filter) => (
              <button
                key={filter.id}
                onClick={() => setActiveFilter(filter.id)}
                className={`px-2.5 py-1 text-xs font-medium rounded-md whitespace-nowrap transition-colors cursor-pointer ${
                  activeFilter === filter.id
                    ? "bg-accent text-white"
                    : "bg-background/80 text-text/70 hover:text-text hover:bg-mid-gray/20 border border-mid-gray/20"
                }`}
              >
                {filter.label}
              </button>
            ))}
          </div>
        </div>

        {/* Entries Container */}
        <div className="bg-background border border-mid-gray/20 rounded-lg overflow-visible shadow-sm">
          {content}
        </div>
      </div>

      {/* Delete confirmation dialog */}
      <Dialog
        open={showDeleteConfirm}
        title={t("settings.history.deleteRecordingsConfirmTitle")}
        description={t("settings.history.deleteRecordingsConfirmMessage")}
        closeLabel={t("common.cancel")}
        onOpenChange={setShowDeleteConfirm}
        footer={
          <>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => setShowDeleteConfirm(false)}
              disabled={isDeleting}
            >
              {t("common.cancel")}
            </Button>
            <Button
              variant="danger"
              size="sm"
              onClick={handleDeleteAllRecordings}
              disabled={isDeleting}
              className="flex items-center gap-2"
            >
              <Trash2 className="w-4 h-4" />
              <span>
                {isDeleting
                  ? t("settings.history.deleting")
                  : t("settings.history.deleteRecordingsConfirmButton")}
              </span>
            </Button>
          </>
        }
      >
        <p className="text-sm text-text/80">
          {t("settings.history.deleteRecordingsConfirmMessage")}
        </p>
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

interface HistoryEntryProps {
  entry: HistoryEntry;
  onToggleSaved: () => void;
  onCopyText: (text: string) => Promise<void>;
  getAudioUrl: (fileName: string) => Promise<string | null>;
  deleteAudio: (id: number) => Promise<void>;
  retryTranscription: (id: number) => Promise<void>;
  postProcessTranscription: (id: number) => Promise<void>;
  multiSttTranscription: (id: number) => Promise<void>;
}

const HistoryEntryComponent: React.FC<HistoryEntryProps> = ({
  entry,
  onToggleSaved,
  onCopyText,
  getAudioUrl,
  deleteAudio,
  retryTranscription,
  postProcessTranscription,
  multiSttTranscription,
}) => {
  const { t, i18n } = useTranslation();
  const [showCopied, setShowCopied] = useState(false);
  const [retrying, setRetrying] = useState<
    "standard" | "post_process" | "multi_stt" | null
  >(null);
  const [activeTab, setActiveTab] = useState<
    "polished" | "raw" | "multi_stt_details"
  >(
    entry.mode === "multi_stt" || isMultiRecordingFileName(entry.file_name)
      ? "multi_stt_details"
      : entry.post_processed_text
        ? "polished"
        : "raw",
  );
  const [showPromptDetails, setShowPromptDetails] = useState(false);
  const [showBrainRaw, setShowBrainRaw] = useState(false);

  const hasTranscription = entry.transcription_text.trim().length > 0;
  const isMultiStt =
    entry.mode === "multi_stt" || isMultiRecordingFileName(entry.file_name);

  const multiSttMeta = useMemo(() => {
    if (!isMultiStt) return null;
    const raw = entry.transcription_text;
    const match = raw.match(/<!--MULTI_STT_METADATA:(.+?)-->$/s);
    if (match) {
      try {
        return JSON.parse(match[1]) as MultiSttHistoryMeta;
      } catch {
        /* fall through */
      }
    }
    return null;
  }, [isMultiStt, entry.transcription_text]);

  // For Multi-STT entries, the primary cleaned text is post_processed_text (merged output)
  // or fall back to parsing the final_merged_text from metadata
  const multiSttCleanedText = useMemo(() => {
    if (!isMultiStt) return null;
    if (entry.post_processed_text) return entry.post_processed_text;
    if (multiSttMeta) return multiSttMeta.final_merged_text;
    return null;
  }, [isMultiStt, entry.post_processed_text, multiSttMeta]);

  const handleLoadAudio = useCallback(
    () => getAudioUrl(entry.file_name),
    [getAudioUrl, entry.file_name],
  );

  const textToDisplay = useMemo(() => {
    // Multi-STT: always show the merged/cleaned output for the copy button / primary text area
    if (isMultiStt && multiSttCleanedText) {
      return multiSttCleanedText;
    }
    if (activeTab === "polished" && entry.post_processed_text) {
      return entry.post_processed_text;
    }
    return entry.transcription_text;
  }, [
    isMultiStt,
    multiSttCleanedText,
    activeTab,
    entry.post_processed_text,
    entry.transcription_text,
  ]);

  const handleCopyText = () => {
    const text =
      isMultiStt && multiSttCleanedText ? multiSttCleanedText : textToDisplay;
    if (!text.trim()) return;
    onCopyText(text);
    setShowCopied(true);
    setTimeout(() => setShowCopied(false), 2000);
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
      toast.success("Transcription updated");
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
      toast.success("Transcript polished with LLM");
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
      toast.success("Multi-STT transcription complete");
    } catch (error) {
      console.error("Failed to Multi-STT re-transcribe:", error);
      toast.error(t("settings.history.multiSttError"), {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setRetrying(null);
    }
  };

  const [loadedAudioDurationSec, setLoadedAudioDurationSec] = useState<
    number | null
  >(null);

  useEffect(() => {
    if (entry.audio_duration_ms == null && entry.file_name) {
      getAudioUrl(entry.file_name).then((url) => {
        if (!url) return;
        const audio = new Audio(url);
        audio.onloadedmetadata = () => {
          if (
            audio.duration &&
            !isNaN(audio.duration) &&
            isFinite(audio.duration)
          ) {
            setLoadedAudioDurationSec(audio.duration);
          }
        };
      });
    }
  }, [entry.audio_duration_ms, entry.file_name, getAudioUrl]);

  // Metric calculations
  const totalDurationSec = entry.audio_duration_ms
    ? entry.audio_duration_ms / 1000
    : loadedAudioDurationSec;
  const speechDurationSec = entry.speech_duration_ms
    ? entry.speech_duration_ms / 1000
    : null;

  const silenceCutPercent = useMemo(() => {
    if (
      totalDurationSec &&
      speechDurationSec &&
      totalDurationSec > speechDurationSec
    ) {
      return Math.max(
        0,
        Math.round(
          ((totalDurationSec - speechDurationSec) / totalDurationSec) * 100,
        ),
      );
    }
    return 0;
  }, [totalDurationSec, speechDurationSec]);

  const wordCount = useMemo(() => {
    if (entry.word_count !== null && entry.word_count !== undefined) {
      return entry.word_count;
    }
    const text = entry.post_processed_text || entry.transcription_text;
    return text.trim() ? text.trim().split(/\s+/).length : 0;
  }, [entry.word_count, entry.post_processed_text, entry.transcription_text]);

  const { wpm, speedRating } = useMemo(() => {
    // Always calculate WPM from the silence-removed speech duration
    const silenceRemovedSec =
      speechDurationSec && speechDurationSec > 0
        ? speechDurationSec
        : totalDurationSec && totalDurationSec > 0
          ? totalDurationSec
          : 0;

    if (silenceRemovedSec <= 0 || wordCount <= 0) {
      return { wpm: 0, speedRating: null };
    }
    const calculatedWpm = Math.round((wordCount / silenceRemovedSec) * 60);

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
  }, [speechDurationSec, totalDurationSec, wordCount, t]);

  const formatDurationSec = (sec: number): string => {
    const rounded = Math.round(sec * 10) / 10;
    return Number.isInteger(rounded) ? `${rounded}s` : `${rounded.toFixed(1)}s`;
  };

  const formattedDate = formatDateTime(String(entry.timestamp), i18n.language);

  return (
    <div className="p-4 flex flex-col gap-3 hover:bg-card/30 transition-colors">
      {/* Header Row: Title & Action Toolbar */}
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-2 flex-wrap">
          <p className="text-xs font-semibold text-text">{formattedDate}</p>

          {/* Model Badge */}
          {entry.model_id && (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] font-medium bg-mid-gray/15 text-text/80 border border-mid-gray/20">
              <Cpu className="w-3 h-3 text-accent" />
              <span>
                {isMultiStt
                  ? `Multi-STT (${(entry.extra_models?.length ?? 0) + 1} models)`
                  : displayModelId(entry.model_id)}
              </span>
            </span>
          )}

          {/* Mode Pill */}
          {entry.mode && entry.mode !== "single" && (
            <span
              className={`px-1.5 py-0.5 rounded text-[10px] font-semibold uppercase tracking-wider ${
                isMultiStt
                  ? "bg-purple-500/10 text-purple-400 border border-purple-500/20"
                  : "bg-teal-500/10 text-teal-400 border border-teal-500/20"
              }`}
            >
              {entry.mode}
            </span>
          )}
        </div>

        {/* Action Buttons */}
        <div className="flex items-center gap-0.5 bg-background/80 border border-mid-gray/20 p-0.5 rounded-lg">
          <IconButton
            onClick={handleCopyText}
            disabled={!hasTranscription || retrying !== null}
            title={t("settings.history.copyToClipboard")}
          >
            {showCopied ? (
              <Check width={15} height={15} className="text-green-400" />
            ) : (
              <Copy width={15} height={15} />
            )}
          </IconButton>

          <IconButton
            onClick={onToggleSaved}
            disabled={retrying !== null}
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
            disabled={retrying !== null}
            title={t("settings.history.retranscribe")}
          >
            <RotateCcw
              width={15}
              height={15}
              className={
                retrying === "standard" ? "animate-spin text-accent" : ""
              }
            />
          </IconButton>

          {/* Post-Process / Polish with LLM */}
          <IconButton
            onClick={handlePostProcess}
            disabled={retrying !== null || !hasTranscription}
            title={t("settings.history.postProcess")}
          >
            <Sparkles
              width={15}
              height={15}
              className={
                retrying === "post_process"
                  ? "animate-pulse text-amber-400"
                  : ""
              }
            />
          </IconButton>

          {/* Multi-STT Parallel Re-transcribe */}
          <IconButton
            onClick={handleMultiStt}
            disabled={retrying !== null}
            title={t("settings.history.multiSttRetranscribe")}
          >
            <Layers
              width={15}
              height={15}
              className={
                retrying === "multi_stt" ? "animate-pulse text-purple-400" : ""
              }
            />
          </IconButton>

          <IconButton
            onClick={handleDeleteEntry}
            disabled={retrying !== null}
            title={t("settings.history.delete")}
            className="hover:text-red-400 hover:bg-red-500/10"
          >
            <Trash2 width={15} height={15} />
          </IconButton>
        </div>
      </div>

      {/* Intelligence & Metrics Bar */}
      <div className="flex flex-wrap items-center gap-2 text-xs">
        {/* Audio Duration & Silence Suppression */}
        {totalDurationSec !== null && (
          <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded bg-mid-gray/10 text-text/80 border border-mid-gray/15">
            <Volume2 className="w-3.5 h-3.5 text-mid-gray" />
            <span>
              {speechDurationSec !== null
                ? silenceCutPercent > 0
                  ? t("settings.history.audioDetails", {
                      total: formatDurationSec(totalDurationSec),
                      speech: formatDurationSec(speechDurationSec),
                      silence: `${silenceCutPercent}%`,
                    })
                  : t("settings.history.audioDetailsNoCut", {
                      total: formatDurationSec(totalDurationSec),
                      speech: formatDurationSec(speechDurationSec),
                    })
                : t("settings.history.audioTotalOnly", {
                    total: formatDurationSec(totalDurationSec),
                  })}
            </span>
          </span>
        )}

        {/* Word Count */}
        {wordCount > 0 && (
          <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded bg-mid-gray/10 text-text/80 border border-mid-gray/15">
            <FileText className="w-3.5 h-3.5 text-mid-gray" />
            <span>{t("settings.history.wordCount", { count: wordCount })}</span>
          </span>
        )}

        {/* WPM Speech Rate */}
        {wpm > 0 && (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded bg-mid-gray/10 text-text/80 border border-mid-gray/15 font-medium">
            <Zap className="w-3 h-3 text-amber-400" />
            <span>{t("settings.history.wpmRating", { wpm })}</span>
          </span>
        )}

        {/* WPM Speed Rating Badge */}
        {speedRating && wpm > 0 && (
          <span
            className={`px-2 py-0.5 rounded text-[11px] font-semibold border ${speedRating.color}`}
          >
            {speedRating.label}
          </span>
        )}

        {/* STT Latency */}
        {entry.transcription_latency_ms !== null &&
          entry.transcription_latency_ms !== undefined && (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded bg-blue-500/10 text-blue-400 border border-blue-500/20 text-[11px] font-medium">
              <Zap className="w-3 h-3" />
              <span>
                {entry.transcription_latency_ms >= 1000
                  ? `${(entry.transcription_latency_ms / 1000).toFixed(1)}s STT`
                  : `${Math.round(entry.transcription_latency_ms)}ms STT`}
              </span>
            </span>
          )}

        {/* LLM Post-Processing Latency */}
        {entry.post_processing_latency_ms !== null &&
          entry.post_processing_latency_ms !== undefined && (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded bg-amber-500/10 text-amber-400 border border-amber-500/20 text-[11px] font-medium">
              <Sparkles className="w-3 h-3" />
              <span>
                {entry.post_processing_latency_ms >= 1000
                  ? `${(entry.post_processing_latency_ms / 1000).toFixed(1)}s LLM`
                  : `${Math.round(entry.post_processing_latency_ms)}ms LLM`}
              </span>
            </span>
          )}

        {/* Audio Format */}
        {entry.sample_rate_hz && (
          <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-mid-gray/10 text-text/60 text-[10px]">
            <span>{`${Math.round(entry.sample_rate_hz / 1000)} kHz`}</span>
          </span>
        )}
      </div>

      {/* View Switcher Tabs */}
      {(entry.post_processed_text || isMultiStt) && (
        <div className="flex items-center gap-1.5 pt-0.5 flex-wrap">
          {/* For Multi-STT: show Details / Cleaned / Raw Log tabs */}
          {isMultiStt && (
            <>
              <button
                onClick={() => setActiveTab("multi_stt_details")}
                className={`px-2.5 py-1 text-xs font-medium rounded-md transition-colors flex items-center gap-1.5 cursor-pointer ${
                  activeTab === "multi_stt_details"
                    ? "bg-purple-500/20 text-purple-300 border border-purple-500/40"
                    : "bg-mid-gray/10 text-text/60 hover:text-text"
                }`}
              >
                <Layers className="w-3 h-3" />
                <span>{t("settings.history.multiSttDetailsTab")}</span>
              </button>
              <button
                onClick={() => setActiveTab("polished")}
                className={`px-2.5 py-1 text-xs font-medium rounded-md transition-colors flex items-center gap-1.5 cursor-pointer ${
                  activeTab === "polished"
                    ? "bg-accent/20 text-accent border border-accent/40"
                    : "bg-mid-gray/10 text-text/60 hover:text-text"
                }`}
              >
                <Sparkles className="w-3 h-3" />
                <span>{t("settings.history.multiSttCleanedOutputTab")}</span>
              </button>
              <button
                onClick={() => setActiveTab("raw")}
                className={`px-2.5 py-1 text-xs font-medium rounded-md transition-colors flex items-center gap-1.5 cursor-pointer ${
                  activeTab === "raw"
                    ? "bg-accent/20 text-accent border border-accent/40"
                    : "bg-mid-gray/10 text-text/60 hover:text-text"
                }`}
              >
                <FileText className="w-3 h-3" />
                <span>{t("settings.history.multiSttRawLogTab")}</span>
              </button>
            </>
          )}
          {/* Non-multi-STT tabs */}
          {!isMultiStt && entry.post_processed_text && (
            <>
              <button
                onClick={() => setActiveTab("polished")}
                className={`px-2.5 py-1 text-xs font-medium rounded-md transition-colors flex items-center gap-1.5 cursor-pointer ${
                  activeTab === "polished"
                    ? "bg-accent/20 text-accent border border-accent/40"
                    : "bg-mid-gray/10 text-text/60 hover:text-text"
                }`}
              >
                <Sparkles className="w-3 h-3" />
                <span>{t("settings.history.polishedTextTab")}</span>
              </button>
              <button
                onClick={() => setActiveTab("raw")}
                className={`px-2.5 py-1 text-xs font-medium rounded-md transition-colors flex items-center gap-1.5 cursor-pointer ${
                  activeTab === "raw"
                    ? "bg-accent/20 text-accent border border-accent/40"
                    : "bg-mid-gray/10 text-text/60 hover:text-text"
                }`}
              >
                <FileText className="w-3 h-3" />
                <span>{t("settings.history.rawTranscriptTab")}</span>
              </button>
            </>
          )}
        </div>
      )}

      {/* Multi-STT Details Panel */}
      {isMultiStt && activeTab === "multi_stt_details" && multiSttMeta && (
        <div className="space-y-3">
          {/* 4 Model Cards */}
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
            {multiSttMeta.models.map((m) => (
              <div
                key={m.slot}
                className="rounded-lg border border-mid-gray/20 bg-card/40 p-3 flex flex-col gap-2"
              >
                <div className="flex items-center justify-between gap-2">
                  <div className="flex items-center gap-1.5 min-w-0">
                    <span className="px-1.5 py-0.5 rounded text-[10px] font-bold bg-purple-500/15 text-purple-300 border border-purple-500/25 shrink-0">
                      {t("settings.history.multiSttModelSlot", {
                        slot: m.slot,
                      })}
                    </span>
                    <span
                      className="text-[11px] font-mono text-text/70 truncate"
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
                    className="p-1 rounded text-text/40 hover:text-accent hover:bg-mid-gray/10 transition-colors disabled:opacity-30 shrink-0"
                  >
                    <Copy className="w-3 h-3" />
                  </button>
                </div>
                <p
                  className={`text-xs select-text cursor-text whitespace-pre-wrap break-words rounded p-2 bg-background/60 border border-mid-gray/15 min-h-[2.5rem] ${
                    m.text.trim() ? "text-text/90" : "text-text/35 italic"
                  }`}
                >
                  {m.text.trim() || t("settings.history.multiSttNoOutput")}
                </p>
              </div>
            ))}
          </div>

          {/* Brain LLM Card */}
          {multiSttMeta.brain && (
            <div className="rounded-lg border border-amber-500/30 bg-amber-500/5 p-3 flex flex-col gap-2">
              <div className="flex items-center justify-between gap-2 flex-wrap">
                <div className="flex items-center gap-1.5 flex-wrap">
                  <span className="px-1.5 py-0.5 rounded text-[10px] font-bold bg-amber-500/20 text-amber-300 border border-amber-500/30">
                    {t("settings.history.multiSttBrainLabel")}
                  </span>
                  <span className="text-[11px] font-mono text-text/75 font-medium">
                    {multiSttMeta.brain.model_name}
                  </span>
                  <span className="text-[10px] text-text/50 bg-mid-gray/10 px-1.5 py-0.5 rounded border border-mid-gray/20">
                    {multiSttMeta.brain.provider_label} (
                    {multiSttMeta.brain.provider_id})
                  </span>
                  {multiSttMeta.brain.prompt_name && (
                    <span className="text-[10px] text-text/50 bg-mid-gray/10 px-1.5 py-0.5 rounded border border-mid-gray/20">
                      {t("settings.history.multiSttBrainPromptLabel", {
                        name: multiSttMeta.brain.prompt_name,
                      })}
                    </span>
                  )}
                  {multiSttMeta.brain.latency_ms != null && (
                    <span className="text-[10px] text-amber-400 bg-amber-500/10 px-1.5 py-0.5 rounded border border-amber-500/20">
                      {multiSttMeta.brain.latency_ms >= 1000
                        ? `${(multiSttMeta.brain.latency_ms / 1000).toFixed(1)}s LLM`
                        : `${Math.round(multiSttMeta.brain.latency_ms)}ms LLM`}
                    </span>
                  )}
                </div>
                <button
                  onClick={() => {
                    if (multiSttMeta.brain?.cleaned_output.trim())
                      onCopyText(multiSttMeta.brain.cleaned_output);
                  }}
                  disabled={!multiSttMeta.brain.cleaned_output.trim()}
                  title={t("settings.history.multiSttCopyBrainOutput")}
                  className="p-1 rounded text-text/40 hover:text-accent hover:bg-mid-gray/10 transition-colors disabled:opacity-30 shrink-0"
                >
                  <Copy className="w-3 h-3" />
                </button>
              </div>

              {/* Cleaned output */}
              <div>
                <p className="text-[10px] text-text/50 mb-1 font-medium uppercase tracking-wider">
                  {t("settings.history.multiSttBrainCleanedOutput")}
                </p>
                <p className="text-xs select-text cursor-text whitespace-pre-wrap break-words rounded p-2 bg-background/60 border border-amber-500/15 text-text/90">
                  {multiSttMeta.brain.cleaned_output ||
                    t("settings.history.multiSttNoOutput")}
                </p>
              </div>

              {/* Raw output toggle */}
              {multiSttMeta.brain.raw_output && (
                <div>
                  <button
                    onClick={() => setShowBrainRaw((v) => !v)}
                    className="text-[10px] text-text/50 hover:text-amber-400 flex items-center gap-1 transition-colors font-medium uppercase tracking-wider"
                  >
                    {showBrainRaw ? (
                      <ChevronUp className="w-3 h-3" />
                    ) : (
                      <ChevronDown className="w-3 h-3" />
                    )}
                    {t("settings.history.multiSttBrainRawToggle")}
                  </button>
                  {showBrainRaw && (
                    <pre className="mt-1 p-2 rounded bg-background/90 text-[10px] text-text/65 border border-amber-500/20 whitespace-pre-wrap font-mono overflow-x-auto max-h-56 overflow-y-auto">
                      {multiSttMeta.brain.raw_output}
                    </pre>
                  )}
                </div>
              )}
            </div>
          )}
        </div>
      )}

      {/* Transcript Text Display (non-details tabs) */}
      {activeTab !== "multi_stt_details" && (
        <div className="relative">
          <p
            className={`text-sm select-text cursor-text whitespace-pre-wrap break-words rounded-md p-2.5 bg-card/40 border border-mid-gray/15 ${
              retrying !== null
                ? "text-text/40 animate-pulse"
                : hasTranscription
                  ? "text-text/95"
                  : "text-text/40 italic"
            }`}
          >
            {retrying === "standard"
              ? t("settings.history.transcribing")
              : retrying === "post_process"
                ? t("settings.history.postProcessing")
                : retrying === "multi_stt"
                  ? t("settings.history.multiSttTranscribing")
                  : hasTranscription
                    ? textToDisplay
                    : t("settings.history.transcriptionFailed")}
          </p>

          {/* Expandable prompt details */}
          {entry.post_process_prompt && (
            <div className="mt-1">
              <button
                onClick={() => setShowPromptDetails(!showPromptDetails)}
                className="text-[11px] text-text/50 hover:text-accent flex items-center gap-1 transition-colors"
              >
                {showPromptDetails ? (
                  <ChevronUp className="w-3 h-3" />
                ) : (
                  <ChevronDown className="w-3 h-3" />
                )}
                <span>{t("settings.history.promptUsed")}</span>
              </button>
              {showPromptDetails && (
                <pre className="mt-1 p-2 rounded bg-background/90 text-[11px] text-text/70 border border-mid-gray/20 whitespace-pre-wrap font-mono">
                  {entry.post_process_prompt}
                </pre>
              )}
            </div>
          )}
        </div>
      )}

      {/* Audio Waveform Player */}
      <AudioPlayer onLoadRequest={handleLoadAudio} className="w-full mt-1" />
    </div>
  );
};
