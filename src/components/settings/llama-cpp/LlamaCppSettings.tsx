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
  if (asset.backend === "cuda") {
    // Extract CUDA version from name: "llama-b9601-bin-win-cuda-12.4-x64.zip" -> "12"
    const match = asset.name.match(/cuda[_-](\d+)\.(\d+)/i);
    if (match) return `CUDA ${match[1]}.${match[2]}`;
    return "CUDA";
  }
  if (asset.backend === "vulkan") return "Vulkan";
  if (asset.backend === "cpu") return "CPU";
  return asset.backend;
}

function assetEmoji(backend: string): string {
  if (backend === "cuda") return "🟢";
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
        commands.detectGpuType().then((r) => (r.status === "ok" ? r.data : "cpu")),
        commands.fetchLlamaReleases().then((r) => (r.status === "ok" ? r.data : [])),
        commands.getDownloadedLlamaServers().then((r) => (r.status === "ok" ? r.data : [])),
        commands.getLlamaServerConfig().then((r) => (r.status === "ok" ? r.data : null)),
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
      const res = await commands.downloadLlamaServer(asset.backend, releaseTag, asset.download_url);
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
            Detected GPU preference:{" "}
            <span className="font-semibold">
              {gpuType === "cuda" ? "🟢 NVIDIA CUDA" : gpuType === "vulkan" ? "🟡 Vulkan" : "⚪ CPU-only"}
            </span>
          </p>
          <p className="text-xs text-mid-gray">
            Pre-compiled llama.cpp server binaries are downloaded from GitHub releases.
            Select a backend and download the server to enable local LLM inference.
          </p>
        </div>

        {/* Available Servers from Latest Release */}
        {latestRelease && (
          <div className="space-y-2">
            <h4 className="text-sm font-semibold text-text">
              Latest Release: {latestRelease.tag} ({latestRelease.name})
            </h4>
            <div className="grid gap-2">
              {latestRelease.assets.map((asset) => {
                const backendInfo = { backend: asset.backend, label: assetLabel(asset), emoji: assetEmoji(asset.backend) };
                const downloaded = isDownloaded(asset.backend, latestRelease.tag);
                const active = isActive(asset.backend, latestRelease.tag);
                const isDl = downloading === `${asset.backend}-${latestRelease.tag}`;

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
                        v{latestRelease.tag} · {Math.round(asset.size_bytes / (1024 * 1024))} MB
                      </p>
                    </div>
                    <div className="flex items-center gap-2 ml-3">
                      {downloaded ? (
                        <>
                          {!active && (
                            <Button
                              variant="primary-soft"
                              size="sm"
                              onClick={() => handleSetActive(asset.backend, latestRelease.tag)}
                            >
                              Use
                            </Button>
                          )}
                          <Button
                            variant="secondary"
                            size="sm"
                            onClick={() => handleRemove(asset.backend, latestRelease.tag)}
                          >
                            Remove
                          </Button>
                        </>
                      ) : (
                        <Button
                          variant="primary"
                          size="sm"
                          disabled={!!downloading}
                          onClick={() => handleDownload(asset, latestRelease.tag)}
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
            <h4 className="text-sm font-semibold text-text mt-4">Installed Servers</h4>
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
                      {srv.backend === "cuda" ? "CUDA" : srv.backend === "vulkan" ? "Vulkan" : "CPU"} · {srv.release_tag}
                      {active && <span className="ml-2 text-green-400 font-semibold">(active)</span>}
                    </span>
                    <div className="flex gap-1">
                      {!active && (
                        <button
                          onClick={() => handleSetActive(srv.backend, srv.release_tag)}
                          className="text-logo-primary hover:underline"
                        >
                          Use
                        </button>
                      )}
                      <button
                        onClick={() => handleRemove(srv.backend, srv.release_tag)}
                        className="text-red-400 hover:underline"
                      >
                        Remove
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {!latestRelease && !loading && (
          <p className="text-xs text-mid-gray">No releases found. Check your internet connection.</p>
        )}

        <Button variant="secondary" size="sm" onClick={refresh} disabled={loading}>
          {loading ? "Refreshing..." : "Refresh Releases"}
        </Button>
      </SettingsGroup>
    </div>
  );
};

export default LlamaCppSettings;
export { LlamaCppSettings };
