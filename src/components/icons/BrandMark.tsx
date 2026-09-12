import { brandStrokes, GRID, type BrandStroke } from "@/lib/brandMark";

/**
 * The application mark: a rounded-square badge around a slashed zero.
 *
 * The drawing itself lives in `@/lib/brandMark` — this component only turns it
 * into elements, so the sidebar icon, the tray icon and the installer icon are
 * the same geometry. Strokes are `currentColor` with no fill, which is what
 * lets the mark sit in a row of line icons and inherit the accent colour.
 *
 * `size` is the rendered pixel size and picks the optical variant (stroke
 * weight, and whether the slash is drawn) — pass the size you are actually
 * drawing at, not a scale factor.
 */
const BrandMark = ({
  size = 24,
  width,
  height,
  className,
  color,
  title,
}: {
  /** Rendered size in pixels. Also selects the optical variant. */
  size?: number | string;
  /** Accepted so the mark can stand in for a sized icon; `size` wins. */
  width?: number | string;
  height?: number | string;
  className?: string;
  /** Any CSS colour. Defaults to `currentColor`, so it inherits. */
  color?: string;
  /** Accessible name. Omit for a decorative mark beside a text label. */
  title?: string;
}) => {
  // `width`/`height` come from the generic icon contract used across the app;
  // the optical variant needs one number, so the first numeric one wins.
  const px = Number(size ?? width ?? height) || 24;
  const strokes: BrandStroke[] = brandStrokes(px);

  return (
    <svg
      width={width ?? size}
      height={height ?? size}
      viewBox={`0 0 ${GRID} ${GRID}`}
      className={className}
      role={title ? "img" : undefined}
      aria-label={title}
      aria-hidden={title ? undefined : true}
      xmlns="http://www.w3.org/2000/svg"
    >
      {strokes.map((stroke) => (
        <path
          key={stroke.id}
          d={stroke.d}
          fill="none"
          stroke={color ?? "currentColor"}
          strokeWidth={stroke.width}
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      ))}
    </svg>
  );
};

export default BrandMark;
