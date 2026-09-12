import { APP_NAME } from "@/lib/appIdentity";
import BrandMark from "./BrandMark";

/**
 * The mark beside the product name — the lockup used wherever the app
 * introduces itself (the sidebar header, the onboarding screens).
 *
 * The name is read from `APP_NAME`, which `scripts/app-meta.ts` generates, so
 * renaming the product renames this text too. That is the whole reason the
 * wordmark is typeset rather than drawn: a hand-drawn wordmark is the one
 * asset a rename cannot regenerate, and it is the reason the old wordmark had
 * to be replaced by hand.
 */
const BrandLockup = ({
  /** Size of the mark in pixels; the text scales with it. */
  size = 24,
  /** Longest the lockup may render, in pixels. The text truncates past it. */
  maxWidth,
  className,
}: {
  size?: number;
  maxWidth?: number | string;
  className?: string;
}) => (
  <span
    className={`inline-flex items-center gap-2 select-none ${className ?? ""}`}
    style={maxWidth === undefined ? undefined : { maxWidth }}
  >
    <BrandMark size={size} className="shrink-0" />
    <span
      className="font-semibold tracking-[0.18em] text-text whitespace-nowrap truncate"
      // Half the mark, rounded: the text sits optically centred against it.
      style={{ fontSize: Math.round(size * 0.62), lineHeight: 1 }}
    >
      {APP_NAME}
    </span>
  </span>
);

export default BrandLockup;
