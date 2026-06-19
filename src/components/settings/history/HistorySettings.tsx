import React, { useCallback, useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { readFile } from "@tauri-apps/plugin-fs";
import {
  Check,
  Copy,
  FolderOpen,
  RotateCcw,
  Sparkles,
  Star,
  Trash2,
  Mic,
  Volume2,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { save } from "@tauri-apps/plugin-dialog";
import {
  commands,
  events,
  type HistoryEntry,
  type HistoryUpdatePayload,
  type PostProcessAction,
} from "@/bindings";
import { useOsType } from "@/hooks/useOsType";
import { useSettings } from "@/hooks/useSettings";
import { getActionIcon } from "@/lib/constants/actionIcons";
import { formatDateTime } from "@/utils/dateFormat";
import { AudioPlayer } from "../../ui/AudioPlayer";
import { Button } from "../../ui/Button";

const IconButton: React.FC<{
  onClick: () => void;
  title: string;
  disabled?: boolean;
  active?: boolean;
  children: React.ReactNode;
}> = ({ onClick, title, disabled, active, children }) => (
  <button
    onClick={onClick}
    disabled={disabled}
    className={`p-1.5 rounded-md flex items-center justify-center transition-colors cursor-pointer disabled:cursor-not-allowed disabled:text-text/20 ${
      active
        ? "text-logo-primary hover:text-logo-primary/80"
        : "text-text/50 hover:text-logo-primary"
    }`}
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

export const HistorySettings: React.FC = () => {
  const { t } = useTranslation();
  const osType = useOsType();
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [hasMore, setHasMore] = useState(true);
  const sentinelRef = useRef<HTMLDivElement>(null);
  const entriesRef = useRef<HistoryEntry[]>([]);
  const loadingRef = useRef(false);

  // Upgraded multi-select state
  const [selectedIds, setSelectedIds] = useState<number[]>([]);
  const [exportingSelected, setExportingSelected] = useState(false);

  // Keep ref in sync for use in IntersectionObserver callback
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

  // Infinite scroll via IntersectionObserver
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

  // Listen for new entries added from the transcription pipeline
  useEffect(() => {
    const unlisten = events.historyUpdatePayload.listen((event) => {
      const payload: HistoryUpdatePayload = event.payload;
      if (payload.action === "added") {
        setEntries((prev) => [payload.entry, ...prev]);
      } else if (payload.action === "updated") {
        setEntries((prev) =>
          prev.map((e) => (e.id === payload.entry.id ? payload.entry : e)),
        );
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const toggleSaved = async (id: number) => {
    // Optimistic update
    setEntries((prev) =>
      prev.map((e) => (e.id === id ? { ...e, saved: !e.saved } : e)),
    );
    try {
      const result = await commands.toggleHistoryEntrySaved(id);
      if (result.status !== "ok") {
        // Revert on failure
        setEntries((prev) =>
          prev.map((e) => (e.id === id ? { ...e, saved: !e.saved } : e)),
        );
      }
    } catch (error) {
      console.error("Failed to toggle saved status:", error);
      // Revert on failure
      setEntries((prev) =>
        prev.map((e) => (e.id === id ? { ...e, saved: !e.saved } : e)),
      );
    }
  };

  const copyToClipboard = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
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
    // Optimistically remove
    setEntries((prev) => prev.filter((e) => e.id !== id));
    setSelectedIds((prev) => prev.filter((selectedId) => selectedId !== id));
    try {
      const result = await commands.deleteHistoryEntry(id);
      if (result.status !== "ok") {
        // Reload on failure
        loadPage();
      }
    } catch (error) {
      console.error("Failed to delete entry:", error);
      loadPage();
    }
  };

  const regenerateHistoryEntry = async (id: number) => {
    const result = await commands.regenerateHistoryEntry(id);
    if (result.status !== "ok") {
      throw new Error(String(result.error));
    }
  };

  const deleteAllEntries = async () => {
    try {
      const result = await commands.deleteAllHistoryEntries();
      if (result.status === "ok") {
        setEntries([]);
        setSelectedIds([]);
        toast.success(
          t("settings.history.deleteAllSuccess", {
            defaultValue: `Deleted all entries`,
          }),
        );
      }
    } catch (error) {
      console.error("Failed to delete all entries:", error);
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

  // Upgraded bulk selection actions
  const handleToggleSelect = (id: number) => {
    setSelectedIds((prev) =>
      prev.includes(id)
        ? prev.filter((selectedId) => selectedId !== id)
        : [...prev, id],
    );
  };

  const isAllSelected =
    entries.length > 0 && selectedIds.length === entries.length;

  const handleSelectAllToggle = () => {
    if (isAllSelected) {
      setSelectedIds([]);
    } else {
      setSelectedIds(entries.map((e) => e.id));
    }
  };

  const deleteSelectedEntries = async () => {
    if (selectedIds.length === 0) return;
    const idsToDelete = [...selectedIds];

    // Optimistically remove from UI
    setEntries((prev) => prev.filter((e) => !idsToDelete.includes(e.id)));
    setSelectedIds([]);

    try {
      const result = await commands.deleteHistoryEntries(idsToDelete);
      if (result.status === "ok") {
        toast.success(
          t("settings.history.deleteSelectedSuccess", {
            defaultValue: "Selected entries deleted successfully.",
          }),
        );
      } else {
        toast.error(
          t("settings.history.deleteSelectedError", {
            defaultValue: `Delete failed: ${result.error}`,
          }),
        );
        loadPage();
      }
    } catch (error) {
      console.error("Failed to delete selected entries:", error);
      toast.error(String(error));
      loadPage();
    }
  };

  const exportSelectedEntries = async () => {
    if (selectedIds.length === 0) return;

    try {
      setExportingSelected(true);
      const filePath = await save({
        filters: [
          {
            name: "Markdown",
            extensions: ["md"],
          },
        ],
        defaultPath: "s2b2s-history-export.md",
      });

      if (!filePath) {
        return;
      }

      const result = await commands.exportHistoryEntries(selectedIds, filePath);
      if (result.status === "ok") {
        toast.success(
          t("settings.history.exportSelectedSuccess", {
            defaultValue: "Selected entries exported successfully!",
          }),
        );
      } else {
        toast.error(
          t("settings.history.exportSelectedError", {
            defaultValue: `Export failed: ${result.error}`,
          }),
        );
      }
    } catch (err) {
      console.error("Failed to export selected entries:", err);
      toast.error(String(err));
    } finally {
      setExportingSelected(false);
    }
  };

  let content: React.ReactNode;

  if (loading) {
    content = (
      <div className="px-4 py-3 text-center text-text/60">
        {t("settings.history.loading")}
      </div>
    );
  } else if (entries.length === 0) {
    content = (
      <div className="px-4 py-3 text-center text-text/60">
        {t("settings.history.empty")}
      </div>
    );
  } else {
    content = (
      <>
        <div className="divide-y divide-mid-gray/20">
          {entries.map((entry) => (
            <HistoryEntryComponent
              key={entry.id}
              entry={entry}
              onToggleSaved={() => toggleSaved(entry.id)}
              onCopyText={() => copyToClipboard(entry.transcription_text)}
              getAudioUrl={getAudioUrl}
              deleteAudio={deleteAudioEntry}
              regenerateEntry={regenerateHistoryEntry}
              isSelected={selectedIds.includes(entry.id)}
              onToggleSelect={() => handleToggleSelect(entry.id)}
            />
          ))}
        </div>
        {/* Sentinel for infinite scroll */}
        <div ref={sentinelRef} className="h-1" />
      </>
    );
  }

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <div className="space-y-2">
        {selectedIds.length > 0 ? (
          <div className="px-4 py-2 bg-mid-gray/10 rounded-lg flex items-center justify-between border border-mid-gray/20 animate-fade-in">
            <div className="flex items-center gap-3">
              <input
                type="checkbox"
                checked={isAllSelected}
                onChange={handleSelectAllToggle}
                className="w-4 h-4 rounded border-mid-gray/40 text-logo-primary focus:ring-logo-primary bg-background-ui cursor-pointer"
              />
              <span className="text-xs text-text/80 font-medium">
                {t("settings.history.selectedCount", {
                  count: selectedIds.length,
                  defaultValue: `${selectedIds.length} selected`,
                })}
              </span>
            </div>
            <div className="flex items-center gap-2">
              <Button
                onClick={exportSelectedEntries}
                variant="secondary"
                size="sm"
                className="flex items-center gap-1"
                disabled={exportingSelected}
              >
                <FolderOpen className="w-3.5 h-3.5" />
                <span className="text-xs">
                  {exportingSelected
                    ? t("settings.history.exporting", {
                        defaultValue: "Exporting...",
                      })
                    : t("settings.history.exportSelected", {
                        defaultValue: "Export Markdown",
                      })}
                </span>
              </Button>
              <Button
                onClick={deleteSelectedEntries}
                variant="danger"
                size="sm"
                className="flex items-center gap-1"
              >
                <Trash2 className="w-3.5 h-3.5" />
                <span className="text-xs">
                  {t("settings.history.deleteSelected", {
                    defaultValue: "Delete Selected",
                  })}
                </span>
              </Button>
            </div>
          </div>
        ) : (
          <div className="px-4 flex items-center justify-between">
            <div>
              <h2 className="text-xs font-medium text-mid-gray uppercase tracking-wide">
                {t("settings.history.title")}
              </h2>
            </div>
            <div className="flex items-center gap-2">
              <Button
                onClick={deleteAllEntries}
                variant="secondary"
                size="sm"
                className="flex items-center gap-1 text-red-400 hover:text-red-300"
                disabled={entries.length === 0}
              >
                <Trash2 className="w-3.5 h-3.5" />
                <span className="text-xs">
                  {t("settings.history.deleteAll")}
                </span>
              </Button>
              <OpenRecordingsButton
                onClick={openRecordingsFolder}
                label={t("settings.history.openFolder")}
              />
            </div>
          </div>
        )}
        <div className="bg-background border border-mid-gray/20 rounded-lg overflow-visible">
          {content}
        </div>
      </div>
    </div>
  );
};

interface HistoryEntryProps {
  entry: HistoryEntry;
  onToggleSaved: () => void;
  onCopyText: () => void;
  getAudioUrl: (fileName: string) => Promise<string | null>;
  deleteAudio: (id: number) => Promise<void>;
  regenerateEntry: (id: number) => Promise<void>;
  isSelected: boolean;
  onToggleSelect: () => void;
}

const HistoryEntryComponent: React.FC<HistoryEntryProps> = ({
  entry,
  onToggleSaved,
  onCopyText,
  getAudioUrl,
  deleteAudio,
  regenerateEntry,
  isSelected,
  onToggleSelect,
}) => {
  const { t, i18n } = useTranslation();
  const [showCopied, setShowCopied] = useState(false);
  const [regenerating, setRegenerating] = useState(false);

  const hasTranscription = entry.transcription_text.trim().length > 0;

  const handleLoadAudio = useCallback(
    () => getAudioUrl(entry.file_name),
    [getAudioUrl, entry.file_name],
  );

  const handleCopyText = () => {
    if (!hasTranscription) {
      return;
    }

    onCopyText();
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

  const handleRegenerate = async () => {
    try {
      setRegenerating(true);
      await regenerateEntry(entry.id);
      toast.success(
        t("settings.history.regenerateSuccess", {
          defaultValue: "History entry successfully regenerated!",
        }),
      );
    } catch (error) {
      console.error("Failed to regenerate history entry:", error);
      toast.error(
        t("settings.history.regenerateError", {
          defaultValue: "Failed to regenerate entry.",
        }),
      );
    } finally {
      setRegenerating(false);
    }
  };

  const formattedDate = formatDateTime(String(entry.timestamp), i18n.language);
  const isTts = entry.entry_type === "tts";

  return (
    <div className="px-4 py-2 pb-5 flex gap-4 items-start hover:bg-mid-gray/5 transition-colors">
      <div className="pt-2 flex items-center">
        <input
          type="checkbox"
          checked={isSelected}
          onChange={onToggleSelect}
          className="w-4 h-4 rounded border-mid-gray/40 text-logo-primary focus:ring-logo-primary bg-background-ui cursor-pointer"
        />
      </div>
      <div className="flex-1 flex flex-col gap-3 min-w-0">
        <div className="flex justify-between items-center">
          <div className="flex items-center gap-2">
            <p className="text-sm font-medium">{formattedDate}</p>
            <span
              className={`text-[10px] px-1.5 py-0.5 rounded-full font-medium ${isTts ? "bg-purple-500/20 text-purple-400" : "bg-logo-primary/20 text-logo-primary"}`}
            >
              {isTts ? "TTS" : "STT"}
            </span>
          </div>
          <div className="flex items-center">
            <IconButton
              onClick={handleCopyText}
              disabled={!hasTranscription || regenerating}
              title={t("settings.history.copyToClipboard")}
            >
              {showCopied ? (
                <Check width={16} height={16} />
              ) : (
                <Copy width={16} height={16} />
              )}
            </IconButton>
            <IconButton
              onClick={onToggleSaved}
              disabled={regenerating}
              active={entry.saved}
              title={
                entry.saved
                  ? t("settings.history.unsave")
                  : t("settings.history.save")
              }
            >
              <Star
                width={16}
                height={16}
                fill={entry.saved ? "currentColor" : "none"}
              />
            </IconButton>
            <IconButton
              onClick={handleRegenerate}
              disabled={regenerating}
              title={
                isTts
                  ? t("settings.history.resynthesize", {
                      defaultValue: "Re-synthesize audio",
                    })
                  : t("settings.history.retranscribe", {
                      defaultValue: "Re-transcribe audio",
                    })
              }
            >
              <RotateCcw
                width={16}
                height={16}
                style={
                  regenerating
                    ? { animation: "spin 1s linear infinite reverse" }
                    : undefined
                }
              />
            </IconButton>
            <IconButton
              onClick={handleDeleteEntry}
              disabled={regenerating}
              title={t("settings.history.delete")}
            >
              <Trash2 width={16} height={16} />
            </IconButton>
          </div>
        </div>

        <p
          className={`italic text-sm pb-2 ${
            regenerating
              ? ""
              : hasTranscription
                ? "text-text/90 select-text cursor-text whitespace-pre-wrap break-words"
                : "text-text/40"
          }`}
          style={
            regenerating
              ? { animation: "transcribe-pulse 3s ease-in-out infinite" }
              : undefined
          }
        >
          {regenerating && (
            <style>{`
              @keyframes transcribe-pulse {
                0%, 100% { color: color-mix(in srgb, var(--color-text) 40%, transparent); }
                50% { color: color-mix(in srgb, var(--color-text) 90%, transparent); }
              }
            `}</style>
          )}
          {regenerating
            ? isTts
              ? t("settings.history.synthesizing", {
                  defaultValue: "Synthesizing audio...",
                })
              : t("settings.history.transcribing", {
                  defaultValue: "Transcribing...",
                })
            : hasTranscription
              ? entry.transcription_text
              : t("settings.history.transcriptionFailed")}
        </p>

        {(entry.model_name || entry.duration_ms != null) && (
          <div className="flex items-center gap-3 text-[10px] text-text/40">
            {isTts ? (
              <span className="flex items-center gap-1">
                <Volume2 className="w-3 h-3" />
                {entry.model_name || "TTS"}
              </span>
            ) : (
              <span className="flex items-center gap-1">
                <Mic className="w-3 h-3" />
                {entry.model_name || "STT"}
              </span>
            )}
            {entry.duration_ms != null && (
              /* eslint-disable-next-line i18next/no-literal-string */
              <span>{entry.duration_ms}ms</span>
            )}
          </div>
        )}

        <AudioPlayer onLoadRequest={handleLoadAudio} className="w-full" />
      </div>
    </div>
  );
};
