import React, { useEffect, useRef, useState } from "react";
import { Tooltip } from "./Tooltip";

interface SettingContainerProps {
  title: string;
  description: string;
  children: React.ReactNode;
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  layout?: "horizontal" | "stacked";
  disabled?: boolean;
  tooltipPosition?: "top" | "bottom";
}

const TooltipIcon: React.FC<{
  tooltipRef: React.RefObject<HTMLDivElement | null>;
  showTooltip: boolean;
  toggleTooltip: () => void;
  setShowTooltip: (v: boolean) => void;
  description: string;
  position: "top" | "bottom";
}> = ({ tooltipRef, showTooltip, toggleTooltip, setShowTooltip, description, position }) => (
  <div
    ref={tooltipRef}
    className="relative"
    onMouseEnter={() => setShowTooltip(true)}
    onMouseLeave={() => setShowTooltip(false)}
    onClick={toggleTooltip}
  >
    <svg
      className="w-4 h-4 text-mid-gray cursor-help hover:text-logo-primary transition-colors duration-200 select-none"
      fill="none"
      stroke="currentColor"
      viewBox="0 0 24 24"
      aria-label="More information"
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          toggleTooltip();
        }
      }}
    >
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
      />
    </svg>
    {showTooltip && (
      <Tooltip targetRef={tooltipRef} position={position}>
        <p className="text-sm text-center leading-relaxed">{description}</p>
      </Tooltip>
    )}
  </div>
);

export const SettingContainer: React.FC<SettingContainerProps> = ({
  title,
  description,
  children,
  descriptionMode = "tooltip",
  grouped = false,
  layout = "horizontal",
  disabled = false,
  tooltipPosition = "top",
}) => {
  const [showTooltip, setShowTooltip] = useState(false);
  const tooltipRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (
        tooltipRef.current &&
        !tooltipRef.current.contains(event.target as Node)
      ) {
        setShowTooltip(false);
      }
    };

    if (showTooltip) {
      document.addEventListener("mousedown", handleClickOutside);
      return () =>
        document.removeEventListener("mousedown", handleClickOutside);
    }
  }, [showTooltip]);

  const toggleTooltip = () => setShowTooltip((v) => !v);

  const containerClasses = grouped
    ? "px-4 p-2"
    : "px-4 p-2 rounded-lg border border-mid-gray/20";

  const commonTooltipProps = {
    tooltipRef,
    showTooltip,
    toggleTooltip,
    setShowTooltip,
    description,
    position: tooltipPosition,
  } as const;

  if (layout === "stacked") {
    return (
      <div className={containerClasses}>
        <div className="flex items-center gap-2 mb-2">
          <h3 className={`text-sm font-medium ${disabled ? "opacity-50" : ""}`}>
            {title}
          </h3>
          {descriptionMode === "tooltip" && <TooltipIcon {...commonTooltipProps} />}
        </div>
        {descriptionMode === "inline" && (
          <p className={`text-sm mb-2 ${disabled ? "opacity-50" : ""}`}>{description}</p>
        )}
        <div className="w-full">{children}</div>
      </div>
    );
  }

  const hContainer = grouped
    ? "flex items-center justify-between px-4 p-2"
    : "flex items-center justify-between px-4 p-2 rounded-lg border border-mid-gray/20";

  return (
    <div className={hContainer}>
      <div className="max-w-2/3">
        <div className="flex items-center gap-2">
          <h3 className={`text-sm font-medium ${disabled ? "opacity-50" : ""}`}>
            {title}
          </h3>
          {descriptionMode === "tooltip" && <TooltipIcon {...commonTooltipProps} />}
        </div>
        {descriptionMode === "inline" && (
          <p className={`text-sm ${disabled ? "opacity-50" : ""}`}>{description}</p>
        )}
      </div>
      <div className="relative">{children}</div>
    </div>
  );
};
