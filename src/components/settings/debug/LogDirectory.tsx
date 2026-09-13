import { createSignal, createEffect } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { commands } from "@/bindings";
import { SettingContainer } from "../../ui/SettingContainer";
import { PathDisplay } from "../../ui/PathDisplay";

interface LogDirectoryProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

export const LogDirectory = ({
  descriptionMode = "tooltip",
  grouped = false,
}: LogDirectoryProps) => {
  const { t } = useTranslation();
  const [logDir, setLogDir] = createSignal<string>("");
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);

  createEffect(
    () => undefined,
    () => {
      const loadLogDirectory = async () => {
        try {
          const result = await commands.getLogDirPath();
          if (result.status === "ok") {
            setLogDir(result.data);
          } else {
            setError(result.error);
          }
        } catch (err) {
          const errorMessage =
            err && typeof err === "object" && "message" in err
              ? String(err.message)
              : "Failed to load log directory";
          setError(errorMessage);
        } finally {
          setLoading(false);
        }
      };

      loadLogDirectory();
    },
  );

  const handleOpen = async () => {
    if (!logDir()) return;
    try {
      await commands.openLogDir();
    } catch (openError) {
      console.error("Failed to open log directory:", openError);
    }
  };

  return (
    <SettingContainer
      title={t("settings.debug.logDirectory.title")}
      description={t("settings.debug.logDirectory.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
      layout="stacked"
    >
      {loading() ? (
        <div class="animate-pulse">
          <div class="h-8 bg-gray-100 rounded" />
        </div>
      ) : error() ? (
        <div class="p-3 bg-red-50 border border-red-200 rounded text-xs text-red-600">
          {t("errors.loadDirectory", { error: error() })}
        </div>
      ) : (
        <PathDisplay path={logDir()} onOpen={handleOpen} disabled={!logDir()} />
      )}
    </SettingContainer>
  );
};
