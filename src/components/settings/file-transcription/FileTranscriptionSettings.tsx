import React, { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { toast } from "sonner";
import {
  Check,
  Copy,
  ExternalLink,
  FileAudio,
  FolderOpen,
  FilePlus,
  Loader2,
  Play,
  Square,
  Trash2,
  Upload,
  X,
} from "lucide-react";

import {
  commands,
  type FileJobStatus,
  type FileTranscriptionMode,
  type FileTranscriptionSettings as FileTranscriptionSettingsType,
  type TranscriptOutputFormat,
} from "@/bindings";
import {
  SettingContainer,
  SettingsGroup,
  Slider,
  ToggleSwitch,
} from "@/components/ui";
import { Alert } from "@/components/ui/Alert";
import { Button } from "@/components/ui/Button";
import { Dropdown, type DropdownOption } from "@/components/ui/Dropdown";
import ProgressBar from "@/components/shared/ProgressBar";
import { useSettings } from "@/hooks/useSettings";
import {
  SUPPORTED_AUDIO_EXTENSIONS,
  isSupportedAudioPath,
  isTerminalStatus,
  type QueueItem,
  useFileTranscriptionStore,
} from "@/stores/fileTranscriptionStore";

const DEFAULTS: Required<FileTranscriptionSettingsType> = {
  mode: "simple",
  output_dir: null,
  output_format: "txt",
  overwrite_existing: false,
  include_subfolders: true,
  max_segment_minutes: 10,
};

const MODES: FileTranscriptionMode[] = [
  "simple",
  "post_process",
  "multi_stt",
  "multi_stt_post_process",
];

const formatSeconds = (secs: number): string => {
  const total = Math.max(0, Math.round(secs));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mm = h > 0 ? String(m).padStart(2, "0") : String(m);
  return `${h > 0 ? `${h}:` : ""}${mm}:${String(s).padStart(2, "0")}`;
};

const STATUS_CLASSES: Record<FileJobStatus, string> = {
  queued: "bg-mid-gray/15 text-text/70 border-mid-gray/20",
  decoding: "bg-logo-primary/15 text-text border-logo-primary/30",
  transcribing: "bg-logo-primary/20 text-text border-logo-primary/40",
  merging: "bg-logo-primary/20 text-text border-logo-primary/40",
  post_processing: "bg-logo-primary/20 text-text border-logo-primary/40",
  saving: "bg-logo-primary/15 text-text border-logo-primary/30",
  done: "bg-green-500/15 text-green-500 border-green-500/30",
  failed: "bg-red-500/15 text-red-400 border-red-500/30",
  cancelled: "bg-mid-gray/15 text-text/60 border-mid-gray/20",
};

const isBusy = (status: FileJobStatus) =>
  !isTerminalStatus(status) && status !== "queued";

/** One queued file: status pill, progress text, and the result once done. */
const QueueRow: React.FC<{
  item: QueueItem;
  running: boolean;
  onRemove: () => void;
}> = ({ item, running, onRemove }) => {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    if (!item.text) return;
    try {
      await navigator.clipboard.writeText(item.text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch (error) {
      console.error("Copy failed:", error);
    }
  };

  const reveal = async (path: string) => {
    const result = await commands.revealPathInFileManager(path);
    if (result.status === "error") toast.error(result.error);
  };

  const meta: string[] = [];
  if (item.audioSeconds != null) meta.push(formatSeconds(item.audioSeconds));
  if (item.elapsedMs != null && item.status === "done") {
    meta.push(
      t("settings.fileTranscription.result.took", {
        seconds: (item.elapsedMs / 1000).toFixed(1),
      }),
    );
  }
  if (item.status === "transcribing" && item.segments && item.segments > 1) {
    meta.push(
      t("settings.fileTranscription.segmentProgress", {
        current: item.segment ?? 1,
        total: item.segments,
      }),
    );
  }

  return (
    <li className="flex flex-col gap-2 px-3 py-2.5 border-b border-mid-gray/15 last:border-b-0">
      <div className="flex items-center gap-3 min-w-0">
        <FileAudio className="w-4 h-4 shrink-0 text-logo-primary" />
        <div className="flex-1 min-w-0">
          <p className="text-sm truncate" title={item.path}>
            {item.name}
          </p>
          <p className="text-xs text-mid-gray truncate">
            {item.error ? (
              <span className="text-red-400">{item.error}</span>
            ) : (
              meta.join(" · ")
            )}
          </p>
        </div>
        <span
          className={`inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] font-medium border shrink-0 ${STATUS_CLASSES[item.status]}`}
        >
          {isBusy(item.status) && <Loader2 className="w-3 h-3 animate-spin" />}
          {t(`settings.fileTranscription.status.${item.status}`)}
        </span>
        {item.status === "done" && item.text != null && (
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setExpanded((v) => !v)}
            title={t(
              expanded
                ? "settings.fileTranscription.result.hide"
                : "settings.fileTranscription.result.show",
            )}
          >
            {t(
              expanded
                ? "settings.fileTranscription.result.hide"
                : "settings.fileTranscription.result.show",
            )}
          </Button>
        )}
        {!running && (
          <Button
            variant="ghost"
            size="sm"
            onClick={onRemove}
            title={t("settings.fileTranscription.remove")}
            aria-label={t("settings.fileTranscription.remove")}
          >
            <X className="w-4 h-4" />
          </Button>
        )}
      </div>

      {expanded && item.text != null && (
        <div className="ml-7 space-y-2">
          <textarea
            readOnly
            value={item.text}
            className="w-full h-36 p-2.5 text-sm rounded-md bg-background border border-mid-gray/30 text-text resize-y focus:outline-none"
          />
          <div className="flex flex-wrap items-center gap-2 text-xs text-mid-gray">
            <Button variant="ghost" size="sm" onClick={copy}>
              {copied ? (
                <span className="inline-flex items-center gap-1">
                  <Check className="w-3 h-3" />
                  {t("settings.fileTranscription.result.copied")}
                </span>
              ) : (
                <span className="inline-flex items-center gap-1">
                  <Copy className="w-3 h-3" />
                  {t("settings.fileTranscription.result.copy")}
                </span>
              )}
            </Button>
            {item.outputPath && (
              <>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => reveal(item.outputPath as string)}
                >
                  <span className="inline-flex items-center gap-1">
                    <ExternalLink className="w-3 h-3" />
                    {t("settings.fileTranscription.result.reveal")}
                  </span>
                </Button>
                <span className="truncate" title={item.outputPath}>
                  {t("settings.fileTranscription.result.savedTo", {
                    path: item.outputPath,
                  })}
                </span>
              </>
            )}
          </div>
        </div>
      )}
    </li>
  );
};

