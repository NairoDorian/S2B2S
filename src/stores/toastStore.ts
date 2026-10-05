/**
 * The live toast queue `components/ui/Toaster.tsx` renders.
 *
 * It is a plain Solid store rather than a `createSolidStore` one because nothing
 * here needs actions: the app's own API is `lib/sessionToast.ts`, which wraps
 * `show` / `dismiss` and additionally records errors and warnings for the Debug
 * page. `id` is a module-level counter, never reused, so a dismissal can never
 * remove a toast that arrived after it.
 */

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
