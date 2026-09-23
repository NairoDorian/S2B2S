import {
  type ToastContent,
  type ToastLevel,
  type ToastOptions,
  show,
} from "@/stores/toastStore";
import { type SessionToastLevel, addToast } from "@/stores/sessionToastStore";

/**
 * A toast's message. Plain text only: the Toaster renders strings, and the
 * Debug page's list of past toasts stores them as they are.
 */
type ToastMessage = string;
/** The handle a toast returns. Nothing keeps one today; the API keeps it. */
type ToastResult = number;

const resolve = (
  message: ToastMessage,
  options?: ToastOptions,
): ToastContent => ({
  message,
  description: options?.description,
  action: options?.action,
});

const showToast = (level: ToastLevel, content: ToastContent): ToastResult =>
  show(level, content);

const showTrackedToast = (
  level: SessionToastLevel,
  content: ToastContent,
): ToastResult => {
  const toastId = showToast(level, content);

  addToast({
    level,
    message: content.message,
    description: content.description,
    actionLabel: content.action?.label,
  });

  return toastId;
};

/**
 * Drop-in replacement for sonner's `toast`: the same levels and the same
 * `{ description, action }` options, but rendered by the app's own
 * `components/ui/Toaster.tsx`. `error` and `warning` are also recorded in
 * `useSessionToastStore` so the Debug page can list them after they
 * auto-dismissed. Ported from AIVORelay.
 *
 * A bare `toast(...)` — sonner's untyped default — is an `info` here, since
 * that is what it renders as.
 */
export const sessionToast = Object.assign(
  (message: ToastMessage, options?: ToastOptions) =>
    showToast("info", resolve(message, options)),
  {
    success: (message: ToastMessage, options?: ToastOptions) =>
      showToast("success", resolve(message, options)),
    info: (message: ToastMessage, options?: ToastOptions) =>
      showToast("info", resolve(message, options)),
    error: (message: ToastMessage, options?: ToastOptions) =>
      showTrackedToast("error", resolve(message, options)),
    warning: (message: ToastMessage, options?: ToastOptions) =>
      showTrackedToast("warning", resolve(message, options)),
  },
);
