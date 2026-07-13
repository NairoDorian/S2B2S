import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands } from "@/bindings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Button } from "../../ui/Button";
import { Alert } from "../../ui/Alert";
import type {
  LlamaRelease,
  LlamaAsset,
  DownloadedServer,
  LlamaServerConfig,
} from "@/bindings";

function assetLabel(asset: LlamaAsset): string {
  if (asset.backend.startsWith("cuda")) {
    const version = asset.backend.replace("cuda-", "");
    return `CUDA ${version}`;
  }
  if (asset.backend === "vulkan") return "Vulkan";
  if (asset.backend === "cpu") return "CPU (x64)";
  return asset.backend;
}

function assetEmoji(backend: string): string {
  if (backend.startsWith("cuda")) return "🟢";
  if (backend === "vulkan") return "🟡";
  return "⚪";
}

const LlamaCppSettings: React.FC = () => {
  const { t } = useTranslation();
  const [gpuType, setGpuType] = useState<string>("cpu");
  const [releases, setReleases] = useState<LlamaRelease[]>([]);
  const [servers, setServers] = useState<DownloadedServer[]>([]);
  const [config, setConfig] = useState<LlamaServerConfig | null>(null);
  const [loading, setLoading] = useState(false);
  const [downloading, setDownloading] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const [gpu, rels, srvs, cfg] = await Promise.all([
        commands
          .detectGpuType()
          .then((r) => (r.status === "ok" ? r.data : "cpu")),
        commands
          .fetchLlamaReleases()
          .then((r) => (r.status === "ok" ? r.data : [])),
        commands
          .getDownloadedLlamaServers()
          .then((r) => (r.status === "ok" ? r.data : [])),
        commands
          .getLlamaServerConfig()
          .then((r) => (r.status === "ok" ? r.data : null)),
      ]);
      setGpuType(gpu);
      setReleases(rels);
      setServers(srvs);
      setConfig(cfg);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const handleDownload = async (asset: LlamaAsset, releaseTag: string) => {
    setDownloading(`${asset.backend}-${releaseTag}`);
    setError(null);
    try {
      const res = await commands.downloadLlamaServer(
        asset.backend,
        releaseTag,
        asset.download_url,
      );
      if (res.status === "ok") {
        await refresh();
        // If no active server, auto-select
        if (!config?.release_tag) {
          await commands.setLlamaServerActive(asset.backend, releaseTag);
          await refresh();
        }
      } else {
        setError(res.error);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setDownloading(null);
    }
  };

  const handleSetActive = async (backend: string, releaseTag: string) => {
    try {
      await commands.setLlamaServerActive(backend, releaseTag);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleRemove = async (backend: string, releaseTag: string) => {
    try {
      await commands.removeLlamaServer(backend, releaseTag);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const isDownloaded = (backend: string, tag: string) =>
    servers.some((s) => s.backend === backend && s.release_tag === tag);

  const isActive = (backend: string, tag: string) =>
    config?.backend === backend && config?.release_tag === tag;

  const latestRelease = releases[0];

  return (
    <div className="space-y-6">
      <SettingsGroup title="Llama.cpp Server">
        {error && (
          <Alert variant="error" contained>
            {error}
          </Alert>
        )}

        {/* GPU Detection */}
        <div className="p-4 rounded-lg border border-logo-primary/10 bg-logo-primary/[0.02]">
          <p className="text-sm text-text/80 mb-1">
            {t("llamaCpp.gpuPreference")}{" "}
            <span className="font-semibold">
              {gpuType === "cuda"
                ? t("llamaCpp.gpu.cuda")
                : gpuType === "vulkan"
                  ? t("llamaCpp.gpu.vulkan")
                  : t("llamaCpp.gpu.cpu")}
            </span>
          </p>
          <p className="text-xs text-mid-gray">
            {t("llamaCpp.serverDescription")}
          </p>
        </div>

        {/* Available Servers from Latest Release */}
        {latestRelease && (
          <div className="space-y-2">
            <h4 className="text-sm font-semibold text-text">
              {t("llamaCpp.latestRelease", {
                tag: latestRelease.tag,
                name: latestRelease.name,
              })}
            </h4>
            <div className="grid gap-2">
              {latestRelease.assets.map((asset) => {
                const backendInfo = {
                  backend: asset.backend,
                  label: assetLabel(asset),
                  emoji: assetEmoji(asset.backend),
                };
                const downloaded = isDownloaded(
                  asset.backend,
                  latestRelease.tag,
                );
                const active = isActive(asset.backend, latestRelease.tag);
                const isDl =
                  downloading === `${asset.backend}-${latestRelease.tag}`;

                return (
                  <div
                    key={asset.name}
                    className={`flex items-center justify-between p-3 rounded-lg border transition-colors ${
                      active
                        ? "border-green-500/30 bg-green-500/[0.05]"
                        : "border-mid-gray/10 bg-mid-gray/[0.02]"
                    }`}
                  >
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <span>{backendInfo?.emoji}</span>
                        <span className="font-medium text-sm text-text">
                          {backendInfo?.label}
                        </span>
                        {active && (
                          <span className="text-[10px] px-2 py-0.5 bg-green-500/15 text-green-400 font-bold rounded">
                            ACTIVE
                          </span>
                        )}
                      </div>
                      <p className="text-xs text-mid-gray mt-0.5">
                        {t("llamaCpp.releaseMeta", {
                          tag: latestRelease.tag,
                          size: Math.round(asset.size_bytes / (1024 * 1024)),
                        })}
                      </p>
                    </div>
                    <div className="flex items-center gap-2 ml-3">
                      {!active && (
                        <Button
                          variant={downloaded ? "primary-soft" : "secondary"}
                          size="sm"
                          onClick={() => {
                            if (downloaded) {
                              handleSetActive(asset.backend, latestRelease.tag);
                            } else {
                              void handleDownload(
                                asset,
                                latestRelease.tag,
                              ).then(() =>
                                handleSetActive(
                                  asset.backend,
                                  latestRelease.tag,
                                ),
                              );
                            }
                          }}
                          disabled={!!downloading && !downloaded}
                        >
                          {active
                            ? "Active"
                            : downloaded
                              ? "Use"
                              : isDl
                                ? "DL..."
                                : "Use"}
                        </Button>
                      )}
                      {downloaded ? (
                        <Button
                          variant="secondary"
                          size="sm"
                          onClick={() =>
                            handleRemove(asset.backend, latestRelease.tag)
                          }
                        >
                          {t("llamaCpp.remove")}
                        </Button>
                      ) : (
                        <Button
                          variant="primary"
                          size="sm"
                          disabled={!!downloading}
                          onClick={() =>
                            handleDownload(asset, latestRelease.tag)
                          }
                        >
                          {isDl ? "Downloading..." : "Download"}
                        </Button>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {/* Installed Servers (other versions) */}
        {servers.length > 0 && (
          <div className="space-y-2">
            <h4 className="text-sm font-semibold text-text mt-4">
              {t("llamaCpp.installedServers")}
            </h4>
            <div className="grid gap-1">
              {servers.map((srv) => {
                const active = isActive(srv.backend, srv.release_tag);
                return (
                  <div
                    key={`${srv.backend}-${srv.release_tag}`}
                    className={`flex items-center justify-between p-2 rounded text-xs ${
                      active ? "bg-logo-primary/5" : ""
                    }`}
                  >
                    <span className="text-text/70">
                      {assetEmoji(srv.backend)}{" "}
                      {srv.backend.startsWith("cuda")
                        ? `CUDA ${srv.backend.replace("cuda-", "")}`
                        : srv.backend === "vulkan"
                          ? "Vulkan"
                          : "CPU (x64)"}{" "}
                      · {srv.release_tag}
                      {active && (
                        <span className="ml-2 text-green-400 font-semibold">
                          {t("llamaCpp.active")}
                        </span>
                      )}
                    </span>
                    <div className="flex gap-1">
                      {!active && (
                        <button
                          onClick={() =>
                            handleSetActive(srv.backend, srv.release_tag)
                          }
                          className="text-logo-primary hover:underline"
                        >
                          {t("llamaCpp.use")}
                        </button>
                      )}
                      <button
                        onClick={() =>
                          handleRemove(srv.backend, srv.release_tag)
                        }
                        className="text-red-400 hover:underline"
                      >
                        {t("llamaCpp.remove")}
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {!latestRelease && !loading && (
          <p className="text-xs text-mid-gray">{t("llamaCpp.noReleases")}</p>
        )}

        <Button
          variant="secondary"
          size="sm"
          onClick={refresh}
          disabled={loading}
        >
          {loading ? "Refreshing..." : "Refresh Releases"}
        </Button>
      </SettingsGroup>
    </div>
  );
};

export default LlamaCppSettings;
export { LlamaCppSettings };
