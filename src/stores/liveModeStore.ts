import { createSolidStore } from "@/lib/solidStore";
import {
  commands,
  events,
  type LiveModeStatus,
  type LiveSessionInfo,
} from "@/bindings";

/** Keep at most this many characters of committed text in memory. */
const MAX_STABLE_CHARS = 400_000;

const IDLE_STATUS: LiveModeStatus = {
  phase: "idle",
  session_dir: null,
  transcript_path: null,
  model_id: null,
  chunk_index: 0,
  chunks: [],
  started_at_ms: null,
  elapsed_ms: 0,
  current_chunk_ms: 0,
  current_chunk_speech_ms: 0,
  transcript_bytes: 0,
  error: null,
};

export const isLiveActive = (status: LiveModeStatus): boolean =>
  status.phase !== "idle" && status.phase !== "error";

interface LiveModeStore {
  status: LiveModeStatus;
  /** Committed transcript text of the running / last session. */
  stable: string;
  /** Volatile live tail (rewritten on every stream update). */
  live: string;
  sessions: LiveSessionInfo[];
  /** A past session opened in the viewer (only while idle). */
  viewing: LiveSessionInfo | null;
  viewingText: string;
  defaultOutputDir: string | null;
  initialized: boolean;

  initialize: () => Promise<void>;
  start: () => Promise<string | null>;
  stop: () => Promise<string | null>;
  refreshSessions: () => Promise<void>;
  viewSession: (session: LiveSessionInfo) => Promise<void>;
  closeViewer: () => void;
}

const liveModeState = createSolidStore<LiveModeStore>((set, get) => ({
  status: IDLE_STATUS,
  stable: "",
  live: "",
  sessions: [],
  viewing: null,
  viewingText: "",
  defaultOutputDir: null,
  initialized: false,

  initialize: async () => {
    if (get().initialized) return;
    set({ initialized: true });
    await events.liveModeStateEvent.listen((event) => {
      const status = event.payload.status;
      const wasActive = isLiveActive(get().status);
      set({ status });
      if (wasActive && !isLiveActive(status)) {
        void get().refreshSessions();
      }
    });
    await events.liveModeTranscriptEvent.listen((event) => {
      const { reset, stable_appended, live } = event.payload;
      set((state) => {
        let stable = reset ? stable_appended : state.stable + stable_appended;
        if (stable.length > MAX_STABLE_CHARS) {
          stable = stable.slice(stable.length - MAX_STABLE_CHARS);
        }
        return { stable, live, viewing: null, viewingText: "" };
      });
    });
    try {
      const status = await commands.liveModeStatus();
      set({ status });
    } catch (error) {
      console.error("Failed to read Live Mode status:", error);
    }
    try {
      const dir = await commands.liveModeDefaultOutputDir();
      if (dir.status === "ok") set({ defaultOutputDir: dir.data });
    } catch (error) {
      console.error("Failed to resolve Live Mode output dir:", error);
    }
    await get().refreshSessions();
  },

  start: async () => {
    set({ stable: "", live: "", viewing: null, viewingText: "" });
    const result = await commands.liveModeStart();
    return result.status === "error" ? result.error : null;
  },

  stop: async () => {
    const result = await commands.liveModeStop();
    return result.status === "error" ? result.error : null;
  },

  refreshSessions: async () => {
    const result = await commands.liveModeListSessions();
    if (result.status === "ok") {
      set({ sessions: result.data });
    }
  },

  viewSession: async (session) => {
    if (!session.transcript_path) return;
    const result = await commands.readTextFile(session.transcript_path);
    if (result.status === "ok") {
      set({ viewing: session, viewingText: result.data });
    } else {
      // Leave the viewer as it was rather than showing this session empty.
      console.error(
        `Failed to read live session transcript ${session.transcript_path}:`,
        result.error,
      );
    }
  },

  closeViewer: () => set({ viewing: null, viewingText: "" }),
}));

export function useLiveModeStore() {
  return liveModeState;
}
