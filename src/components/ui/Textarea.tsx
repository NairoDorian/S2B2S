import { omit } from "solid-js";
import type { JSX } from "@solidjs/web";
interface TextareaProps extends JSX.TextareaHTMLAttributes<HTMLTextAreaElement> {
  variant?: "default" | "compact";
}

export const Textarea = (props: TextareaProps): JSX.Element => {
  const variant = () => props.variant ?? "default";
  const rest = omit(props, "class", "variant");
  const baseClasses =
    "px-2 py-1 text-sm font-semibold bg-mid-gray/10 border border-mid-gray/80 rounded-md text-start transition-[background-color,border-color] duration-150 hover:bg-accent/10 hover:border-accent focus:outline-none focus:bg-accent/10 focus:border-accent resize-y";

  const variantClasses = {
    default: "px-3 py-2 min-h-[100px]",
    compact: "px-2 py-1 min-h-[80px]",
  } as const;

  return (
    <textarea
      {...rest}
      class={`${baseClasses} ${variantClasses[variant()]} ${props.class ?? ""}`}
    />
  );
};
