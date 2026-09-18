import { createSignal, createEffect, Show } from "solid-js";
import { Tooltip } from "./Tooltip";
import type { JSX } from "@solidjs/web";

interface SettingContainerProps {
  title: string;
  description: string;
  children: JSX.Element;
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  layout?: "horizontal" | "stacked";
  disabled?: boolean;
  tooltipPosition?: "top" | "bottom";
}

export const SettingContainer = (props: SettingContainerProps): JSX.Element => {
  const [showTooltip, setShowTooltip] = createSignal(false);
  let tooltipRef: HTMLDivElement | null = null;

  // The compute tracks `showTooltip`; the apply only touches the DOM and
  // returns its own teardown, so a re-run replaces the listener instead of
  // stacking one (Solid 2: cleanup is returned, not registered with
  // `onCleanup` inside).
  createEffect(
    () => showTooltip(),
    (show) => {
      if (!show) return;
      const handleClickOutside = (event: MouseEvent) => {
        if (tooltipRef && !tooltipRef.contains(event.target as Node)) {
          setShowTooltip(false);
        }
      };
      document.addEventListener("mousedown", handleClickOutside);
      return () =>
        document.removeEventListener("mousedown", handleClickOutside);
    },
  );

  const toggleTooltip = () => {
    setShowTooltip(!showTooltip());
  };

  // Accessors, not destructured locals: a component body runs once, so a
  // destructured prop is a mount-time snapshot and the row would never follow
  // a settings or layout change.
  const grouped = () => props.grouped ?? false;
  const disabled = () => props.disabled ?? false;
  const stacked = () => (props.layout ?? "horizontal") === "stacked";
  const tooltipMode = () => (props.descriptionMode ?? "tooltip") === "tooltip";

  const containerClasses = () =>
    grouped() ? "px-4 p-2" : "px-4 p-2 rounded-lg border border-mid-gray/20";

  const horizontalContainerClasses = () =>
    grouped()
      ? "flex items-center justify-between min-h-12 px-4 p-2"
      : "flex items-center justify-between min-h-12 px-4 p-2 rounded-lg border border-mid-gray/20";

  const Title = () => (
    <h3 class={`text-sm font-medium ${disabled() ? "opacity-50" : ""}`}>
      {props.title}
    </h3>
  );

  // The description text of inline mode; the info icon of tooltip mode.
  const DescriptionOrIcon = () => (
    <Show
      when={tooltipMode()}
      fallback={
        <p class={`text-sm ${disabled() ? "opacity-50" : ""}`}>
          {props.description}
        </p>
      }
    >
      <div
        ref={(el) => {
          tooltipRef = el;
        }}
        class="relative inline-flex items-center"
        onMouseEnter={() => setShowTooltip(true)}
        onMouseLeave={() => setShowTooltip(false)}
      >
        <button
          type="button"
          onClick={toggleTooltip}
          class="text-mid-gray cursor-help hover:text-accent transition-colors duration-200 select-none p-0 border-0 bg-transparent flex items-center justify-center"
          aria-label="More information"
        >
          <svg
            class="w-4 h-4"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
            aria-hidden="true"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width={2}
              d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
            />
          </svg>
        </button>
        {showTooltip() && (
          <Tooltip
            targetRef={{ current: tooltipRef }}
            position={props.tooltipPosition ?? "top"}
          >
            <p class="text-sm text-center leading-relaxed">
              {props.description}
            </p>
          </Tooltip>
        )}
      </div>
    </Show>
  );

  return (
    <Show
      when={stacked()}
      fallback={
        <div class={horizontalContainerClasses()}>
          <div class="max-w-2/3">
            <Show
              when={tooltipMode()}
              fallback={
                <>
                  <Title />
                  <DescriptionOrIcon />
                </>
              }
            >
              <div class="flex items-center gap-2">
                <Title />
                <DescriptionOrIcon />
              </div>
            </Show>
          </div>
          <div class="relative">{props.children}</div>
        </div>
      }
    >
      <div class={containerClasses()}>
        <Show
          when={tooltipMode()}
          fallback={
            <div class="mb-2">
              <Title />
              <DescriptionOrIcon />
            </div>
          }
        >
          <div class="flex items-center gap-2 mb-2">
            <Title />
            <DescriptionOrIcon />
          </div>
        </Show>
        <div class="w-full">{props.children}</div>
      </div>
    </Show>
  );
};
