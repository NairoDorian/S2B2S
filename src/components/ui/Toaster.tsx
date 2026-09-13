import { createSignal, createEffect, For } from "solid-js";
import { useToastStore, type ToastRecord, dismiss } from "@/stores/toastStore";
import type { JSX } from "@solidjs/web";

const TOAST_DURATION_MS = 4000;
const VISIBLE_TOASTS = 3;
const TRANSITION_MS = 200;

const CONTAINER_CLASS =
  "fixed bottom-8 end-8 z-[999999999] flex w-[356px] max-w-[calc(100vw-4rem)] flex-col gap-3.5";

const TOAST_CLASS =
  "bg-background border border-mid-gray/20 rounded-lg shadow-lg px-4 py-3 flex items-center gap-3 text-sm";
const TITLE_CLASS = "font-medium";
const DESCRIPTION_CLASS = "text-mid-gray";
const ACTION_CLASS =
  "px-2 py-1 text-xs font-medium rounded-lg border bg-mid-gray/10 border-mid-gray/20 hover:bg-background-ui/30 hover:border-accent cursor-pointer whitespace-nowrap";

interface ToastItemProps {
  toast: ToastRecord;
  onDismiss: (id: number) => void;
}

const ToastItem = (props: ToastItemProps): JSX.Element => {
  const [entered, setEntered] = createSignal(false);
  const [leaving, setLeaving] = createSignal(false);
  const [hovered, setHovered] = createSignal(false);
  const [focused, setFocused] = createSignal(false);

  const remaining = { current: TOAST_DURATION_MS };
  const deadline = { current: 0 };

  createEffect(
    () => undefined,
    () => {
      setEntered(true);
    },
  );

  createEffect(
    () => hovered() || focused() || leaving(),
    (blocked) => {
      if (blocked) return;
      deadline.current = Date.now() + remaining.current;
      const handle = setTimeout(() => setLeaving(true), remaining.current);
      return () => {
        clearTimeout(handle);
        remaining.current = Math.max(0, deadline.current - Date.now());
      };
    },
  );

  createEffect(
    () => leaving(),
    (isLeaving) => {
      if (!isLeaving) return;
      const handle = setTimeout(
        () => props.onDismiss(props.toast.id),
        TRANSITION_MS,
      );
      return () => clearTimeout(handle);
    },
  );

  const handleAction = () => {
    props.toast.action?.onClick();
    setLeaving(true);
  };

  return (
    <div
      role="status"
      aria-live="polite"
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      onFocus={() => setFocused(true)}
      onBlur={() => setFocused(false)}
      class={`${TOAST_CLASS} transition-all duration-200 ease-out ${entered() && !leaving() ? "translate-y-0 opacity-100" : "translate-y-2 opacity-0"}`}
    >
      <div class="min-w-0 flex-1">
        <div class={TITLE_CLASS}>{props.toast.message}</div>
        {props.toast.description !== undefined && (
          <div class={DESCRIPTION_CLASS}>{props.toast.description}</div>
        )}
      </div>
      {props.toast.action && (
        <button type="button" onClick={handleAction} class={ACTION_CLASS}>
          {props.toast.action.label}
        </button>
      )}
    </div>
  );
};

export const Toaster = (): JSX.Element => {
  const store = useToastStore();
  // Accessors, not snapshots. A Solid component body runs once, so capturing
  // `store.toasts` here would freeze the list at whatever it held at mount and
  // no toast would ever appear.
  const visibleToasts = () => store.toasts.slice(-VISIBLE_TOASTS);

  createEffect(
    () => store.toasts.length,
    () => {
      for (const toast of store.toasts.slice(0, -VISIBLE_TOASTS))
        dismiss(toast.id);
    },
  );

  return (
    <div class={CONTAINER_CLASS}>
      <For each={visibleToasts()}>
        {(toast) => <ToastItem toast={toast} onDismiss={dismiss} />}
      </For>
    </div>
  );
};
