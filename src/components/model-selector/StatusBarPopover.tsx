import React from "react";
import { ChevronDown } from "lucide-react";

interface StatusBarPopoverProps {
  /** Whether the panel is showing. Owned by the parent so only one bar popover is open at a time. */
  open: boolean;
  onToggle: () => void;
  /** Accessible name for the pill (the visible content is usually an abbreviation). */
  label: string;
  /** Compact content shown in the status bar. */
  trigger: React.ReactNode;
  /** Panel heading. */
  title: string;
  /** Optional control rendered at the end of the heading row. */
  headerAction?: React.ReactNode;
  /** Optional muted line under the heading. */
  subtitle?: React.ReactNode;
  disabled?: boolean;
  /** Tailwind width for the panel. Panels are content-sized, never wider than the window. */
  widthClass?: string;
  children: React.ReactNode;
}

/**
 * A status-bar pill that opens a panel above itself.
 *
 * Purely presentational: dismissal (click-outside, Escape) is handled once by
 * the status bar that owns the open state, so the pills behave as a single
 * mutually-exclusive group.
 */
export const StatusBarPopover: React.FC<StatusBarPopoverProps> = ({
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
}) => {
  return (
    <div className="relative shrink-0">
      <button
        type="button"
        onClick={onToggle}
        disabled={disabled}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={label}
        title={label}
        className={`flex max-w-full items-center gap-1.5 rounded-md border px-1.5 py-1 transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${
          open
            ? "border-mid-gray/30 bg-mid-gray/15 text-text/90"
            : "border-transparent hover:border-mid-gray/25 hover:bg-mid-gray/10 hover:text-text/90"
        }`}
      >
        {trigger}
        <ChevronDown
          className={`h-3 w-3 shrink-0 transition-transform ${open ? "rotate-180" : ""}`}
        />
      </button>

      {open && (
        <div
          role="dialog"
          aria-label={title}
          className={`absolute bottom-full start-0 z-50 mb-2 ${widthClass} overflow-hidden rounded-lg border border-mid-gray/25 bg-background shadow-xl`}
        >
          <div className="flex items-center justify-between gap-2 border-b border-mid-gray/20 px-3 py-2">
            <div className="min-w-0">
              <h3 className="truncate text-xs font-semibold uppercase tracking-wide text-text/70">
                {title}
              </h3>
              {subtitle && (
                <div className="mt-0.5 text-[11px] leading-snug text-text/45">
                  {subtitle}
                </div>
              )}
            </div>
            {headerAction}
          </div>
          {children}
        </div>
      )}
    </div>
  );
};

export default StatusBarPopover;
