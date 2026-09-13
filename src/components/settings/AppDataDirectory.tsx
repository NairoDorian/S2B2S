import { createSignal, createEffect } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { commands } from "@/bindings";
import { SettingContainer } from "../ui/SettingContainer";
import { PathDisplay } from "../ui/PathDisplay";

interface AppDataDirectoryProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

export const AppDataDirectory = (props: AppDataDirectoryProps) => {
  const { descriptionMode = "inline", grouped = false } = props;
  const { t } = useTranslation();
  const [appDirPath, setAppDirPath] = createSignal<string>("");
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);

  createEffect(
    () => undefined,
    () => {
      const loadAppDirectory = async () => {
        try {
          const result = await commands.getAppDirPath();
          if (result.status === "ok") {
            setAppDirPath(result.data);
          } else {
            setError(result.error);
          }
        } catch (err) {
          setError(
            err instanceof Error ? err.message : "Failed to load app directory",
          );
        } finally {
          setLoading(false);
        }
      };

      loadAppDirectory();
    },
  );

  const handleOpen = async () => {
    if (!appDirPath()) return;
    try {
      await commands.openAppDataDir();
    } catch (openError) {
      console.error("Failed to open app data directory:", openError);
    }
  };

  if (loading()) {
    return (
      <div class="animate-pulse">
        <div class="h-4 bg-gray-200 rounded w-1/3 mb-2"></div>
        <div class="h-8 bg-gray-100 rounded"></div>
      </div>
    );
  }

  if (error()) {
    return (
      <div class="p-4 bg-red-50 border border-red-200 rounded-lg">
        <p class="text-red-600 text-sm">
          {t("errors.loadDirectory", { error: error() })}
        </p>
      </div>
    );
  }

  return (
    <SettingContainer
      title={t("settings.about.appDataDirectory.title")}
      description={t("settings.about.appDataDirectory.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
      layout="stacked"
    >
      <PathDisplay
        path={appDirPath()}
        onOpen={handleOpen}
        disabled={!appDirPath()}
      />
    </SettingContainer>
  );
};
