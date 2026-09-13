import { APP_NAME } from "@/lib/appIdentity";
import BrandMark from "./BrandMark";
import type { JSX } from "@solidjs/web";

interface BrandLockupProps {
  size?: number;
  maxWidth?: number | string;
  class?: string;
}

const BrandLockup = (props: BrandLockupProps): JSX.Element => {
  const size = () => props.size ?? 24;

  return (
    <span
      class={`inline-flex items-center gap-2 select-none ${props.class ?? ""}`}
      style={
        props.maxWidth === undefined
          ? undefined
          : {
              "max-width":
                typeof props.maxWidth === "number"
                  ? `${props.maxWidth}px`
                  : props.maxWidth,
            }
      }
    >
      <BrandMark size={size()} class="shrink-0" />
      <span
        class="font-semibold tracking-[0.18em] text-text whitespace-nowrap truncate"
        style={{
          "font-size": `${Math.round(size() * 0.62)}px`,
          "line-height": 1,
        }}
      >
        {APP_NAME}
      </span>
    </span>
  );
};

export default BrandLockup;
