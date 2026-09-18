/* oxlint-disable jsx-a11y/prefer-tag-over-role */
import { createEffect, createUniqueId, Show } from "solid-js";
import { X } from "@/components/icons/lucide";
import type { JSX } from "@solidjs/web";
import { Portal } from "@solidjs/web";

const FOCUSABLE_SELECTOR = [
  "a[href]",
  "button:not([disabled])",
  "textarea:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

interface DialogProps {
  open: boolean;
  title: JSX.Element;
  children: JSX.Element;
  onOpenChange: (open: boolean) => void;
  description?: JSX.Element;
  footer?: JSX.Element;
  closeLabel: string;
  dismissible?: boolean;
  closeOnBackdrop?: boolean;
  showCloseButton?: boolean;
  initialFocusRef?: { current: HTMLElement | null } | null;
  class?: string;
  contentClassName?: string;
  contentFades?: boolean;
}

const isVisible = (element: HTMLElement) => {
  const style = window.getComputedStyle(element);
  return style.visibility !== "hidden" && style.display !== "none";
};

const getFocusableElements = (container: HTMLElement) =>
  Array.from(
    container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR),
  ).filter(
    (element) =>
      !element.hasAttribute("disabled") &&
      element.getAttribute("aria-hidden") !== "true" &&
      isVisible(element),
  );

export const Dialog = (props: DialogProps): JSX.Element => {
  const titleId = createUniqueId();
  const descriptionId = createUniqueId();
  let contentRef: HTMLDivElement | null = null;
  let previousFocusRef: HTMLElement | null = null;

  const dismissible = () => props.dismissible ?? true;

  // The compute tracks `open`; the apply owns the body lock and focus and
  // returns its teardown, so closing (or unmounting) restores both. Reading
  // `props.open` in the apply would be untracked and the effect would never
  // re-run — the dialog would open exactly once per mount.
  createEffect(
    () => props.open,
    (open) => {
      if (!open) return;
      previousFocusRef =
        document.activeElement instanceof HTMLElement
          ? document.activeElement
          : null;
      const previousOverflow = document.body.style.overflow;
      document.body.style.overflow = "hidden";

      const focusDialog = () => {
        const fallback = contentRef;
        const target = props.initialFocusRef?.current ?? fallback;
        target?.focus();
      };

      const animationFrame = requestAnimationFrame(focusDialog);
      return () => {
        cancelAnimationFrame(animationFrame);
        document.body.style.overflow = previousOverflow;
        previousFocusRef?.focus();
        previousFocusRef = null;
      };
    },
  );

  createEffect(
    () => [props.open, dismissible()] as const,
    ([open, dismiss]) => {
      if (!open) return;
      const handleKeyDown = (event: KeyboardEvent) => {
        if (event.key === "Escape" && dismiss) {
          event.preventDefault();
          props.onOpenChange(false);
          return;
        }
        if (event.key !== "Tab" || !contentRef) return;
        const focusableElements = getFocusableElements(contentRef);
        if (focusableElements.length === 0) {
          event.preventDefault();
          contentRef.focus();
          return;
        }
        const firstElement = focusableElements[0];
        const lastElement = focusableElements[focusableElements.length - 1];
        const activeElement = document.activeElement;
        if (activeElement === contentRef) {
          event.preventDefault();
          if (event.shiftKey) lastElement.focus();
          else firstElement.focus();
        } else if (event.shiftKey && activeElement === firstElement) {
          event.preventDefault();
          lastElement.focus();
        } else if (!event.shiftKey && activeElement === lastElement) {
          event.preventDefault();
          firstElement.focus();
        }
      };
      document.addEventListener("keydown", handleKeyDown);
      return () => document.removeEventListener("keydown", handleKeyDown);
    },
  );

  const handleBackdropMouseDown = (event: MouseEvent) => {
    if (
      dismissible() &&
      (props.closeOnBackdrop ?? true) &&
      event.target === event.currentTarget
    )
      props.onOpenChange(false);
  };

  const contentStyle = () =>
    (props.contentFades ?? true)
      ? ({
          "mask-image":
            "linear-gradient(to bottom, transparent 0, black 10px, black calc(100% - 20px), transparent 100%)",
          "-webkit-mask-image":
            "linear-gradient(to bottom, transparent 0, black 10px, black calc(100% - 20px), transparent 100%)",
        } as Record<string, string>)
      : undefined;

  return (
    <Show when={props.open}>
      <Portal mount={document.body}>
        <div
          role="presentation"
          class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4 sm:p-6"
          onMouseDown={handleBackdropMouseDown}
        >
          <div
            ref={(el) => {
              contentRef = el;
            }}
            role="dialog"
            aria-modal="true"
            aria-labelledby={titleId}
            aria-describedby={props.description ? descriptionId : undefined}
            tabindex={-1}
            class={`flex max-h-[calc(100dvh-2rem)] w-full max-w-lg flex-col overflow-hidden rounded-lg border border-mid-gray/20 bg-background shadow-xl outline-none sm:max-h-[calc(100dvh-3rem)] ${props.class ?? ""}`}
          >
            <div class="flex shrink-0 items-start justify-between gap-3 border-b border-mid-gray/20 px-4 py-2.5">
              <div class="min-w-0">
                <h2 id={titleId} class="text-base font-semibold text-text">
                  {props.title}
                </h2>
                {props.description && (
                  <p id={descriptionId} class="mt-1 text-sm text-mid-gray">
                    {props.description}
                  </p>
                )}
              </div>
              {dismissible() && (props.showCloseButton ?? true) && (
                <button
                  type="button"
                  onClick={() => props.onOpenChange(false)}
                  aria-label={props.closeLabel}
                  class="shrink-0 cursor-pointer rounded-md border border-transparent p-1 text-mid-gray transition-colors hover:border-mid-gray/20 hover:bg-mid-gray/10 hover:text-text focus:outline-none focus-visible:ring-1 focus-visible:ring-accent"
                >
                  <X class="h-4 w-4" aria-hidden="true" />
                </button>
              )}
            </div>
            <div
              class={`min-h-0 overflow-y-auto px-4 pb-4 pt-3 ${props.contentClassName ?? ""}`}
              style={contentStyle()}
            >
              {props.children}
            </div>
            {props.footer && (
              <div class="flex shrink-0 justify-end gap-2 border-t border-mid-gray/20 px-4 py-3">
                {props.footer}
              </div>
            )}
          </div>
        </div>
      </Portal>
    </Show>
  );
};
