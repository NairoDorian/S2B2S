import { createSignal, createEffect } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { commands } from "@/bindings";
import { sessionToast as toast } from "@/lib/sessionToast";
import { SettingContainer } from "../../ui/SettingContainer";
import { PathDisplay } from "../../ui/PathDisplay";

interface LogDirectoryProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

export const LogDirectory = (props: LogDirectoryProps) => {
  const { t } = useTranslation();
  const [logDir, setLogDir] = createSignal<string>("");
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);

  createEffect(
    () => undefined,
    () => {
      commands
        .getLogDirPath()
        .then((result) => {
          if (result.status === "ok") {
            setLogDir(result.data);
          } else {
            setError(result.error);
          }
        })
        .catch((err) => {
          setError(
            err && typeof err === "object" && "message" in err
              ? String(err.message)
              : t("errors.loadDirectoryUnknown"),
          );
        })
        .finally(() => setLoading(false));
    },
  );

  const handleOpen = async () => {
    if (!logDir()) return;
    try {
      const result = await commands.openLogDir();
      if (result.status === "error") {
        console.error("Failed to open log directory:", result.error);
        toast.error(result.error);
      }
    } catch (openError) {
      console.error("Failed to open log directory:", openError);
      toast.error(String(openError));
    }
  };

  return (
    <SettingContainer
      title={t("settings.debug.logDirectory.title")}
      description={t("settings.debug.logDirectory.description")}
      descriptionMode={props.descriptionMode ?? "tooltip"}
      grouped={props.grouped ?? false}
      layout="stacked"
    >
      {/* Theme tokens, not hardcoded grays / reds that glow on the dark
          theme — the same classes AppDataDirectory uses. */}
      {loading() ? (
        <div class="animate-pulse">
          <div class="h-8 bg-mid-gray/10 rounded" />
        </div>
      ) : error() ? (
        <div class="p-3 bg-red-500/10 border border-red-200/30 rounded text-xs text-red-400">
          {t("errors.loadDirectory", { error: error() })}
        </div>
      ) : (
        <PathDisplay path={logDir()} onOpen={handleOpen} disabled={!logDir()} />
      )}
    </SettingContainer>
  );
};
