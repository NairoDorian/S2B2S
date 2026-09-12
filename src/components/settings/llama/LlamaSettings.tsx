import React, { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Download,
  FolderOpen,
  Play,
  RefreshCw,
  Square,
  Trash2,
} from "lucide-react";
import {
  commands,
  type GgufFile,
  type LlamaSettings as LlamaSettingsType,
} from "@/bindings";
import { sessionToast as toast } from "@/lib/sessionToast";
import { useSettings } from "../../../hooks/useSettings";
import { useLlamaStore } from "../../../stores/llamaStore";
import { Button } from "../../ui/Button";
import { Dropdown } from "../../ui/Dropdown";
import { Input } from "../../ui/Input";
import { SettingContainer } from "../../ui/SettingContainer";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Textarea } from "../../ui/Textarea";
import { ToggleSwitch } from "../../ui/ToggleSwitch";

const BACKENDS = ["auto", "cuda-13.3", "cuda-12.4", "vulkan", "cpu"] as const;
const CHANNELS = ["latest", "stable", "nightly"] as const;
const LOG_POLL_MS = 1000;

/** `LlamaSettings` with every field present and numbers non-null. */
type Cfg = {
  server_dir: string | null;
  model_path: string | null;
  draft_model_path: string | null;
  mmproj_path: string | null;
  mmproj_enabled: boolean;
  port: number;
  context_size: number;
  gpu_layers: number;
  threads: number;
  flash_attn: boolean;
  reasoning: boolean;
  temperature: number;
  top_p: number;
  top_k: number;
  min_p: number;
  spec_draft_n_max: number;
  alias: string;
  extra_args: string;
  custom_args: string | null;
  attn_rot_disable: boolean;
  autostart: boolean;
  start_on_demand: boolean;
  stop_on_exit: boolean;
  backend: string;
  channel: string;
  include_cudart: boolean;
};

const NULLABLE_KEYS = new Set([
  "server_dir",
  "model_path",
  "draft_model_path",
  "mmproj_path",
  "custom_args",
]);

/** Fill gaps and nulls from the saved settings with the script defaults. */
const resolve = (saved: LlamaSettingsType | null | undefined): Cfg => {
  const out: Record<string, unknown> = { ...DEFAULTS };
  if (saved) {
    for (const [key, value] of Object.entries(saved)) {
      if (value === undefined) continue;
      if (value === null && !NULLABLE_KEYS.has(key)) continue;
      out[key] = value;
    }
  }
  return out as Cfg;
};

const DEFAULTS: Cfg = {
  server_dir: null,
  model_path: null,
  draft_model_path: null,
  mmproj_path: null,
  mmproj_enabled: false,
  // Keep in step with DEFAULT_LLAMA_PORT (settings.rs): registered range, not
  // ephemeral — Windows reserves blocks above 49152 that nothing can bind.
  port: 18080,
  context_size: 8192,
  gpu_layers: -1,
  threads: -1,
  flash_attn: true,
  reasoning: false,
  temperature: 0.05,
  top_p: 0.35,
  top_k: 64,
  min_p: 0,
  spec_draft_n_max: 4,
  alias: "gemma-4-E2B-Q4-MTP",
  extra_args: "",
  custom_args: null,
  attn_rot_disable: true,
  autostart: false,
  start_on_demand: true,
  stop_on_exit: true,
  backend: "auto",
  channel: "latest",
  include_cudart: false,
};

const formatMb = (bytes: number) => `${(bytes / 1_048_576).toFixed(0)} MB`;

const dirOf = (path: string | null | undefined): string | null => {
  if (!path) return null;
  const idx = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return idx > 0 ? path.slice(0, idx) : null;
};

/**
 * Settings → Local LLM (llama.cpp): install a llama.cpp release, pick the
 * model / MTP draft / mmproj files, tune the server command line, and start,
 * stop or restart the supervised server with its log in view. The command
 * preview is the exact line that will run — the same one the maintainer's
 * launch script used to run by hand.
 */
