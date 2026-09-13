import type { JSX } from "@solidjs/web";
import { omit } from "solid-js";

interface ButtonProps extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?:
    | "primary"
    | "primary-soft"
    | "secondary"
    | "warning"
    | "danger"
    | "danger-ghost"
    | "ghost";
  size?: "sm" | "md" | "lg";
}

export const Button = (props: ButtonProps) => {
  const rest = omit(props, "variant", "size", "class", "children");
  const variant = () => props.variant ?? "primary";
  const size = () => props.size ?? "md";

  const baseClasses =
    "font-medium rounded-lg border focus:outline-none transition-colors disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer";

  const variantClasses: Record<string, string> = {
    primary:
      "text-white bg-background-ui border-background-ui hover:bg-background-ui/80 hover:border-background-ui/80 focus:ring-1 focus:ring-background-ui",
    "primary-soft":
      "text-text bg-accent/20 border-transparent hover:bg-accent/30 focus:ring-1 focus:ring-accent",
    secondary:
      "bg-mid-gray/10 border-mid-gray/20 hover:bg-background-ui/30 hover:border-accent focus:outline-none",
    warning:
      "text-text bg-mid-gray/10 border-mid-gray/20 hover:bg-warning/15 hover:border-warning focus:ring-1 focus:ring-warning",
    danger:
      "text-white bg-red-600 border-mid-gray/20 hover:bg-red-700 hover:border-red-700 focus:ring-1 focus:ring-red-500",
    "danger-ghost":
      "text-red-400 border-transparent hover:text-red-300 hover:bg-red-500/10 focus:bg-red-500/20",
    ghost:
      "text-current border-transparent hover:bg-mid-gray/10 hover:border-accent focus:bg-mid-gray/20",
  };

  const sizeClasses: Record<string, string> = {
    sm: "px-2 py-1 text-xs",
    md: "px-4 py-[5px] text-sm",
    lg: "px-4 py-2 text-base",
  };

  return (
    <button
      {...rest}
      class={[
        baseClasses,
        variantClasses[variant()],
        sizeClasses[size()],
        props.class ?? "",
      ]}
    >
      {props.children}
    </button>
  );
};
