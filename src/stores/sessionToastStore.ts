import { createStore } from "solid-js";

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

interface SessionToastStoreData {
  toasts: SessionToastRecord[];
  showErrors: boolean;
  showWarnings: boolean;
}

const MAX_RECORDS = 200;
let nextToastId = 1;

const [sessionToastStore, setSessionToastStore] =
  createStore<SessionToastStoreData>({
    toasts: [] as SessionToastRecord[],
    showErrors: true,
    showWarnings: true,
  });

export const addToast = (toast: NewSessionToast) => {
  setSessionToastStore((state) => {
    state.toasts = [
      ...state.toasts.slice(-(MAX_RECORDS - 1)),
      { ...toast, id: nextToastId++, shownAt: Date.now() },
    ];
  });
};

export const setShowErrors = (showErrors: boolean) => {
  setSessionToastStore((state) => ({ ...state, showErrors }));
};

export const setShowWarnings = (showWarnings: boolean) => {
  setSessionToastStore((state) => ({ ...state, showWarnings }));
};

export const useSessionToastStore = () => sessionToastStore;
