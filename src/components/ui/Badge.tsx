import type { JSX } from "@solidjs/web";
interface BadgeProps {
  children: JSX.Element;
  variant?: "primary" | "success" | "secondary";
  class?: string;
}

const Badge = (props: BadgeProps): JSX.Element => {
  const variantClasses = {
    primary: "bg-accent",
    success: "bg-green-500/20 text-green-400",
    secondary: "bg-mid-gray/20 text-text/70",
  };
  const variant = () => props.variant ?? "primary";
  const className = () => props.class ?? "";

  return (
    <span
      class={`inline-flex items-center px-3 py-1 rounded-full text-xs font-medium ${variantClasses[variant()]} ${className()}`}
    >
      {props.children}
    </span>
  );
};

export default Badge;
