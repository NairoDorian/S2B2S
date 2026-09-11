import { create } from "zustand";
import {
  commands,
  events,
  type CudaToolkitInfo,
  type InstalledLlamaServer,
  type LlamaDownloadEvent,
  type LlamaRelease,
  type LlamaServerStateEvent,
} from "@/bindings";
import { sessionToast as toast } from "@/lib/sessionToast";

/**
 * In-app llama.cpp server: supervised state (pushed by the backend), logs
 * (pulled only while the page shows them), release list and installs.
 * Lives outside the page so the footer indicator and the page share one
 * subscription.
 */
interface LlamaStore {
  state: LlamaServerStateEvent | null;
  logs: string[];
  releases: LlamaRelease[];
  releasesLoading: boolean;
  releasesError: string | null;
  installed: InstalledLlamaServer[];
  download: LlamaDownloadEvent | null;
  detectedBackend: string | null;
  commandPreview: string;
  commandError: string | null;
  initialized: boolean;

  initialize: () => Promise<void>;
  refreshState: () => Promise<void>;
  refreshLogs: () => Promise<void>;
  refreshInstalled: () => Promise<void>;
  refreshPreview: () => Promise<void>;
  fetchReleases: (channel: string, force: boolean) => Promise<void>;
  start: () => Promise<void>;
  stop: () => Promise<void>;
  restart: () => Promise<void>;
  install: (
    tag: string,
    backend: string,
    includeCudart: boolean,
  ) => Promise<void>;
  removeCudaRuntime: (dir: string) => Promise<void>;
  cudaToolkit: CudaToolkitInfo | null;
  removeInstalled: (dir: string) => Promise<void>;
}

export const useLlamaStore = create<LlamaStore>()((set, get) => ({
  state: null,
  logs: [],
  releases: [],
  releasesLoading: false,
  releasesError: null,
  installed: [],
  download: null,
  detectedBackend: null,
  cudaToolkit: null,
  commandPreview: "",
  commandError: null,
  initialized: false,

  initialize: async () => {
    if (get().initialized) return;
    set({ initialized: true });
    await events.llamaServerStateEvent.listen((event) => {
      set({ state: event.payload });
    });
    await events.llamaDownloadEvent.listen((event) => {
      const download = event.payload;
      set({ download });
      if (download.phase === "done") {
        void get().refreshInstalled();
      } else if (download.phase === "error" && download.message) {
        toast.error(download.message);
      }
    });
    await get().refreshState();
    commands
      .detectLlamaBackend()
      .then((backend) => set({ detectedBackend: backend }))
      .catch(() => {});
    commands
      .detectCudaToolkit()
      .then((toolkit) => set({ cudaToolkit: toolkit }))
      .catch(() => {});
  },

  refreshState: async () => {
    try {
      set({ state: await commands.getLlamaServerState() });
    } catch (error) {
      console.error("Failed to read llama-server state:", error);
    }
  },

  refreshLogs: async () => {
    try {
      set({ logs: await commands.getLlamaServerLogs() });
    } catch (error) {
      console.error("Failed to read llama-server logs:", error);
    }
  },

  refreshInstalled: async () => {
    try {
      set({ installed: await commands.listInstalledLlamaServers() });
    } catch (error) {
      console.error("Failed to list llama.cpp installs:", error);
    }
  },

  refreshPreview: async () => {
    const result = await commands.getLlamaCommandPreview();
    if (result.status === "ok") {
      set({ commandPreview: result.data, commandError: null });
    } else {
      set({ commandPreview: "", commandError: String(result.error) });
    }
  },

  fetchReleases: async (channel, force) => {
    set({ releasesLoading: true, releasesError: null });
    const result = await commands.fetchLlamaReleases(channel, force);
    if (result.status === "ok") {
      set({ releases: result.data, releasesLoading: false });
    } else {
      set({ releasesError: String(result.error), releasesLoading: false });
    }
  },

  start: async () => {
    const result = await commands.startLlamaServer();
    if (result.status === "error") toast.error(String(result.error));
  },
  stop: async () => {
    const result = await commands.stopLlamaServer();
    if (result.status === "error") toast.error(String(result.error));
  },
  restart: async () => {
    const result = await commands.restartLlamaServer();
    if (result.status === "error") toast.error(String(result.error));
  },
  install: async (tag, backend, includeCudart) => {
    set({
      download: {
        tag,
        backend,
        phase: "downloading",
        downloaded_bytes: 0,
        total_bytes: 0,
        message: null,
        dir: null,
      },
    });
    const result = await commands.installLlamaRelease(
      tag,
      backend,
      includeCudart,
    );
    if (result.status === "error") {
      toast.error(String(result.error));
      set({ download: null });
    }
  },
  removeCudaRuntime: async (dir) => {
    const result = await commands.removeBundledCudaRuntime(dir);
    if (result.status === "error") {
      toast.error(String(result.error));
      return;
    }
    toast.success(`Freed ${result.data} MB`);
    await get().refreshInstalled();
  },
  removeInstalled: async (dir) => {
    const result = await commands.removeInstalledLlamaServer(dir);
    if (result.status === "error") {
      toast.error(String(result.error));
      return;
    }
    await get().refreshInstalled();
  },
}));
