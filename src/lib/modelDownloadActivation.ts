import { commands } from "@/bindings";

interface ModelSelectionSnapshot {
  modelId: string;
  provider: string;
}

interface ModelDownloadActivationIntent extends ModelSelectionSnapshot {
  generation: number;
  modelIdToActivate: string;
}

let latestGeneration = 0;
let pendingIntent: ModelDownloadActivationIntent | null = null;

const captureModelSelection =
  async (): Promise<ModelSelectionSnapshot | null> => {
    try {
      const [modelResult, settingsResult] = await Promise.all([
        commands.getCurrentModel(),
        commands.getAppSettings(),
      ]);

      if (modelResult.status !== "ok" || settingsResult.status !== "ok") {
        return null;
      }

      return {
        modelId: modelResult.data,
        provider: "local",
      };
    } catch {
      return null;
    }
  };

/**
 * Record that this download is now the only one allowed to auto-activate.
 * The snapshot prevents a completion from overriding a later manual model choice.
 */
export const beginModelDownloadActivationIntent = async (
  modelId: string,
): Promise<void> => {
  const generation = ++latestGeneration;
  const snapshot = await captureModelSelection();

  if (generation !== latestGeneration) {
    return;
  }

  pendingIntent = snapshot
    ? {
        ...snapshot,
        generation,
        modelIdToActivate: modelId,
      }
    : null;
};

/** Cancel an intent only when it belongs to this failed/cancelled download. */
export const cancelModelDownloadActivationIntent = (modelId: string): void => {
  if (pendingIntent?.modelIdToActivate !== modelId) {
    return;
  }

  latestGeneration += 1;
  pendingIntent = null;
};

/** Any explicit user selection supersedes every pending download activation. */
export const invalidateModelDownloadActivationIntent = (): void => {
  latestGeneration += 1;
  pendingIntent = null;
};
