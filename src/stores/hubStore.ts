// Zustand store for the unified Model Hub.
//
// Aggregates download progress across ALL model collections (STT, Brain,
// TTS/audio.cpp, and llama-server runtimes) into a single active-downloads
// surface, driven by the typed `model-hub-*` events. STT now emits those
// same typed events from the model.rs manager.

import { create } from "zustand";
import { toast } from "sonner";
import { produce } from "immer";
import {
  commands,
  events,
  type ModelCollection,
  type ModelHubDownloadProgress,
  type ModelHubNotification,
} from "@/bindings";

interface ActiveDownload {
  collection: ModelCollection;
  id: string;
  name: string;
  file: string | null;
  percent: number | null;
  speedMbps: number | null;
  status: string;
}

interface HubStore {
  downloads: Record<string, ActiveDownload>;
  initialized: boolean;

  initialize: () => Promise<void>;
  downloadModel: (collection: ModelCollection, id: string) => Promise<boolean>;
  cancelDownload: (collection: ModelCollection, id: string) => Promise<void>;
  deleteModel: (collection: ModelCollection, id: string) => Promise<boolean>;
  refresh: () => Promise<void>;
  clear: (key: string) => void;
}

const keyFor = (collection: ModelCollection, id: string) =>
  `${collection}:${id}` as const;

export const useHubStore = create<HubStore>()(
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (set: any, get: any) => ({
    downloads: {},
    initialized: false,

    initialize: async () => {
      if (get().initialized) return;

      // Restore any in-flight / last-known downloads known to the backend.
      try {
        const data = await commands.hubGetActiveDownloads();
        const map: Record<string, ActiveDownload> = {};
        for (const p of data) {
          map[keyFor(p.collection, p.id)] = {
            collection: p.collection,
            id: p.id,
            name: p.name,
            file: p.file ?? null,
            percent: p.percent,
            speedMbps: p.speedMbps,
            status: p.status,
          };
        }
        set({ downloads: map });
      } catch {
        /* backend unreachable during early init is fine */
      }

      // Typed hub events (Brain / audio.cpp / runtimes).
      const hubProgress = events.modelHubDownloadProgress.listen((ev) => {
        const p: ModelHubDownloadProgress = ev.payload;
        set(
          produce((state: any) => {
            const k = keyFor(p.collection, p.id);
            if (p.status === "completed" || p.status === "failed") {
              delete state.downloads[k];
            } else {
              state.downloads[k] = {
                collection: p.collection,
                id: p.id,
                name: p.name,
                file: p.file ?? null,
                percent: p.percent,
                speedMbps: p.speedMbps,
                status: p.status,
              };
            }
          }),
        );
      });

      const hubNotify = events.modelHubNotification.listen((ev) => {
        const n: ModelHubNotification = ev.payload;
        const k = keyFor(n.collection, n.id);
        set((state: any) => {
          const next = { ...state };
          delete next.downloads[k];
          return next;
        });
        if (n.kind === "failed" && n.error) {
          toast.error(n.error);
        } else if (n.kind === "completed") {
          toast.success(`${n.name} ready`);
        }
      });

      // Store cleanup functions so initialize can be re-invoked safely.
      (get as any)._cleanup = () => {
        void hubProgress.then((fn) => fn());
        void hubNotify.then((fn) => fn());
      };

      set({ initialized: true });
    },

    downloadModel: async (collection, id) => {
      try {
        const result = await commands.hubDownloadModel({ collection, id });
        if (result.status === "ok") {
          return true;
        }
        toast.error(result.error);
        return false;
      } catch (err) {
        toast.error(String(err));
        return false;
      }
    },

    cancelDownload: async (collection, id) => {
      const result = await commands.hubCancelDownload(collection, id);
      if (result.status !== "ok") {
        toast.error(result.error);
      }
    },

    deleteModel: async (collection, id) => {
      const result = await commands.hubDeleteModel(collection, id);
      if (result.status === "ok") {
        const k = keyFor(collection, id);
        set((state: any) => {
          const next = { ...state };
          delete next.downloads[k];
          return next;
        });
        return true;
      }
      toast.error(result.error);
      return false;
    },

    refresh: async () => {
      try {
        const data = await commands.hubGetActiveDownloads();
        const map: Record<string, ActiveDownload> = {};
        for (const p of data) {
          map[keyFor(p.collection, p.id)] = {
            collection: p.collection,
            id: p.id,
            name: p.name,
            file: p.file ?? null,
            percent: p.percent,
            speedMbps: p.speedMbps,
            status: p.status,
          };
        }
        set({ downloads: map });
      } catch {
        /* ignore */
      }
    },

    clear: (key) => {
      set(
        produce((state: any) => {
          delete state.downloads[key];
        }),
      );
    },
  }),
);
