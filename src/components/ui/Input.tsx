import { omit } from "solid-js";
import type { JSX } from "@solidjs/web";
interface InputProps extends JSX.InputHTMLAttributes<HTMLInputElement> {
  variant?: "default" | "compact";
}

export const Input = (props: InputProps): JSX.Element => {
  const variant = () => props.variant ?? "default";
  const disabled = () => props.disabled ?? false;
  const rest = omit(props, "class", "variant", "disabled");
  const baseClasses =
    "px-2 py-1 text-sm font-semibold bg-mid-gray/10 border border-mid-gray/80 rounded-md text-start transition-all duration-150";

  const interactiveClasses = () =>
    disabled()
      ? "opacity-60 cursor-not-allowed bg-mid-gray/10 border-mid-gray/40"
      : "hover:bg-accent/10 hover:border-accent focus:outline-none focus:bg-accent/20 focus:border-accent";

  const variantClasses = {
    default: "px-3 py-2",
    compact: "px-2 py-1",
  } as const;

  return (
    <input
      {...rest}
      class={`${baseClasses} ${variantClasses[variant()]} ${interactiveClasses()} ${props.class ?? ""}`}
      disabled={disabled()}
    />
  );
};
