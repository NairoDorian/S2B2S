/* oxlint-disable jsx-a11y/prefer-tag-over-role */
import { Show } from "solid-js";
import { ChevronDown } from "@/components/icons/lucide";
import type { JSX } from "@solidjs/web";

interface StatusBarPopoverProps {
  open: boolean;
  onToggle: () => void;
  label: string;
  trigger: JSX.Element;
  title: string;
  headerAction?: JSX.Element;
  subtitle?: JSX.Element;
  disabled?: boolean;
  widthClass?: string;
  children: JSX.Element;
}

export const StatusBarPopover = (props: StatusBarPopoverProps): JSX.Element => {
  return (
    <div class="relative shrink-0">
      <button
        type="button"
        onClick={() => props.onToggle()}
        disabled={props.disabled ?? false}
        aria-haspopup="dialog"
        aria-expanded={props.open ? "true" : "false"}
        aria-label={props.label}
        title={props.label}
        class={`flex max-w-full items-center gap-1.5 rounded-md border px-1.5 py-1 transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${props.open ? "border-mid-gray/30 bg-mid-gray/15 text-text/90" : "border-transparent hover:border-mid-gray/25 hover:bg-mid-gray/10 hover:text-text/90"}`}
      >
        {props.trigger}
        <ChevronDown
          class={`h-3 w-3 shrink-0 transition-transform ${props.open ? "rotate-180" : ""}`}
        />
      </button>
      <Show when={props.open}>
        <div
          role="dialog"
          aria-label={props.title}
          class={`absolute bottom-full start-0 z-50 mb-2 ${props.widthClass ?? "w-[min(22rem,calc(100vw-2rem))]"} overflow-hidden rounded-lg border border-mid-gray/25 bg-background shadow-xl`}
        >
          <div class="flex items-center justify-between gap-2 border-b border-mid-gray/20 px-3 py-2">
            <div class="min-w-0">
              <h3 class="truncate text-xs font-semibold uppercase tracking-wide text-text/70">
                {props.title}
              </h3>
              <Show when={props.subtitle}>
                <div class="mt-0.5 text-[11px] leading-snug text-text/45">
                  {props.subtitle}
                </div>
              </Show>
            </div>
            {props.headerAction}
          </div>
          {props.children}
        </div>
      </Show>
    </div>
  );
};

export default StatusBarPopover;
