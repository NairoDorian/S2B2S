import { For } from "solid-js";
import { brandStrokes, GRID, type BrandStroke } from "@/lib/brandMark";
import type { JSX } from "@solidjs/web";

interface BrandMarkProps {
  size?: number | string;
  width?: number | string;
  height?: number | string;
  class?: string;
  color?: string;
  title?: string;
}

const BrandMark = (props: BrandMarkProps): JSX.Element => {
  const px = () => Number(props.size ?? props.width ?? props.height) || 24;
  const strokes = (): BrandStroke[] => brandStrokes(px());

  return (
    <svg
      width={props.width ?? props.size}
      height={props.height ?? props.size}
      viewBox={`0 0 ${GRID} ${GRID}`}
      class={props.class}
      role={props.title ? "img" : undefined}
      aria-label={props.title}
      aria-hidden={props.title ? undefined : "true"}
      xmlns="http://www.w3.org/2000/svg"
    >
      <For each={strokes()} keyed={(stroke) => stroke.id}>
        {(stroke) => (
          <path
            d={stroke().d}
            fill="none"
            stroke={props.color ?? "currentColor"}
            stroke-width={stroke().width}
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        )}
      </For>
    </svg>
  );
};

export default BrandMark;
