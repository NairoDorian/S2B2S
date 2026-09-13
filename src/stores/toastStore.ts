import { createStore } from "solid-js";

export type ToastLevel = "success" | "info" | "warning" | "error";

export interface ToastAction {
  label: string;
  onClick: () => void;
}

export interface ToastOptions {
  description?: string;
  action?: ToastAction;
}

export interface ToastContent extends ToastOptions {
  message: string;
}

export interface ToastRecord extends ToastContent {
  id: number;
  level: ToastLevel;
}

interface ToastStore {
  toasts: ToastRecord[];
}

let nextToastId = 1;

const [toastStore, setToastStore] = createStore<ToastStore>({
  toasts: [],
});

export function show(level: ToastLevel, content: ToastContent): number {
  const id = nextToastId++;
  setToastStore((state) => ({
    toasts: [...state.toasts, { id, level, ...content }],
  }));
  return id;
}

export function dismiss(id: number): void {
  setToastStore((state) => ({
    toasts: state.toasts.filter((toast) => toast.id !== id),
  }));
}

export function useToastStore() {
  return toastStore;
}
