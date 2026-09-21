import type { JSX } from "@solidjs/web";
interface ModelStatusButtonProps {
  status: ModelStatus;
  displayText: string;
  isDropdownOpen: boolean;
  onClick: () => void;
  class?: string;
  // Draw the model name inside the app's active box (`border-accent
  // bg-accent/10 text-accent`, the same treatment an active sidebar section
  // gets). Set for the one family whose streaming behaviour is distinctive —
  // R2T2, whose continuous chunk-size control the model menu also swaps in — so
  // the bar shows at a glance not just that a model is loaded but that it is
  // *this* one. The caller decides; this stays a dumb component.
  highlight?: boolean;
}

type ModelStatus =
  | "ready"
  | "loading"
  | "downloading"
  | "verifying"
  | "error"
  | "unloaded"
  | "none";

const getStatusColor = (status: ModelStatus): string => {
  switch (status) {
    case "ready":
      return "bg-green-400";
    case "loading":
      return "bg-yellow-400 animate-pulse";
    case "downloading":
      return "bg-accent animate-pulse";
    case "verifying":
      return "bg-orange-400 animate-pulse";
    case "error":
      return "bg-red-400";
    case "unloaded":
      return "bg-mid-gray/60";
    case "none":
      return "bg-red-400";
    default:
      return "bg-mid-gray/60";
  }
};

const ModelStatusButton = (props: ModelStatusButtonProps): JSX.Element => {
  return (
    <button
      onClick={() => props.onClick()}
      class={`flex items-center gap-2 hover:text-text/80 transition-colors ${props.class ?? ""}`}
      title={`Model status: ${props.displayText}`}
    >
      <div class={`w-2 h-2 rounded-full ${getStatusColor(props.status)}`} />
      <span
        class={
          props.highlight
            ? "max-w-28 truncate rounded border-2 border-accent bg-accent/10 px-1.5 py-0.5 text-accent"
            : "max-w-28 truncate"
        }
      >
        {props.displayText}
      </span>
      <svg
        class={`w-3 h-3 transition-transform ${props.isDropdownOpen ? "rotate-180" : ""}`}
        fill="none"
        stroke="currentColor"
        viewBox="0 0 24 24"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width={2}
          d="M19 9l-7 7-7-7"
        />
      </svg>
    </button>
  );
};

export default ModelStatusButton;
