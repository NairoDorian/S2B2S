import { useState, useEffect, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import { commands } from "@/bindings";

export interface LlamaState {
  isDownloaded: boolean;
  isDownloading: boolean;
  downloadProgress: number;
  currentFile: string;
  downloadSpeed: number;
  error: string | null;
  startDownload: () => Promise<void>;
  refreshStatus: () => Promise<void>;
}

interface DownloadProgressPayload {
  status: string;
  file: string;
  percentage: number;
  speed_mbps: number;
  error: string | null;
}

export const useLlamaState = (): LlamaState => {
  const [isDownloaded, setIsDownloaded] = useState<boolean>(true);
  const [isDownloading, setIsDownloading] = useState<boolean>(false);
  const [downloadProgress, setDownloadProgress] = useState<number>(0);
  const [currentFile, setCurrentFile] = useState<string>("");
  const [downloadSpeed, setDownloadSpeed] = useState<number>(0);
  const [error, setError] = useState<string | null>(null);

  const refreshStatus = useCallback(async () => {
    try {
      const statusRes = await commands.getLlamaModelsStatus();
      if (statusRes.status === "ok") {
        setIsDownloaded(statusRes.data);
      }

      const downloadingRes = await commands.isLlamaDownloading();
      if (downloadingRes.status === "ok") {
        setIsDownloading(downloadingRes.data);
        if (downloadingRes.data) {
          setError(null);
        }
      }
    } catch (err) {
      console.error("Failed to fetch llama models status:", err);
    }
  }, []);

  const startDownload = useCallback(async () => {
    setError(null);
    setDownloadProgress(0);
    setCurrentFile("");
    setDownloadSpeed(0);
    setIsDownloading(true);

    try {
      const res = await commands.downloadLlamaModels();
      if (res.status === "error") {
        setError(res.error);
        setIsDownloading(false);
      }
    } catch (err) {
      setError(String(err));
      setIsDownloading(false);
    }
  }, []);

  useEffect(() => {
    void refreshStatus();

    const unlistenPromise = listen<DownloadProgressPayload>(
      "llama-download-state",
      (event) => {
        const payload = event.payload;
        if (payload.status === "downloading") {
          setIsDownloading(true);
          setCurrentFile(payload.file);
          setDownloadProgress(payload.percentage);
          setDownloadSpeed(payload.speed_mbps);
        } else if (payload.status === "completed") {
          setIsDownloading(false);
          setIsDownloaded(true);
          setDownloadProgress(100);
          setCurrentFile("");
          setDownloadSpeed(0);
        } else if (payload.status === "error") {
          setIsDownloading(false);
          setError(payload.error || "Unknown download error");
        }
      }
    );

    return () => {
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, [refreshStatus]);

  return {
    isDownloaded,
    isDownloading,
    downloadProgress,
    currentFile,
    downloadSpeed,
    error,
    startDownload,
    refreshStatus,
  };
};
