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
  const {
    open,
    onToggle,
    label,
    trigger,
    title,
    headerAction,
    subtitle,
    disabled = false,
    widthClass = "w-[min(22rem,calc(100vw-2rem))]",
    children,
  } = props;
  return (
    <div class="relative shrink-0">
      <button
        type="button"
        onClick={onToggle}
        disabled={disabled}
        aria-haspopup="dialog"
        aria-expanded={open ? "true" : "false"}
        aria-label={label}
        title={label}
        class={`flex max-w-full items-center gap-1.5 rounded-md border px-1.5 py-1 transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${open ? "border-mid-gray/30 bg-mid-gray/15 text-text/90" : "border-transparent hover:border-mid-gray/25 hover:bg-mid-gray/10 hover:text-text/90"}`}
      >
        {trigger}
        <ChevronDown
          class={`h-3 w-3 shrink-0 transition-transform ${open ? "rotate-180" : ""}`}
        />
      </button>
      <Show when={open}>
        <div
          role="dialog"
          aria-label={title}
          class={`absolute bottom-full start-0 z-50 mb-2 ${widthClass} overflow-hidden rounded-lg border border-mid-gray/25 bg-background shadow-xl`}
        >
          <div class="flex items-center justify-between gap-2 border-b border-mid-gray/20 px-3 py-2">
            <div class="min-w-0">
              <h3 class="truncate text-xs font-semibold uppercase tracking-wide text-text/70">
                {title}
              </h3>
              <Show when={subtitle}>
                <div class="mt-0.5 text-[11px] leading-snug text-text/45">
                  {subtitle}
                </div>
              </Show>
            </div>
            {headerAction}
          </div>
          {children}
        </div>
      </Show>
    </div>
  );
};

export default StatusBarPopover;
