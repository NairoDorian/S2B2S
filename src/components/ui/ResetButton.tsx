import ResetIcon from "../icons/ResetIcon";
import type { JSX } from "@solidjs/web";

interface ResetButtonProps {
  onClick: () => void;
  disabled?: boolean;
  class?: string;
  ariaLabel?: string;
  children?: JSX.Element;
}

export const ResetButton = (props: ResetButtonProps): JSX.Element => {
  const disabled = () => props.disabled ?? false;
  const className = () => props.class ?? "";
  return (
    <button
      type="button"
      aria-label={props.ariaLabel}
      class={`p-1 rounded-md border border-transparent transition-all duration-150 ${disabled() ? "opacity-50 cursor-not-allowed text-text/40" : "hover:bg-accent/30 active:bg-accent/50 active:translate-y-[1px] hover:cursor-pointer hover:border-accent text-text/80"} ${className()}`}
      onClick={() => props.onClick()}
      disabled={disabled()}
    >
      {props.children ?? <ResetIcon />}
    </button>
  );
};
