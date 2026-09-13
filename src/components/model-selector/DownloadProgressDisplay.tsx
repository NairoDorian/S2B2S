import { Show } from "solid-js";
import { ProgressBar } from "../shared";
import type { JSX } from "@solidjs/web";

interface DownloadProgress {
  model_id: string;
  downloaded: number;
  total: number;
  percentage: number;
}
interface DownloadStats {
  startTime: number;
  lastUpdate: number;
  totalDownloaded: number;
  speed: number;
}
interface DownloadProgressDisplayProps {
  downloadProgress: Record<string, DownloadProgress>;
  downloadStats: Record<string, DownloadStats>;
  class?: string;
}

const DownloadProgressDisplay = (
  props: DownloadProgressDisplayProps,
): JSX.Element | null => {
  // Live reads: download progress updates continuously, so a body-level
  // snapshot of the maps would freeze the bar at its first value.
  const progressData = () =>
    Object.entries(props.downloadProgress).map(([id, progress]) => ({
      id: progress.model_id || id,
      percentage: progress.percentage,
      speed: props.downloadStats[progress.model_id]?.speed,
    }));

  return (
    <Show when={progressData().length > 0}>
      <ProgressBar
        progress={progressData()}
        class={props.class ?? ""}
        showSpeed={progressData().length === 1}
        size="medium"
      />
    </Show>
  );
};

export default DownloadProgressDisplay;
