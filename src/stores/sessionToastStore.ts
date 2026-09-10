import { create } from "zustand";

export type SessionToastLevel = "error" | "warning";

export interface SessionToastRecord {
  id: number;
  level: SessionToastLevel;
  shownAt: number;
  message: string;
  description?: string;
  actionLabel?: string;
}

type NewSessionToast = Omit<SessionToastRecord, "id" | "shownAt">;

interface SessionToastStore {
  toasts: SessionToastRecord[];
  showErrors: boolean;
  showWarnings: boolean;
  addToast: (toast: NewSessionToast) => void;
  setShowErrors: (showErrors: boolean) => void;
  setShowWarnings: (showWarnings: boolean) => void;
}

const MAX_RECORDS = 200;
let nextToastId = 1;

/**
 * Every error/warning toast shown this session, so one that auto-dismissed
 * while the user was looking elsewhere can still be read on the Debug page.
 * Deliberately in-memory only: it resets with the app.
 */
export const useSessionToastStore = create<SessionToastStore>((set) => ({
  toasts: [],
  showErrors: true,
  showWarnings: true,
  addToast: (toast) =>
    set((state) => ({
      toasts: [
        ...state.toasts.slice(-(MAX_RECORDS - 1)),
        { ...toast, id: nextToastId++, shownAt: Date.now() },
      ],
    })),
  setShowErrors: (showErrors) => set({ showErrors }),
  setShowWarnings: (showWarnings) => set({ showWarnings }),
}));
