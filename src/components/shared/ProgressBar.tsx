import { Show, For } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import type { JSX } from "@solidjs/web";

export interface ProgressData {
  id: string;
  percentage: number;
  speed?: number;
  label?: string;
}
interface ProgressBarProps {
  progress: ProgressData[];
  class?: string;
  size?: "small" | "medium" | "large";
  showSpeed?: boolean;
  showLabel?: boolean;
}

const ProgressBar = (props: ProgressBarProps): JSX.Element | null => {
  const { t } = useTranslation();
  // Live reads: `progress` is a fresh array on every download tick, so both
  // the array and each item's fields are read inside the JSX bindings.
  const sizeClasses = {
    small: "w-16 h-1",
    medium: "w-20 h-1.5",
    large: "w-24 h-2",
  };

  return (
    <Show when={props.progress.length > 0}>
      <Show
        when={props.progress.length === 1 ? props.progress[0] : undefined}
        fallback={
          <div class={`flex items-center gap-2 ${props.class ?? ""}`}>
            <div class="flex gap-1">
              <For each={props.progress}>
                {(item) => {
                  const percentage = Math.max(
                    0,
                    Math.min(100, item.percentage),
                  );
                  return (
                    <progress
                      value={percentage}
                      max={100}
                      title={item.label || `${percentage}%`}
                      class="w-3 h-1.5 [&::-webkit-progress-bar]:rounded-full [&::-webkit-progress-bar]:bg-mid-gray/20 [&::-webkit-progress-value]:rounded-full [&::-webkit-progress-value]:bg-accent"
                    />
                  );
                }}
              </For>
            </div>
            <div class="text-xs text-text/60 min-w-fit">
              {t("common.downloadingCount", { count: props.progress.length })}
            </div>
          </div>
        }
      >
        {(item) => {
          const percentage = () =>
            Math.max(0, Math.min(100, item().percentage));
          return (
            <div class={`flex items-center gap-3 ${props.class ?? ""}`}>
              <progress
                value={percentage()}
                max={100}
                class={`${sizeClasses[props.size ?? "medium"]} [&::-webkit-progress-bar]:rounded-full [&::-webkit-progress-bar]:bg-mid-gray/20 [&::-webkit-progress-value]:rounded-full [&::-webkit-progress-value]:bg-accent`}
              />
              <Show when={props.showSpeed || props.showLabel}>
                <div class="text-xs text-text/60 tabular-nums min-w-fit">
                  <Show when={props.showLabel && item().label}>
                    <span class="me-2">{item().label}</span>
                  </Show>
                  <Show
                    when={
                      props.showSpeed &&
                      item().speed !== undefined &&
                      item().speed! > 0
                        ? item().speed
                        : undefined
                    }
                  >
                    {(speed) => (
                      <span>
                        {t("common.downloadSpeed", {
                          speed: speed().toFixed(1),
                        })}
                      </span>
                    )}
                  </Show>
                  <Show
                    when={
                      props.showSpeed &&
                      !(item().speed !== undefined && item().speed! > 0)
                    }
                  >
                    <span>{t("common.downloading")}</span>
                  </Show>
                </div>
              </Show>
            </div>
          );
        }}
      </Show>
    </Show>
  );
};

export default ProgressBar;
