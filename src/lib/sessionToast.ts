import {
  type ToastContent,
  type ToastLevel,
  type ToastOptions,
  show,
  dismiss,
} from "@/stores/toastStore";
import { type SessionToastLevel, addToast } from "@/stores/sessionToastStore";

type ToastNode =
  | string
  | number
  | boolean
  | null
  | undefined
  | ToastNode[]
  | { props: Record<string, unknown> };

/** A toast's message: text, markup, or a function returning either. */
type ToastMessage = ToastNode | (() => ToastNode);
/** The handle a toast returns. Nothing keeps one today; the API keeps it. */
type ToastResult = number;

const resolveToastNode = (node: ToastMessage | undefined): ToastNode =>
  typeof node === "function" ? node() : node;

const isValidElement = (
  node: unknown,
): node is { props: Record<string, unknown> } =>
  typeof node === "object" &&
  node !== null &&
  "props" in node &&
  typeof (node as Record<string, unknown>).props === "object";

/** The message as plain text, for the Debug page's list of past toasts. */
const getNodeText = (node: ToastNode): string | undefined => {
  if (typeof node === "string" || typeof node === "number") {
    return String(node);
  }
  if (Array.isArray(node)) {
    const text = node.map(getNodeText).filter((part) => part !== undefined);
    return text.length > 0 ? text.join("") : undefined;
  }
  if (isValidElement(node)) {
    const props = node.props as { children?: ToastNode; "aria-label"?: string };
    return getNodeText(props.children) ?? props["aria-label"];
  }
  return undefined;
};

const resolve = (
  message: ToastMessage,
  options?: ToastOptions,
): ToastContent => ({
  message: getNodeText(resolveToastNode(message)) ?? "",
  description: getNodeText(resolveToastNode(options?.description)),
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
    message: getNodeText(content.message) ?? "",
    description: getNodeText(content.description),
    actionLabel: content.action ? getNodeText(content.action.label) : undefined,
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
    dismiss: (id: number) => dismiss(id),
  },
);
