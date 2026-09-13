import { createSignal, createEffect, For } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { commands } from "@/bindings";
import { SettingContainer } from "../../ui/SettingContainer";

interface DebugPathsProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

const SETTINGS_STORE_FILE = "settings_store.json";

export const DebugPaths = ({
  descriptionMode = "inline",
  grouped = false,
}: DebugPathsProps) => {
  const { t } = useTranslation();
  const [appDir, setAppDir] = createSignal<string | null>(null);
  const [error, setError] = createSignal<string | null>(null);

  createEffect(
    () => undefined,
    () => {
      let cancelled = false;
      const load = async () => {
        try {
          const result = await commands.getAppDirPath();
          if (cancelled) return;
          if (result.status === "ok") {
            setAppDir(result.data);
          } else {
            setError(result.error);
          }
        } catch (e) {
          if (!cancelled) {
            setError(e instanceof Error ? e.message : String(e));
          }
        }
      };
      load();
      return () => {
        cancelled = true;
      };
    },
  );

  const rows: [string, string | null][] = [
    [t("settings.debug.paths.appData"), appDir()],
    [t("settings.debug.paths.models"), appDir() ? `${appDir()}/models` : null],
    [
      t("settings.debug.paths.settings"),
      appDir() ? `${appDir()}/${SETTINGS_STORE_FILE}` : null,
    ],
  ];

  return (
    <SettingContainer
      title={t("settings.debug.paths.title")}
      description={t("settings.debug.paths.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    >
      <div class="text-sm text-gray-600 space-y-2">
        <For each={rows}>
          {([label, path]) => (
            <div>
              <span class="font-medium">{label}</span>{" "}
              <span class="font-mono text-xs select-text">
                {path ?? (error() ? `— ${error()}` : "…")}
              </span>
            </div>
          )}
        </For>
      </div>
    </SettingContainer>
  );
};
