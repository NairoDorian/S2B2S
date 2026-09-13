import { createSignal, createEffect, createMemo, For } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { open } from "@tauri-apps/plugin-dialog";
import { sessionToast as toast } from "@/lib/sessionToast";
import {
  Check,
  Copy,
  ExternalLink,
  FolderOpen,
  Loader2,
  Mic,
  Radio,
  RefreshCw,
  Square,
  X,
} from "@/components/icons/lucide";
import {
  commands,
  type LiveModePhase,
  type LiveModeSettings as LiveModeSettingsType,
  type LiveSessionInfo,
  type LiveTranscriptGranularity,
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
import { useSettings } from "@/hooks/useSettings";
import { useModelStore } from "@/stores/modelStore";
import { isLiveActive, useLiveModeStore } from "@/stores/liveModeStore";
import { formatDateTime } from "@/utils/dateFormat";

const DEFAULTS: Required<LiveModeSettingsType> = {
  output_dir: null,
  chunk_minutes: 5,
  transcript_format: "txt",
  granularity: "character",
  save_audio: true,
  prefer_silence_boundary: true,
};

const formatClock = (ms: number): string => {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mm = h > 0 ? String(m).padStart(2, "0") : String(m);
  return `${h > 0 ? `${h}:` : ""}${mm}:${String(s).padStart(2, "0")}`;
};

const formatBytes = (bytes: number): string => {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
};

const PHASE_CLASSES: Record<LiveModePhase, string> = {
  idle: "bg-mid-gray/15 text-text/70 border-mid-gray/20",
  starting: "bg-accent/15 text-text border-accent/30",
  listening: "bg-green-500/15 text-green-500 border-green-500/30",
  rotating: "bg-accent/20 text-text border-accent/40",
  stopping: "bg-accent/15 text-text border-accent/30",
  error: "bg-red-500/15 text-red-400 border-red-500/30",
};

export const LiveModeSettings = () => {
  const { t, i18n } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const modelStore = useModelStore();
  const store = useLiveModeStore();
  const [copied, setCopied] = createSignal(false);
  const [now, setNow] = createSignal(Date.now());
  let previewRef: HTMLDivElement | undefined;

  const options = createMemo<Required<LiveModeSettingsType>>(() => ({
    ...DEFAULTS,
    ...getSetting("live_mode"),
  }));

  const saveOptions = (patch: Partial<LiveModeSettingsType>) =>
    updateSetting("live_mode", { ...options(), ...patch });

  createEffect(
    () => undefined,
    () => {
      void store.initialize();
    },
  );

  const active = () => isLiveActive(store.status);
  const phase = () => store.status.phase;

  createEffect(
    () => active(),
    (isActive) => {
      if (!isActive) return;
      const id = setInterval(() => setNow(Date.now()), 500);
      return () => clearInterval(id);
    },
  );

  createEffect(
    () => [store.stable, store.live, active()],
    () => {
      const el = previewRef;
      if (el && active()) el.scrollTop = el.scrollHeight;
    },
  );

  const modelInfo = createMemo(() => {
    const id = modelStore.currentModel;
    const models = modelStore.models;
    return id ? models.find((model) => model.id === id) : undefined;
  });
  const modelSupportsStreaming = () => modelInfo()?.supports_streaming ?? false;
  const modelLabel = () => modelInfo()?.name ?? modelStore.currentModel;

  const elapsedMs = () =>
    active() && store.status.started_at_ms != null
      ? Math.max(0, now() - store.status.started_at_ms)
      : (store.status.elapsed_ms ?? 0);

  const start = async () => {
    const error = await store.start();
    if (error) toast.error(error);
  };
  const stop = async () => {
    const error = await store.stop();
    if (error) toast.error(error);
  };

  const reveal = async (path: string) => {
    const result = await commands.revealPathInFileManager(path);
    if (result.status === "error") toast.error(result.error);
  };

  const previewText = () =>
    store.viewing ? store.viewingText : `${store.stable}${store.live}`;
  const copy = async () => {
    if (!previewText()) return;
    try {
      await navigator.clipboard.writeText(previewText());
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch (error) {
      console.error("Copy failed:", error);
    }
  };

  const pickOutputFolder = async () => {
    const selection = await open({ directory: true, multiple: false });
    if (!selection || Array.isArray(selection)) return;
    void saveOptions({ output_dir: selection });
  };

  const formatOptions = createMemo<DropdownOption[]>(() => [
    { value: "txt", label: t("settings.liveMode.format.txt") },
    { value: "md", label: t("settings.liveMode.format.md") },
  ]);
  const granularityOptions = createMemo<DropdownOption[]>(() => [
    {
      value: "character",
      label: t("settings.liveMode.granularity.options.character.label"),
      description: t(
        "settings.liveMode.granularity.options.character.description",
      ),
    },
    {
      value: "word",
      label: t("settings.liveMode.granularity.options.word.label"),
      description: t("settings.liveMode.granularity.options.word.description"),
    },
  ]);

  const transcriptPath = () =>
    store.viewing
      ? store.viewing.transcript_path
      : store.status.transcript_path;
  const sessionDir = () =>
    store.viewing ? store.viewing.dir : store.status.session_dir;

  return (
    <div class="max-w-3xl w-full mx-auto space-y-6 pb-8">
      <SettingsGroup
        title={t("settings.liveMode.title")}
        description={t("settings.liveMode.description")}
      >
        <div class="p-3 space-y-3">
          {!modelSupportsStreaming() && (
            <Alert variant="warning">
              {modelStore.currentModel
                ? t("settings.liveMode.model.notStreaming", {
                    model: modelLabel(),
                  })
                : t("settings.liveMode.model.none")}
            </Alert>
          )}
          {phase() === "error" && store.status.error && (
            <Alert variant="error">{store.status.error}</Alert>
          )}
          <div class="flex flex-wrap items-center gap-3 rounded-lg border border-mid-gray/20 bg-background p-3">
            <div
              class={`p-3 rounded-full ${active() ? "bg-green-500/15" : "bg-mid-gray/10"}`}
            >
              {phase() === "listening" ? (
                <Mic class="w-6 h-6 text-green-500 animate-pulse" />
              ) : active() ? (
                <Loader2 class="w-6 h-6 text-accent animate-spin" />
              ) : (
                <Radio class="w-6 h-6 text-mid-gray" />
              )}
            </div>
            <div class="flex-1 min-w-[180px]">
              <div class="flex items-center gap-2">
                <span
                  class={`inline-flex items-center px-2 py-0.5 rounded text-[11px] font-medium border ${PHASE_CLASSES[phase()]}`}
                >
                  {t(`settings.liveMode.status.${phase()}`)}
                </span>
                {modelLabel() && (
                  <span class="text-xs text-mid-gray truncate">
                    {t("settings.liveMode.model.label", {
                      model: modelLabel(),
                    })}
                  </span>
                )}
              </div>
              <dl class="mt-2 grid grid-cols-3 gap-2 text-xs">
                <div>
                  <dt class="text-mid-gray">
                    {t("settings.liveMode.stats.elapsed")}
                  </dt>
                  <dd class="font-mono text-sm">{formatClock(elapsedMs())}</dd>
                </div>
                <div>
                  <dt class="text-mid-gray">
                    {t("settings.liveMode.stats.chunk", {
                      index: store.status.chunk_index,
                    })}
                  </dt>
                  <dd class="font-mono text-sm">
                    {formatClock(store.status.current_chunk_ms ?? 0)}
                  </dd>
                </div>
                <div>
                  <dt class="text-mid-gray">
                    {t("settings.liveMode.stats.speech")}
                  </dt>
                  <dd class="font-mono text-sm">
                    {formatClock(store.status.current_chunk_speech_ms ?? 0)}
                  </dd>
                </div>
              </dl>
            </div>
            {active() ? (
              <Button
                variant="danger"
                onClick={stop}
                disabled={phase() === "stopping"}
              >
                <span class="inline-flex items-center gap-1.5">
                  <Square class="w-4 h-4" />
                  {t("settings.liveMode.stop")}
                </span>
              </Button>
            ) : (
              <Button
                variant="primary"
                onClick={start}
                disabled={!modelSupportsStreaming()}
              >
                <span class="inline-flex items-center gap-1.5">
                  <Mic class="w-4 h-4" />
                  {t("settings.liveMode.start")}
                </span>
              </Button>
            )}
          </div>
        </div>
      </SettingsGroup>

      <SettingsGroup title={t("settings.liveMode.transcript.title")}>
        <div class="p-3 space-y-2">
          <div class="flex flex-wrap items-center gap-2 text-xs text-mid-gray">
            {store.viewing ? (
              <span class="inline-flex items-center gap-1">
                {t("settings.liveMode.transcript.viewing", {
                  name: store.viewing.name,
                })}
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={store.closeViewer}
                  aria-label={t("settings.liveMode.transcript.closeViewer")}
                  title={t("settings.liveMode.transcript.closeViewer")}
                >
                  <X class="w-3 h-3" />
                </Button>
              </span>
            ) : (
              <span
                class="font-mono truncate max-w-full"
                title={transcriptPath() ?? undefined}
              >
                {transcriptPath() ?? t("settings.liveMode.transcript.noFile")}
              </span>
            )}
            <div class="flex-1" />
            <Button
              variant="ghost"
              size="sm"
              onClick={copy}
              disabled={!previewText()}
            >
              {copied() ? (
                <span class="inline-flex items-center gap-1">
                  <Check class="w-3 h-3" />
                  {t("settings.liveMode.transcript.copied")}
                </span>
              ) : (
                <span class="inline-flex items-center gap-1">
                  <Copy class="w-3 h-3" />
                  {t("settings.liveMode.transcript.copy")}
                </span>
              )}
            </Button>
            {sessionDir() && (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  const dir = sessionDir();
                  if (dir) void reveal(dir);
                }}
              >
                <span class="inline-flex items-center gap-1">
                  <ExternalLink class="w-3 h-3" />
                  {t("settings.liveMode.transcript.reveal")}
                </span>
              </Button>
            )}
          </div>
          <div
            ref={(el) => {
              previewRef = el;
            }}
            class="h-64 overflow-y-auto rounded-lg border border-mid-gray/20 bg-background p-3 text-sm leading-relaxed whitespace-pre-wrap break-words font-mono"
          >
            {previewText() ? (
              store.viewing ? (
                store.viewingText
              ) : (
                <>
                  {store.stable}
                  {store.live && <span class="text-accent">{store.live}</span>}
                  {phase() === "listening" && (
                    <span class="text-accent animate-pulse">{"▍"}</span>
                  )}
                </>
              )
            ) : (
              <span class="text-mid-gray/70">
                {t("settings.liveMode.transcript.empty")}
              </span>
            )}
          </div>
          {!store.viewing && store.status.chunks.length > 0 && (
            <details class="text-xs">
              <summary class="cursor-pointer text-mid-gray">
                {t("settings.liveMode.chunks.title", {
                  count: store.status.chunks.length,
                })}
              </summary>
              <ul class="mt-2 max-h-40 overflow-y-auto rounded-md border border-mid-gray/20 divide-y divide-mid-gray/15">
                <For each={store.status.chunks}>
                  {(chunk) => (
                    <li class="flex items-center gap-3 px-3 py-1.5">
                      <span class="font-mono w-10 shrink-0">{`#${chunk.index}`}</span>
                      <span class="flex-1 truncate text-mid-gray">
                        {t("settings.liveMode.chunks.item", {
                          duration: formatClock(chunk.duration_ms ?? 0),
                          size: formatBytes(chunk.bytes ?? 0),
                          chars: chunk.text_chars,
                        })}
                      </span>
                      {chunk.path && (
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => reveal(chunk.path as string)}
                          aria-label={t("settings.liveMode.transcript.reveal")}
                          title={t("settings.liveMode.transcript.reveal")}
                        >
                          <ExternalLink class="w-3 h-3" />
                        </Button>
                      )}
                    </li>
                  )}
                </For>
              </ul>
            </details>
          )}
        </div>
      </SettingsGroup>

      <SettingsGroup title={t("settings.liveMode.settings.title")}>
        <SettingContainer
          title={t("settings.liveMode.outputFolder.title")}
          description={t("settings.liveMode.outputFolder.description")}
          descriptionMode="tooltip"
          grouped
          layout="stacked"
        >
          <div class="flex flex-wrap items-center gap-2 w-full">
            <span
              class="flex-1 min-w-0 text-xs font-mono truncate px-2 py-1.5 rounded-md bg-background border border-mid-gray/20"
              title={
                options().output_dir ?? store.defaultOutputDir ?? undefined
              }
            >
              {options().output_dir ??
                store.defaultOutputDir ??
                t("settings.liveMode.outputFolder.default")}
            </span>
            <Button
              variant="secondary"
              size="sm"
              onClick={pickOutputFolder}
              disabled={active()}
            >
              <span class="inline-flex items-center gap-1.5">
                <FolderOpen class="w-4 h-4" />
                {t("settings.liveMode.outputFolder.choose")}
              </span>
            </Button>
            {options().output_dir && (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => saveOptions({ output_dir: null })}
                disabled={active()}
              >
                {t("settings.liveMode.outputFolder.reset")}
              </Button>
            )}
          </div>
        </SettingContainer>
        <Slider
          value={options().chunk_minutes}
          onChange={(value) =>
            saveOptions({ chunk_minutes: Math.round(value) })
          }
          min={1}
          max={60}
          step={1}
          label={t("settings.liveMode.chunkMinutes.label")}
          description={t("settings.liveMode.chunkMinutes.description")}
          descriptionMode="tooltip"
          grouped
          formatValue={(v) =>
            t("settings.liveMode.chunkMinutes.value", {
              minutes: Math.round(v),
            })
          }
          onReset={() => saveOptions({ chunk_minutes: DEFAULTS.chunk_minutes })}
          disabled={active() || isUpdating("live_mode")}
        />
        <SettingContainer
          title={t("settings.liveMode.format.title")}
          description={t("settings.liveMode.format.description")}
          descriptionMode="tooltip"
          grouped
          layout="horizontal"
        >
          <Dropdown
            options={formatOptions()}
            selectedValue={options().transcript_format}
            onSelect={(value) =>
              saveOptions({
                transcript_format: value as TranscriptOutputFormat,
              })
            }
            disabled={active()}
          />
        </SettingContainer>
        <SettingContainer
          title={t("settings.liveMode.granularity.title")}
          description={t("settings.liveMode.granularity.description")}
          descriptionMode="tooltip"
          grouped
          layout="horizontal"
        >
          <Dropdown
            options={granularityOptions()}
            selectedValue={options().granularity}
            onSelect={(value) =>
              saveOptions({ granularity: value as LiveTranscriptGranularity })
            }
            disabled={active()}
            class="min-w-[200px]"
          />
        </SettingContainer>
        <ToggleSwitch
          checked={options().save_audio}
          onChange={(checked) => saveOptions({ save_audio: checked })}
          isUpdating={isUpdating("live_mode")}
          disabled={active()}
          label={t("settings.liveMode.saveAudio.label")}
          description={t("settings.liveMode.saveAudio.description")}
          descriptionMode="tooltip"
          grouped
        />
        <ToggleSwitch
          checked={options().prefer_silence_boundary}
          onChange={(checked) =>
            saveOptions({ prefer_silence_boundary: checked })
          }
          isUpdating={isUpdating("live_mode")}
          disabled={active()}
          label={t("settings.liveMode.silenceBoundary.label")}
          description={t("settings.liveMode.silenceBoundary.description")}
          descriptionMode="tooltip"
          grouped
        />
      </SettingsGroup>

      <SettingsGroup
        title={t("settings.liveMode.sessions.title")}
        description={t("settings.liveMode.sessions.description")}
      >
        <div class="p-3 space-y-2">
          <div class="flex items-center justify-end">
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void store.refreshSessions()}
            >
              <span class="inline-flex items-center gap-1">
                <RefreshCw class="w-3 h-3" />
                {t("settings.liveMode.sessions.refresh")}
              </span>
            </Button>
          </div>
          {store.sessions.length === 0 ? (
            <p class="text-xs text-mid-gray/70 px-1">
              {t("settings.liveMode.sessions.empty")}
            </p>
          ) : (
            <ul class="rounded-lg border border-mid-gray/20 bg-background divide-y divide-mid-gray/15 max-h-64 overflow-y-auto">
              <For each={store.sessions}>
                {(session: LiveSessionInfo) => {
                  const isCurrent = () =>
                    active() && session.dir === store.status.session_dir;
                  return (
                    <li class="flex items-center gap-3 px-3 py-2">
                      <div class="flex-1 min-w-0">
                        <p class="text-sm truncate" title={session.dir}>
                          {session.name}
                        </p>
                        <p class="text-xs text-mid-gray truncate">
                          {t("settings.liveMode.sessions.item", {
                            date: formatDateTime(
                              String(session.modified_ms ?? 0),
                              i18n.language,
                            ),
                            chunks: session.chunk_count,
                            size: formatBytes(session.transcript_bytes ?? 0),
                          })}
                        </p>
                      </div>
                      {!isCurrent() && session.transcript_path && (
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => void store.viewSession(session)}
                          disabled={active()}
                        >
                          {t("settings.liveMode.sessions.view")}
                        </Button>
                      )}
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => reveal(session.dir)}
                        aria-label={t("settings.liveMode.transcript.reveal")}
                        title={t("settings.liveMode.transcript.reveal")}
                      >
                        <ExternalLink class="w-3 h-3" />
                      </Button>
                    </li>
                  );
                }}
              </For>
            </ul>
          )}
        </div>
      </SettingsGroup>
    </div>
  );
};
