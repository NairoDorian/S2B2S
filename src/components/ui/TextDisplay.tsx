import { createSignal } from "solid-js";
import { SettingContainer } from "./SettingContainer";
import type { JSX } from "@solidjs/web";

interface TextDisplayProps {
  label: string;
  description: string;
  value: string;
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  placeholder?: string;
  copyable?: boolean;
  monospace?: boolean;
  onCopy?: (value: string) => void;
}

export const TextDisplay = (props: TextDisplayProps): JSX.Element => {
  const {
    label,
    description,
    value,
    descriptionMode = "tooltip",
    grouped = false,
    placeholder = "Not available",
    copyable = false,
    monospace = false,
    onCopy,
  } = props;
  const [showCopied, setShowCopied] = createSignal(false);

  const handleCopy = async () => {
    if (!value || !copyable) return;
    try {
      await navigator.clipboard.writeText(value);
      setShowCopied(true);
      setTimeout(() => setShowCopied(false), 1500);
      if (onCopy) {
        onCopy(value);
      }
    } catch (err) {
      console.error("Failed to copy to clipboard:", err);
    }
  };

  const displayValue = value || placeholder;
  const textClasses = monospace ? "font-mono break-all" : "break-words";

  return (
    <SettingContainer
      title={label}
      description={description}
      descriptionMode={descriptionMode}
      grouped={grouped}
      layout="stacked"
    >
      <div class="flex items-center space-x-2">
        <div class="flex-1 min-w-0">
          <div
            class={`px-2 min-h-8 flex items-center bg-mid-gray/10 border border-mid-gray/80 rounded-md text-xs ${textClasses} ${!value ? "opacity-60" : ""}`}
          >
            {displayValue}
          </div>
        </div>
        {copyable && value && (
          <button
            onClick={handleCopy}
            class="flex items-center justify-center px-2 py-1 w-12 min-h-8 text-xs font-semibold bg-mid-gray/10 hover:bg-accent/10 border border-mid-gray/80 hover:border-accent hover:text-accent rounded-md transition-all duration-150 shrink-0 cursor-pointer"
            title="Copy to clipboard"
          >
            {showCopied() ? (
              <div class="flex items-center space-x-1">
                <svg
                  class="w-4 h-4"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width={2}
                    d="M5 13l4 4L19 7"
                  />
                </svg>
              </div>
            ) : (
              "Copy"
            )}
          </button>
        )}
      </div>
    </SettingContainer>
  );
};