export const LlamaSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const store = useLlamaStore();
  const saved = getSetting("llama") as LlamaSettingsType | null | undefined;
  const cfg: Cfg = useMemo(() => resolve(saved), [saved]);
  const [showLogs, setShowLogs] = useState(true);
  const [ggufFiles, setGgufFiles] = useState<GgufFile[]>([]);

  const save = useCallback(
    (patch: Partial<Cfg>) => updateSetting("llama", { ...cfg, ...patch }),
    [cfg, updateSetting],
  );

  // One subscription for the whole app; the page pulls what it shows.
  useEffect(() => {
    void store.initialize();
    void store.refreshInstalled();
    void store.fetchReleases(cfg.channel, false);
  }, []);

  useEffect(() => {
    void store.refreshPreview();
  }, [cfg]);

  // Logs are polled only while this page shows them.
  useEffect(() => {
    if (!showLogs) return;
    void store.refreshLogs();
    const id = window.setInterval(() => void store.refreshLogs(), LOG_POLL_MS);
    return () => window.clearInterval(id);
  }, [showLogs, store.state?.status]);

  // GGUF files next to the chosen model, for the pickers.
  const modelDir = dirOf(cfg.model_path);
  useEffect(() => {
    if (!modelDir) {
      setGgufFiles([]);
      return;
    }
    commands
      .listGgufFiles(modelDir)
      .then(setGgufFiles)
      .catch(() => setGgufFiles([]));
  }, [modelDir]);

  const pickFile = async (
    key: "model_path" | "draft_model_path" | "mmproj_path",
  ) => {
    const selection = await open({
      multiple: false,
      directory: false,
      defaultPath: modelDir ?? undefined,
      filters: [{ name: "GGUF", extensions: ["gguf"] }],
    });
    if (typeof selection === "string") {
      await save({ [key]: selection } as Partial<Cfg>);
    }
  };

  const pickServerDir = async () => {
    const selection = await open({ directory: true, multiple: false });
    if (typeof selection === "string") {
      await save({ server_dir: selection });
    }
  };

  const detectExisting = async () => {
    const found = await commands.detectLlamaInstall();
    if (!found) {
      toast.error(t("settings.llama.backend.detectNone"));
      return;
    }
    await save({
      server_dir: found.server_dir,
      model_path: found.model_path ?? cfg.model_path,
      draft_model_path: found.draft_model_path ?? cfg.draft_model_path,
      mmproj_path: found.mmproj_path ?? cfg.mmproj_path,
    });
    toast.success(
      t("settings.llama.backend.detectFound", { dir: found.server_dir }),
    );
  };

  const useForPostProcessing = async () => {
    const result = await commands.applyLlamaToPostProcessing();
    if (result.status === "error") {
      toast.error(String(result.error));
      return;
    }
    toast.success(t("settings.llama.server.appliedToast", { port: cfg.port }));
  };

  const status = store.state?.status ?? "stopped";
  const running = status === "ready" || status === "starting";
  const download = store.download;
  const downloading =
    download !== null &&
    (download.phase === "downloading" || download.phase === "extracting");
  const downloadedBytes = download?.downloaded_bytes ?? 0;
  const totalBytes = download?.total_bytes ?? 0;
  const busy = isUpdating("llama");
  const toolkit = store.cudaToolkit;
  // One release only: the newest of the channel that ships a build for the
  // wanted backend (releases without binaries point at a backing tag).
  const wantedBackend =
    cfg.backend === "auto" ? store.detectedBackend : cfg.backend;
  const latestRelease =
    store.releases.find((r) =>
      r.assets.some((a) => a.backend === wantedBackend),
    ) ??
    store.releases.find((r) => r.assets.length > 0) ??
    store.releases[0];
  const latestAsset = latestRelease
    ? (latestRelease.assets.find((a) => a.backend === wantedBackend) ??
      latestRelease.assets[0])
    : undefined;
  const latestInstalled =
    !!latestRelease && store.installed.some((i) => i.tag === latestRelease.tag);
  const cudaToolkitDescription = !toolkit
    ? t("settings.llama.backend.includeCudartMissing")
    : [
        t("settings.llama.backend.includeCudartFound", {
          dir: toolkit.runtime_dir,
          version: toolkit.version ?? "?",
        }),
        toolkit.has_cublas
          ? ""
          : t("settings.llama.backend.includeCudartNoCublas"),
        toolkit.on_path
          ? t("settings.llama.backend.includeCudartOnPath")
          : t("settings.llama.backend.includeCudartOffPath"),
      ]
        .filter(Boolean)
        .join(" ");

  const optionsFor = (kind: string, allowNone: boolean) => [
    ...(allowNone
      ? [{ value: "", label: t("settings.llama.model.none") }]
      : []),
    ...ggufFiles
      .filter((f) => f.kind === kind)
      .map((f) => ({ value: f.path, label: `${f.name} · ${f.size_mb} MB` })),
  ];

  const statusColor =
    status === "ready"
      ? "bg-emerald-400"
      : status === "starting"
        ? "bg-warning animate-pulse"
        : status === "error"
          ? "bg-error"
          : "bg-mid-gray/60";

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      {/* ---- Server control ---- */}
      <SettingsGroup
        title={t("settings.llama.server.title")}
        description={t("settings.llama.server.description")}
      >
        <div className="px-4 py-3 flex flex-wrap items-center gap-3">
          <span className={`inline-block w-2.5 h-2.5 ${statusColor}`} />
          <span className="text-sm font-medium text-text">
            {t(`settings.llama.status.${status}`)}
          </span>
          <span className="text-xs text-text/60 font-mono">
            {store.state?.model ?? t("settings.llama.model.none")}
            {store.state?.draft ? " +MTP" : ""}
            {store.state?.mmproj ? " +mmproj" : ""}
          </span>
          <span className="text-xs text-text/50 font-mono">
            {`http://127.0.0.1:${cfg.port}/v1`}
          </span>
          <div className="ms-auto flex items-center gap-2">
            {!running ? (
              <Button
                size="sm"
                variant="primary-soft"
                onClick={() => void store.start()}
              >
                <span className="inline-flex items-center gap-1.5">
                  <Play className="w-3.5 h-3.5" />
                  {t("settings.llama.server.start")}
                </span>
              </Button>
            ) : (
              <Button
                size="sm"
                variant="danger-ghost"
                onClick={() => void store.stop()}
              >
                <span className="inline-flex items-center gap-1.5">
                  <Square className="w-3.5 h-3.5" />
                  {t("settings.llama.server.stop")}
                </span>
              </Button>
            )}
            <Button
              size="sm"
              variant="secondary"
              onClick={() => void store.restart()}
            >
              <span className="inline-flex items-center gap-1.5">
                <RefreshCw className="w-3.5 h-3.5" />
                {t("settings.llama.server.restart")}
              </span>
            </Button>
          </div>
        </div>
        {store.state?.message && (
          <div className="px-4 py-2 text-xs text-text/70 whitespace-pre-wrap font-mono border-t border-mid-gray/20">
            {store.state.message}
          </div>
        )}
        <SettingContainer
          title={t("settings.llama.server.useForPostProcessing")}
          description={t(
            "settings.llama.server.useForPostProcessingDescription",
          )}
          grouped={true}
          layout="horizontal"
        >
          <Button size="sm" variant="secondary" onClick={useForPostProcessing}>
            {t("settings.llama.server.apply")}
          </Button>
        </SettingContainer>
        <ToggleSwitch
          checked={cfg.start_on_demand}
          onChange={(v) => save({ start_on_demand: v })}
          isUpdating={busy}
          label={t("settings.llama.server.startOnDemand")}
          description={t("settings.llama.server.startOnDemandDescription")}
          grouped={true}
        />
        <ToggleSwitch
          checked={cfg.autostart}
          onChange={(v) => save({ autostart: v })}
          isUpdating={busy}
          label={t("settings.llama.server.autostart")}
          description={t("settings.llama.server.autostartDescription")}
          grouped={true}
        />
        <ToggleSwitch
          checked={cfg.stop_on_exit}
          onChange={(v) => save({ stop_on_exit: v })}
          isUpdating={busy}
          label={t("settings.llama.server.stopOnExit")}
          description={t("settings.llama.server.stopOnExitDescription")}
          grouped={true}
        />
        <SettingContainer
          title={t("settings.llama.server.logs")}
          description={t("settings.llama.server.logsDescription")}
          grouped={true}
          layout="stacked"
        >
          <div className="flex items-center justify-between mb-2">
            <Button
              size="sm"
              variant="ghost"
              onClick={() => setShowLogs((s) => !s)}
            >
              {showLogs
                ? t("settings.llama.server.hideLogs")
                : t("settings.llama.server.showLogs")}
            </Button>
          </div>
          {showLogs && (
            <pre className="max-h-64 overflow-auto bg-mid-gray/10 border border-mid-gray/20 p-2 text-[11px] leading-snug font-mono text-text/80 whitespace-pre-wrap select-text cursor-text">
              {store.logs.length
                ? store.logs.join("\n")
                : t("settings.llama.server.noLogs")}
            </pre>
          )}
        </SettingContainer>
      </SettingsGroup>

      {/* ---- Backend (llama.cpp binaries) ---- */}
      <SettingsGroup
        title={t("settings.llama.backend.title")}
        description={t("settings.llama.backend.description")}
      >
        <SettingContainer
          title={t("settings.llama.backend.serverDir")}
          description={t("settings.llama.backend.serverDirDescription")}
          grouped={true}
          layout="stacked"
        >
          <div className="flex flex-wrap items-center gap-2">
            <Input
              variant="compact"
              className="flex-1 min-w-[16rem] font-mono text-xs"
              value={cfg.server_dir ?? ""}
              onChange={(e) => save({ server_dir: e.target.value || null })}
              placeholder={t("settings.llama.backend.serverDirPlaceholder")}
            />
            <Button size="sm" variant="secondary" onClick={pickServerDir}>
              <FolderOpen className="w-3.5 h-3.5" />
            </Button>
            <Button size="sm" variant="secondary" onClick={detectExisting}>
              {t("settings.llama.backend.detect")}
            </Button>
          </div>
        </SettingContainer>
        <SettingContainer
          title={t("settings.llama.backend.preferred")}
          description={t("settings.llama.backend.preferredDescription", {
            detected: store.detectedBackend ?? "…",
          })}
          grouped={true}
          layout="horizontal"
        >
          <div className="flex items-center gap-2">
            <Dropdown
              options={BACKENDS.map((b) => ({ value: b, label: b }))}
              selectedValue={cfg.backend}
              onSelect={(v) => save({ backend: v })}
            />
            <Dropdown
              options={CHANNELS.map((c) => ({
                value: c,
                label: t(`settings.llama.backend.channel.${c}`),
              }))}
              selectedValue={cfg.channel}
              onSelect={(v) => {
                void save({ channel: v });
                void store.fetchReleases(v, false);
              }}
            />
          </div>
        </SettingContainer>
        <ToggleSwitch
          checked={cfg.include_cudart}
          onChange={(v) => save({ include_cudart: v })}
          isUpdating={busy}
          label={t("settings.llama.backend.includeCudart")}
          description={cudaToolkitDescription}
          grouped={true}
        />
        <SettingContainer
          title={t("settings.llama.backend.releases")}
          description={t("settings.llama.backend.releasesDescription")}
          grouped={true}
          layout="stacked"
        >
          <div className="flex items-center justify-between mb-2">
            <span className="text-xs text-text/60">
              {store.releasesLoading
                ? t("settings.llama.backend.loading")
                : (store.releasesError ?? "")}
            </span>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => void store.fetchReleases(cfg.channel, true)}
            >
              <RefreshCw className="w-3.5 h-3.5" />
            </Button>
          </div>
          {downloading && download && (
            <div className="mb-2">
              <div className="flex justify-between text-xs text-text/70 mb-1">
                <span>
                  {download.phase === "extracting"
                    ? t("settings.llama.backend.extracting")
                    : t("settings.llama.backend.downloading")}
                </span>
                <span className="font-mono">
                  {totalBytes > 0
                    ? `${formatMb(downloadedBytes)} / ${formatMb(totalBytes)}`
                    : formatMb(downloadedBytes)}
                </span>
              </div>
              <div className="h-1.5 bg-mid-gray/20 overflow-hidden">
                <div
                  className="h-full bg-logo-primary transition-[width]"
                  style={{
                    width:
                      totalBytes > 0
                        ? `${Math.min(100, (downloadedBytes / totalBytes) * 100)}%`
                        : "100%",
                  }}
                />
              </div>
            </div>
          )}
          <ul className="divide-y divide-mid-gray/20 border border-mid-gray/20">
            {latestRelease && (
              <li
                key={latestRelease.tag}
                className="flex items-center gap-3 px-3 py-2 text-xs"
              >
                <span className="font-mono text-text">{latestRelease.tag}</span>
                <span className="text-text/50 truncate flex-1">
                  {latestRelease.published_at.slice(0, 10)}
                  {latestAsset
                    ? ` · ${latestAsset.backend} · ${formatMb(latestAsset.size_bytes ?? 0)}`
                    : ""}
                  {latestInstalled
                    ? ` · ${t("settings.llama.backend.alreadyInstalled")}`
                    : ""}
                </span>
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={downloading || latestInstalled}
                  onClick={() =>
                    void store.install(
                      latestRelease.tag,
                      cfg.backend,
                      cfg.include_cudart,
                    )
                  }
                >
                  <span className="inline-flex items-center gap-1.5">
                    <Download className="w-3.5 h-3.5" />
                    {t("settings.llama.backend.install")}
                  </span>
                </Button>
              </li>
            )}
            {!store.releasesLoading && !latestRelease && (
              <li className="px-3 py-2 text-xs text-text/50">
                {t("settings.llama.backend.noReleases")}
              </li>
            )}
          </ul>
        </SettingContainer>
        <SettingContainer
          title={t("settings.llama.backend.installed")}
          description={t("settings.llama.backend.installedDescription")}
          grouped={true}
          layout="stacked"
        >
          <ul className="divide-y divide-mid-gray/20 border border-mid-gray/20">
            {store.installed.map((inst) => {
              const active = cfg.server_dir === inst.dir;
              return (
                <li
                  key={inst.dir}
                  className="flex items-center gap-3 px-3 py-2 text-xs"
                >
                  <span
                    className={`font-mono ${active ? "text-logo-primary" : "text-text"}`}
                  >
                    {inst.name}
                  </span>
                  <span className="text-text/50 flex-1">
                    {`${inst.size_mb} MB`}
                    {inst.cuda_runtime_mb > 0
                      ? ` · ${t("settings.llama.backend.bundledRuntime", { mb: inst.cuda_runtime_mb })}`
                      : ""}
                    {inst.has_server
                      ? ""
                      : ` · ${t("settings.llama.backend.missingServer")}`}
                  </span>
                  {inst.cuda_runtime_mb > 0 && (
                    <Button
                      size="sm"
                      variant="secondary"
                      title={t("settings.llama.backend.removeRuntimeTitle")}
                      onClick={() => void store.removeCudaRuntime(inst.dir)}
                    >
                      {t("settings.llama.backend.removeRuntime")}
                    </Button>
                  )}
                  {!active && (
                    <Button
                      size="sm"
                      variant="secondary"
                      onClick={() => save({ server_dir: inst.dir })}
                    >
                      {t("settings.llama.backend.use")}
                    </Button>
                  )}
                  {active && (
                    <span className="text-[10px] uppercase tracking-wider text-logo-primary">
                      {t("settings.llama.backend.active")}
                    </span>
                  )}
                  <Button
                    size="sm"
                    variant="danger-ghost"
                    disabled={active}
                    onClick={() => void store.removeInstalled(inst.dir)}
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                  </Button>
                </li>
              );
            })}
            {store.installed.length === 0 && (
              <li className="px-3 py-2 text-xs text-text/50">
                {t("settings.llama.backend.noneInstalled")}
              </li>
            )}
          </ul>
        </SettingContainer>
      </SettingsGroup>

      {/* ---- Model files ---- */}
      <SettingsGroup
        title={t("settings.llama.model.title")}
        description={t("settings.llama.model.description")}
      >
        {(
          [
            ["model_path", "model", false],
            ["draft_model_path", "draft", true],
            ["mmproj_path", "mmproj", true],
          ] as const
        ).map(([key, kind, allowNone]) => (
          <SettingContainer
            key={key}
            title={t(`settings.llama.model.${kind}`)}
            description={t(`settings.llama.model.${kind}Description`)}
            grouped={true}
            layout="stacked"
          >
            <div className="flex flex-wrap items-center gap-2">
              {optionsFor(kind, allowNone).length > (allowNone ? 1 : 0) ? (
                <Dropdown
                  className="flex-1 min-w-[16rem]"
                  options={optionsFor(kind, allowNone)}
                  selectedValue={cfg[key] ?? ""}
                  onSelect={(v) => save({ [key]: v || null } as Partial<Cfg>)}
                />
              ) : (
                <Input
                  variant="compact"
                  className="flex-1 min-w-[16rem] font-mono text-xs"
                  value={cfg[key] ?? ""}
                  onChange={(e) =>
                    save({
                      [key]: e.target.value || null,
                    } as Partial<Cfg>)
                  }
                  placeholder={t("settings.llama.model.pathPlaceholder")}
                />
              )}
              <Button
                size="sm"
                variant="secondary"
                onClick={() => pickFile(key)}
              >
                <FolderOpen className="w-3.5 h-3.5" />
              </Button>
            </div>
          </SettingContainer>
        ))}
        <ToggleSwitch
          checked={cfg.mmproj_enabled}
          onChange={(v) => save({ mmproj_enabled: v })}
          isUpdating={busy}
          label={t("settings.llama.model.mmprojEnabled")}
          description={t("settings.llama.model.mmprojEnabledDescription")}
          grouped={true}
        />
      </SettingsGroup>

      {/* ---- Command line ---- */}
      <SettingsGroup
        title={t("settings.llama.command.title")}
        description={t("settings.llama.command.description")}
      >
        <SettingContainer
          title={t("settings.llama.command.preview")}
          description={t("settings.llama.command.previewDescription")}
          grouped={true}
          layout="stacked"
        >
          <pre className="bg-mid-gray/10 border border-mid-gray/20 p-2 text-[11px] font-mono text-text/80 whitespace-pre-wrap break-all select-text cursor-text">
            {store.commandError ?? store.commandPreview}
          </pre>
        </SettingContainer>
        {(
          [
            ["port", 1024, 65535, 1],
            ["context_size", 512, 262144, 512],
            ["gpu_layers", -1, 999, 1],
            ["threads", -1, 256, 1],
            ["top_k", 0, 1000, 1],
            ["spec_draft_n_max", 1, 16, 1],
          ] as const
        ).map(([key, min, max, step]) => (
          <SettingContainer
            key={key}
            title={t(`settings.llama.command.${key}`)}
            description={t(`settings.llama.command.${key}Description`)}
            grouped={true}
            layout="horizontal"
          >
            <Input
              type="number"
              variant="compact"
              className="w-28 font-mono"
              min={min}
              max={max}
              step={step}
              value={cfg[key]}
              onChange={(e) => {
                const n = Number(e.target.value);
                if (Number.isFinite(n)) void save({ [key]: n } as Partial<Cfg>);
              }}
            />
          </SettingContainer>
        ))}
        {(
          [
            ["temperature", 0, 2, 0.01],
            ["top_p", 0, 1, 0.01],
            ["min_p", 0, 1, 0.01],
          ] as const
        ).map(([key, min, max, step]) => (
          <SettingContainer
            key={key}
            title={t(`settings.llama.command.${key}`)}
            description={t(`settings.llama.command.${key}Description`)}
            grouped={true}
            layout="horizontal"
          >
            <Input
              type="number"
              variant="compact"
              className="w-28 font-mono"
              min={min}
              max={max}
              step={step}
              value={cfg[key] ?? 0}
              onChange={(e) => {
                const n = Number(e.target.value);
                if (Number.isFinite(n)) void save({ [key]: n } as Partial<Cfg>);
              }}
            />
          </SettingContainer>
        ))}
        <SettingContainer
          title={t("settings.llama.command.alias")}
          description={t("settings.llama.command.aliasDescription")}
          grouped={true}
          layout="horizontal"
        >
          <Input
            variant="compact"
            className="w-56 font-mono"
            value={cfg.alias}
            onChange={(e) => save({ alias: e.target.value })}
          />
        </SettingContainer>
        <ToggleSwitch
          checked={cfg.flash_attn}
          onChange={(v) => save({ flash_attn: v })}
          isUpdating={busy}
          label={t("settings.llama.command.flashAttn")}
          description={t("settings.llama.command.flashAttnDescription")}
          grouped={true}
        />
        <ToggleSwitch
          checked={cfg.reasoning}
          onChange={(v) => save({ reasoning: v })}
          isUpdating={busy}
          label={t("settings.llama.command.reasoning")}
          description={t("settings.llama.command.reasoningDescription")}
          grouped={true}
        />
        <ToggleSwitch
          checked={cfg.attn_rot_disable}
          onChange={(v) => save({ attn_rot_disable: v })}
          isUpdating={busy}
          label={t("settings.llama.command.attnRot")}
          description={t("settings.llama.command.attnRotDescription")}
          grouped={true}
        />
        <SettingContainer
          title={t("settings.llama.command.extraArgs")}
          description={t("settings.llama.command.extraArgsDescription")}
          grouped={true}
          layout="stacked"
        >
          <Input
            variant="compact"
            className="w-full font-mono text-xs"
            value={cfg.extra_args}
            onChange={(e) => save({ extra_args: e.target.value })}
            placeholder="--cache-type-k q8_0 --batch-size 512"
          />
        </SettingContainer>
        <SettingContainer
          title={t("settings.llama.command.customArgs")}
          description={t("settings.llama.command.customArgsDescription")}
          grouped={true}
          layout="stacked"
        >
          <Textarea
            variant="compact"
            className="w-full font-mono text-xs"
            value={cfg.custom_args ?? ""}
            onChange={(e) => save({ custom_args: e.target.value || null })}
            placeholder={store.commandPreview.replace(/^\S+\s/, "")}
          />
        </SettingContainer>
      </SettingsGroup>
    </div>
  );
};
