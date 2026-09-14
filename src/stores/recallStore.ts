import { createSolidStore } from "@/lib/solidStore";
import {
  commands,
  type RecallNoteMeta,
  type RecallVaultInfo,
} from "@/bindings";

/** Draft of the note currently open in the editor. */
export interface RecallDraft {
  title: string;
  /** Comma-separated tag text as typed in the UI. */
  tags: string;
  body: string;
}

const parseTags = (tags: string): string[] =>
  tags
    .split(",")
    .map((t) => t.trim())
    .filter((t) => t.length > 0);

interface RecallStore {
  notes: RecallNoteMeta[];
  vault: RecallVaultInfo | null;
  initialized: boolean;

  /** Encryption state of the vault, or null until first read. */
  encryption: { enabled: boolean; unlocked: boolean } | null;

  activeId: string | null;
  draft: RecallDraft;
  /** The last saved body, so dirty state survives round-trips. */
  savedDraft: RecallDraft;
  saving: boolean;
  dictate: "idle" | "recording" | "transcribing";

  initialize: () => Promise<void>;
  refresh: () => Promise<void>;
  refreshEncryption: () => Promise<void>;
  openNote: (id: string) => Promise<string | null>;
  closeNote: () => void;
  setDraft: (patch: Partial<RecallDraft>) => void;
  createNote: (title: string) => Promise<string | null>;
  save: () => Promise<string | null>;
  remove: (id: string) => Promise<string | null>;
  dictateStart: () => Promise<string | null>;
  /** Stops the recording and resolves with the transcribed text. */
  dictateStop: () => Promise<{ text: string } | { error: string }>;
  dictateCancel: () => Promise<string | null>;
  openVaultFolder: () => Promise<void>;
  enableEncryption: (
    passphrase: string,
  ) => Promise<
    { notesEncrypted: number; backupDir: string } | { error: string }
  >;
  unlock: (passphrase: string) => Promise<string | null>;
  lock: () => Promise<string | null>;
  disableEncryption: (passphrase: string) => Promise<string | null>;
}

const EMPTY_DRAFT: RecallDraft = { title: "", tags: "", body: "" };

const recallState = createSolidStore<RecallStore>((set, get) => ({
  notes: [],
  vault: null,
  initialized: false,

  encryption: null,

  activeId: null,
  draft: EMPTY_DRAFT,
  savedDraft: EMPTY_DRAFT,
  saving: false,
  dictate: "idle",

  initialize: async () => {
    if (get().initialized) return;
    set({ initialized: true });
    await get().refresh();
  },

  refresh: async () => {
    const [notes, vault, encryption] = await Promise.all([
      commands.recallListNotes(),
      commands.recallVaultInfo(),
      commands.recallEncryptionStatus(),
    ]);
    if (notes.status === "ok" && vault.status === "ok") {
      set({ notes: notes.data, vault: vault.data });
    }
    if (encryption.status === "ok") {
      set({ encryption: encryption.data });
    }
  },

  refreshEncryption: async () => {
    const result = await commands.recallEncryptionStatus();
    if (result.status === "ok") {
      set({ encryption: result.data });
    }
    await get().refresh();
  },

  openNote: async (id) => {
    const result = await commands.recallReadNote(id);
    if (result.status === "error") return result.error;
    const draft: RecallDraft = {
      title: result.data.meta.title,
      tags: result.data.meta.tags.join(", "),
      body: result.data.body,
    };
    set({ activeId: id, draft, savedDraft: draft });
    return null;
  },

  closeNote: () =>
    set({ activeId: null, draft: EMPTY_DRAFT, savedDraft: EMPTY_DRAFT }),

  setDraft: (patch) =>
    set((state) => ({ draft: { ...state.draft, ...patch } })),

  createNote: async (title) => {
    const result = await commands.recallCreateNote(title, []);
    if (result.status === "error") return result.error;
    await get().refresh();
    const openError = await get().openNote(result.data.id);
    return openError ?? null;
  },

  save: async () => {
    const id = get().activeId;
    if (!id) return null;
    set({ saving: true });
    const { draft } = get();
    const result = await commands.recallWriteNote(
      id,
      draft.title,
      parseTags(draft.tags),
      draft.body,
    );
    if (result.status === "error") {
      set({ saving: false });
      return result.error;
    }
    set({ saving: false, savedDraft: { ...draft } });
    await get().refresh();
    return null;
  },

  remove: async (id) => {
    const result = await commands.recallDeleteNote(id);
    if (result.status === "error") return result.error;
    if (get().activeId === id) get().closeNote();
    await get().refresh();
    return null;
  },

  dictateStart: async () => {
    const result = await commands.recallDictateStart();
    if (result.status === "error") return result.error;
    set({ dictate: "recording" });
    return null;
  },

  dictateStop: async () => {
    set({ dictate: "transcribing" });
    const result = await commands.recallDictateStop();
    set({ dictate: "idle" });
    return result.status === "error"
      ? { error: result.error }
      : { text: result.data };
  },

  dictateCancel: async () => {
    const result = await commands.recallDictateCancel();
    set({ dictate: "idle" });
    return result.status === "error" ? result.error : null;
  },

  openVaultFolder: async () => {
    await commands.recallOpenVaultFolder();
  },

  enableEncryption: async (passphrase) => {
    const result = await commands.recallEnableEncryption(passphrase);
    if (result.status === "error") return { error: result.error };
    await get().refreshEncryption();
    // The notes the page held are ciphertext metadata now; the editor closes.
    get().closeNote();
    return {
      notesEncrypted: result.data.notes_encrypted,
      backupDir: result.data.backup_dir,
    };
  },

  unlock: async (passphrase) => {
    const result = await commands.recallUnlockVault(passphrase);
    if (result.status === "error") return result.error;
    await get().refreshEncryption();
    return null;
  },

  lock: async () => {
    const result = await commands.recallLockVault();
    if (result.status === "error") return result.error;
    get().closeNote();
    await get().refreshEncryption();
    return null;
  },

  disableEncryption: async (passphrase) => {
    const result = await commands.recallDisableEncryption(passphrase);
    if (result.status === "error") return result.error;
    get().closeNote();
    await get().refreshEncryption();
    return null;
  },
}));

export function useRecallStore() {
  return recallState;
}

export { parseTags };
