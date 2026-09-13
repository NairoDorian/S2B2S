import { Dynamic } from "@solidjs/web";
import {
  AlertCircle,
  AlertTriangle,
  Info,
  CheckCircle,
} from "@/components/icons/lucide";
import type { JSX } from "@solidjs/web";

type AlertVariant = "error" | "warning" | "info" | "success";

interface AlertProps {
  variant?: AlertVariant;
  contained?: boolean;
  children: JSX.Element;
  class?: string;
}

const variantStyles: Record<
  AlertVariant,
  { container: string; icon: string; text: string }
> = {
  error: {
    container: "bg-red-500/10",
    icon: "text-red-500",
    text: "text-red-400",
  },
  warning: {
    container: "bg-yellow-500/10",
    icon: "text-yellow-500",
    text: "text-yellow-400",
  },
  info: {
    container: "bg-blue-500/10",
    icon: "text-blue-500",
    text: "text-blue-400",
  },
  success: {
    container: "bg-green-500/10",
    icon: "text-green-500",
    text: "text-green-400",
  },
};

const variantIcons: Record<
  AlertVariant,
  (props: { class?: string }) => JSX.Element
> = {
  error: AlertCircle,
  warning: AlertTriangle,
  info: Info,
  success: CheckCircle,
};

export const Alert = (props: AlertProps): JSX.Element => {
  const variant = () => props.variant ?? "error";
  const styles = () => variantStyles[variant()];
  const className = () => props.class ?? "";

  return (
    <div
      class={`flex items-start gap-3 p-4 ${styles().container} ${props.contained ? "" : "rounded-lg"} ${className()}`}
    >
      <Dynamic
        component={variantIcons[variant()]}
        class={`w-5 h-5 shrink-0 mt-0.5 ${styles().icon}`}
      />
      <p class={`text-sm ${styles().text}`}>{props.children}</p>
    </div>
  );
};