export const FileTranscriptionSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const store = useFileTranscriptionStore();
  const [dragOver, setDragOver] = useState(false);

  const options = useMemo<Required<FileTranscriptionSettingsType>>(
    () => ({ ...DEFAULTS, ...getSetting("file_transcription") }),
    [getSetting],
  );
  const postProcessEnabled = getSetting("post_process_enabled") ?? false;
  const multiSttEnabled = getSetting("multi_stt_enabled") ?? false;
  const hasExtraModels = Boolean(
    getSetting("multi_stt_model_2") ||
    getSetting("multi_stt_model_3") ||
    getSetting("multi_stt_model_4"),
  );

  const saveOptions = useCallback(
    (patch: Partial<FileTranscriptionSettingsType>) =>
      updateSetting("file_transcription", { ...options, ...patch }),
    [options, updateSetting],
  );

  useEffect(() => {
    void store.initialize();
  }, [store.initialize]);

  const addFolder = useCallback(
    async (folder: string) => {
      const result = await commands.listAudioFilesInFolder(
        folder,
        options.include_subfolders,
      );
      if (result.status === "error") {
        toast.error(result.error);
        return 0;
      }
      return store.addPaths(result.data);
    },
    [options.include_subfolders, store.addPaths],
  );

  const addDropped = useCallback(
    async (paths: string[]) => {
      let added = 0;
      for (const path of paths) {
        if (isSupportedAudioPath(path)) {
          added += store.addPaths([path]);
        } else {
          // Folders have no extension we recognise; the backend tells us if
          // the path is not a folder either.
          const result = await commands.listAudioFilesInFolder(
            path,
            options.include_subfolders,
          );
          if (result.status === "ok") added += store.addPaths(result.data);
        }
      }
      if (added === 0) {
        toast.error(
          t("settings.fileTranscription.errors.nothingAdded", {
            formats: SUPPORTED_AUDIO_EXTENSIONS.join(", ").toUpperCase(),
          }),
        );
      }
    },
    [options.include_subfolders, store.addPaths, t],
  );

  // Native drag & drop from the OS onto the window.
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    getCurrentWebviewWindow()
      .onDragDropEvent((event) => {
        if (store.running) return;
        if (event.payload.type === "enter" || event.payload.type === "over") {
          setDragOver(true);
        } else if (event.payload.type === "leave") {
          setDragOver(false);
        } else if (event.payload.type === "drop") {
          setDragOver(false);
          void addDropped(event.payload.paths);
        }
      })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch((error) =>
        console.error("Drag & drop listener unavailable:", error),
      );
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [addDropped, store.running]);

  const pickFiles = async () => {
    const selection = await open({
      multiple: true,
      directory: false,
      filters: [
        {
          name: t("settings.fileTranscription.dialog.filterName"),
          extensions: [...SUPPORTED_AUDIO_EXTENSIONS],
        },
      ],
    });
    if (!selection) return;
    const paths = Array.isArray(selection) ? selection : [selection];
    store.addPaths(paths);
  };

  const pickFolder = async () => {
    const selection = await open({ directory: true, multiple: false });
    if (!selection || Array.isArray(selection)) return;
    const added = await addFolder(selection);
    if (added === 0) {
      toast.error(t("settings.fileTranscription.errors.emptyFolder"));
    }
  };

  const pickOutputFolder = async () => {
    const selection = await open({ directory: true, multiple: false });
    if (!selection || Array.isArray(selection)) return;
    void saveOptions({ output_dir: selection });
  };

  const start = async () => {
    const error = await store.start();
    if (error) toast.error(error);
  };

  const modeOptions = useMemo<DropdownOption[]>(
    () =>
      MODES.map((mode) => ({
        value: mode,
        label: t(`settings.fileTranscription.mode.options.${mode}.label`),
        description: t(
          `settings.fileTranscription.mode.options.${mode}.description`,
        ),
      })),
    [t],
  );
  const formatOptions = useMemo<DropdownOption[]>(
    () => [
      { value: "txt", label: t("settings.fileTranscription.format.txt") },
      { value: "md", label: t("settings.fileTranscription.format.md") },
    ],
    [t],
  );

  const needsPostProcess =
    options.mode === "post_process" ||
    options.mode === "multi_stt_post_process";
  const needsMultiStt =
    options.mode === "multi_stt" || options.mode === "multi_stt_post_process";

  const total = store.items.length;
  const finished = store.items.filter((i) => isTerminalStatus(i.status)).length;
  const failed = store.items.filter((i) => i.status === "failed").length;
  const done = store.items.filter((i) => i.status === "done").length;
  const pending = store.items.filter((i) => i.status !== "done").length;
  const percentage = total === 0 ? 0 : Math.round((finished / total) * 100);

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6 pb-8">
      <SettingsGroup
        title={t("settings.fileTranscription.groups.files")}
        description={t("settings.fileTranscription.description")}
      >
        <div className="p-3 space-y-3">
          <div
            role="button"
            tabIndex={0}
            aria-label={t("settings.fileTranscription.dropZone.title")}
            onClick={pickFiles}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                void pickFiles();
              }
            }}
            className={`border-2 border-dashed rounded-xl p-6 text-center cursor-pointer transition-colors ${
              dragOver
                ? "border-logo-primary bg-logo-primary/10"
                : "border-mid-gray/30 hover:border-logo-primary/50 hover:bg-mid-gray/5"
            }`}
          >
            <div className="flex flex-col items-center gap-2">
              <div
                className={`p-3 rounded-full ${dragOver ? "bg-logo-primary/20" : "bg-mid-gray/10"}`}
              >
                <Upload className="w-6 h-6 text-logo-primary" />
              </div>
              <p className="text-sm font-medium">
                {t("settings.fileTranscription.dropZone.title")}
              </p>
              <p className="text-xs text-mid-gray">
                {t("settings.fileTranscription.dropZone.subtitle")}
              </p>
              <p className="text-[11px] text-mid-gray/70 uppercase tracking-wide">
                {SUPPORTED_AUDIO_EXTENSIONS.join(" · ")}
              </p>
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <Button
              variant="secondary"
              size="sm"
              onClick={pickFiles}
              disabled={store.running}
            >
              <span className="inline-flex items-center gap-1.5">
                <FilePlus className="w-4 h-4" />
                {t("settings.fileTranscription.addFiles")}
              </span>
            </Button>
            <Button
              variant="secondary"
              size="sm"
              onClick={pickFolder}
              disabled={store.running}
            >
              <span className="inline-flex items-center gap-1.5">
                <FolderOpen className="w-4 h-4" />
                {t("settings.fileTranscription.addFolder")}
              </span>
            </Button>
            <div className="flex-1" />
            {total > 0 && (
              <span className="text-xs text-mid-gray">
                {t("settings.fileTranscription.queued", { count: total })}
              </span>
            )}
            {finished > 0 && !store.running && (
              <Button variant="ghost" size="sm" onClick={store.clearFinished}>
                {t("settings.fileTranscription.clearFinished")}
              </Button>
            )}
            {total > 0 && !store.running && (
              <Button
                variant="danger-ghost"
                size="sm"
                onClick={store.clear}
                title={t("settings.fileTranscription.clear")}
              >
                <span className="inline-flex items-center gap-1.5">
                  <Trash2 className="w-4 h-4" />
                  {t("settings.fileTranscription.clear")}
                </span>
              </Button>
            )}
          </div>

          {total > 0 && (
            <ul className="rounded-lg border border-mid-gray/20 bg-background max-h-80 overflow-y-auto">
              {store.items.map((item) => (
                <QueueRow
                  key={item.path}
                  item={item}
                  running={store.running}
                  onRemove={() => store.removePath(item.path)}
                />
              ))}
            </ul>
          )}
        </div>
      </SettingsGroup>

      <SettingsGroup title={t("settings.fileTranscription.groups.options")}>
        <SettingContainer
          title={t("settings.fileTranscription.mode.title")}
          description={t("settings.fileTranscription.mode.description")}
          descriptionMode="tooltip"
          grouped
          layout="horizontal"
        >
          <Dropdown
            options={modeOptions}
            selectedValue={options.mode}
            onSelect={(value) =>
              saveOptions({ mode: value as FileTranscriptionMode })
            }
            disabled={store.running || isUpdating("file_transcription")}
            className="min-w-[220px]"
          />
        </SettingContainer>

        {needsPostProcess && !postProcessEnabled && (
          <div className="px-4 pb-3">
            <Alert variant="warning">
              {t("settings.fileTranscription.warnings.postProcessDisabled")}
            </Alert>
          </div>
        )}
        {needsMultiStt && (!multiSttEnabled || !hasExtraModels) && (
          <div className="px-4 pb-3">
            <Alert variant="warning">
              {t("settings.fileTranscription.warnings.multiSttNoModels")}
            </Alert>
          </div>
        )}

        <SettingContainer
          title={t("settings.fileTranscription.outputFolder.title")}
          description={t("settings.fileTranscription.outputFolder.description")}
          descriptionMode="tooltip"
          grouped
          layout="stacked"
        >
          <div className="flex flex-wrap items-center gap-2 w-full">
            <span
              className="flex-1 min-w-0 text-xs font-mono truncate px-2 py-1.5 rounded-md bg-background border border-mid-gray/20"
              title={options.output_dir ?? undefined}
            >
              {options.output_dir ??
                t("settings.fileTranscription.outputFolder.nextToSource")}
            </span>
            <Button
              variant="secondary"
              size="sm"
              onClick={pickOutputFolder}
              disabled={store.running}
            >
              {t("settings.fileTranscription.outputFolder.choose")}
            </Button>
            {options.output_dir && (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => saveOptions({ output_dir: null })}
                disabled={store.running}
              >
                {t("settings.fileTranscription.outputFolder.useNextToSource")}
              </Button>
            )}
          </div>
        </SettingContainer>

        <SettingContainer
          title={t("settings.fileTranscription.format.title")}
          description={t("settings.fileTranscription.format.description")}
          descriptionMode="tooltip"
          grouped
          layout="horizontal"
        >
          <Dropdown
            options={formatOptions}
            selectedValue={options.output_format}
            onSelect={(value) =>
              saveOptions({ output_format: value as TranscriptOutputFormat })
            }
            disabled={store.running}
          />
        </SettingContainer>

        <ToggleSwitch
          checked={options.overwrite_existing}
          onChange={(checked) => saveOptions({ overwrite_existing: checked })}
          isUpdating={isUpdating("file_transcription")}
          label={t("settings.fileTranscription.overwrite.label")}
          description={t("settings.fileTranscription.overwrite.description")}
          descriptionMode="tooltip"
          grouped
        />
        <ToggleSwitch
          checked={options.include_subfolders}
          onChange={(checked) => saveOptions({ include_subfolders: checked })}
          isUpdating={isUpdating("file_transcription")}
          label={t("settings.fileTranscription.subfolders.label")}
          description={t("settings.fileTranscription.subfolders.description")}
          descriptionMode="tooltip"
          grouped
        />
        <Slider
          value={options.max_segment_minutes}
          onChange={(value) =>
            saveOptions({ max_segment_minutes: Math.round(value) })
          }
          min={1}
          max={60}
          step={1}
          label={t("settings.fileTranscription.segmentMinutes.label")}
          description={t(
            "settings.fileTranscription.segmentMinutes.description",
          )}
          descriptionMode="tooltip"
          grouped
          formatValue={(v) =>
            t("settings.fileTranscription.segmentMinutes.value", {
              minutes: Math.round(v),
            })
          }
          onReset={() =>
            saveOptions({ max_segment_minutes: DEFAULTS.max_segment_minutes })
          }
          disabled={store.running}
        />
      </SettingsGroup>

      <SettingsGroup title={t("settings.fileTranscription.groups.run")}>
        <div className="p-3 space-y-3">
          <div className="flex flex-wrap items-center gap-2">
            {store.running ? (
              <Button variant="danger" onClick={() => void store.cancel()}>
                <span className="inline-flex items-center gap-1.5">
                  <Square className="w-4 h-4" />
                  {t("settings.fileTranscription.cancel")}
                </span>
              </Button>
            ) : (
              <Button
                variant="primary"
                onClick={start}
                disabled={pending === 0}
              >
                <span className="inline-flex items-center gap-1.5">
                  <Play className="w-4 h-4" />
                  {t("settings.fileTranscription.start", { count: pending })}
                </span>
              </Button>
            )}
            <span className="text-sm text-mid-gray">
              {store.running
                ? t("settings.fileTranscription.running", {
                    done: finished,
                    total,
                  })
                : total > 0
                  ? t("settings.fileTranscription.summary", {
                      done,
                      failed,
                      total,
                    })
                  : t("settings.fileTranscription.errors.noFiles")}
            </span>
          </div>
          {total > 0 && (store.running || finished > 0) && (
            <ProgressBar
              progress={[{ id: "file-transcription", percentage }]}
              size="small"
            />
          )}
        </div>
      </SettingsGroup>
    </div>
  );
};
