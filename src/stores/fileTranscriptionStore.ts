import { createSolidStore } from "@/lib/solidStore";
import {
  commands,
  events,
  type FileJobStatus,
  type FileTranscriptionEvent,
} from "@/bindings";

/** Mirrors `SUPPORTED_EXTENSIONS` in `src-tauri/src/file_transcription.rs`. */
export const SUPPORTED_AUDIO_EXTENSIONS = [
  "wav",
  "mp3",
  "m4a",
  "mp4",
  "aac",
  "flac",
  "ogg",
  "oga",
] as const;

export const isSupportedAudioPath = (path: string): boolean => {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return (SUPPORTED_AUDIO_EXTENSIONS as readonly string[]).includes(ext);
};

const fileNameOf = (path: string): string => path.split(/[/\\]/).pop() ?? path;

export interface QueueItem {
  path: string;
  name: string;
  status: FileJobStatus;
  segment: number | null;
  segments: number | null;
  text: string | null;
  outputPath: string | null;
  error: string | null;
  audioSeconds: number | null;
  elapsedMs: number | null;
}

const TERMINAL = new Set<FileJobStatus>(["done", "failed", "cancelled"]);
export const isTerminalStatus = (status: FileJobStatus) => TERMINAL.has(status);

interface FileTranscriptionStore {
  items: QueueItem[];
  running: boolean;
  jobId: number | null;
  initialized: boolean;

  initialize: () => Promise<void>;
  addPaths: (paths: string[]) => number;
  removePath: (path: string) => void;
  clear: () => void;
  clearFinished: () => void;
  start: () => Promise<string | null>;
  cancel: () => Promise<void>;
}

const newItem = (path: string): QueueItem => ({
  path,
  name: fileNameOf(path),
  status: "queued",
  segment: null,
  segments: null,
  text: null,
  outputPath: null,
  error: null,
  audioSeconds: null,
  elapsedMs: null,
});

const applyEvent = (
  items: QueueItem[],
  event: FileTranscriptionEvent,
): QueueItem[] =>
  items.map((item) =>
    item.path === event.path
      ? {
          ...item,
          status: event.status,
          segment: event.segment ?? null,
          segments: event.segments ?? null,
          text: event.text ?? item.text,
          outputPath: event.output_path ?? item.outputPath,
          error: event.error ?? null,
          audioSeconds: event.audio_seconds ?? item.audioSeconds,
          elapsedMs: event.elapsed_ms ?? item.elapsedMs,
        }
      : item,
  );

/**
 * Queue + progress of the "Transcribe Files" page. Lives in a store (not the
 * page) because the job keeps running in the backend while the user browses
 * other pages; the event listener is installed once and survives unmounts.
 */
const fileTranscriptionState = createSolidStore<FileTranscriptionStore>(
  (set, get) => ({
    items: [],
    running: false,
    jobId: null,
    initialized: false,

    initialize: async () => {
      if (get().initialized) return;
      set({ initialized: true });
      await events.fileTranscriptionEvent.listen((event) => {
        const payload = event.payload;
        // `start` marks the store running before the start command replies,
        // and the job's first events (or all of them, for a job that fails at
        // once) can arrive before that reply: adopt the job then.
        if (get().jobId === null && get().running) {
          set({ jobId: payload.job_id });
        }
        if (payload.job_id !== get().jobId) return;
        if (payload.batch_finished) {
          set((state) => ({
            running: false,
            // Anything the backend never reached stays queued for a rerun.
            items: state.items.map((item) =>
              isTerminalStatus(item.status)
                ? item
                : { ...item, status: "queued" },
            ),
          }));
          return;
        }
        set((state) => ({ items: applyEvent(state.items, payload) }));
      });
      try {
        const status = await commands.getFileTranscriptionStatus();
        if (status.running) {
          set({ running: true, jobId: status.job_id });
        }
      } catch (error) {
        console.error("Failed to read file transcription status:", error);
      }
    },

    addPaths: (paths) => {
      const existing = new Set(get().items.map((item) => item.path));
      const fresh = paths.filter(
        (path) => isSupportedAudioPath(path) && !existing.has(path),
      );
      if (fresh.length === 0) return 0;
      set((state) => ({
        items: [...state.items, ...fresh.map(newItem)],
      }));
      return fresh.length;
    },

    removePath: (path) =>
      set((state) => ({
        items: state.items.filter((item) => item.path !== path),
      })),

    clear: () => set({ items: [] }),

    clearFinished: () =>
      set((state) => ({
        items: state.items.filter((item) => !isTerminalStatus(item.status)),
      })),

    start: async () => {
      const pending = get().items.filter((item) => item.status !== "done");
      if (pending.length === 0 || get().running) return null;
      set((state) => ({
        items: state.items.map((item) =>
          item.status === "done"
            ? item
            : { ...newItem(item.path), status: "queued" },
        ),
        running: true,
        jobId: null,
      }));
      let result: Awaited<ReturnType<typeof commands.startFileTranscription>>;
      try {
        result = await commands.startFileTranscription(
          pending.map((item) => item.path),
        );
      } catch (error) {
        set({ running: false });
        throw error;
      }
      if (result.status === "error") {
        set({ running: false, jobId: null });
        return result.error;
      }
      const jobId = result.data;
      set({ jobId });
      if (!get().running) return null; // batch_finished already arrived
      // A job that finished before the reply may have emitted its last event
      // before the listener could adopt it; the status command settles it.
      try {
        const status = await commands.getFileTranscriptionStatus();
        if (status.job_id === jobId && !status.running && get().running) {
          set((state) => ({
            running: false,
            items: state.items.map((item) =>
              isTerminalStatus(item.status)
                ? item
                : { ...item, status: "queued" },
            ),
          }));
        }
      } catch (error) {
        console.error("Failed to read file transcription status:", error);
      }
      return null;
    },

    cancel: async () => {
      await commands.cancelFileTranscription();
    },
  }),
);

export function useFileTranscriptionStore() {
  return fileTranscriptionState;
}
