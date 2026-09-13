import { createSignal, createEffect, Show } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { commands } from "@/bindings";
import { SettingContainer } from "../ui/SettingContainer";
import { PathDisplay } from "../ui/PathDisplay";

interface AppDataDirectoryProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const AppDataDirectory = (props: AppDataDirectoryProps) => {
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

  // Branches live in the JSX: a component body runs once, so the body-level
  // `if (loading()) return <skeleton/>` this replaces rendered the skeleton
  // forever — the "blank white block" between Source Code and Log Directory.
  // The skeleton itself uses theme tokens, not hardcoded grays that glow on
  // the dark theme.
  return (
    <Show
      when={!loading()}
      fallback={
        <div class="animate-pulse">
          <div class="h-4 bg-mid-gray/20 rounded w-1/3 mb-2"></div>
          <div class="h-8 bg-mid-gray/10 rounded"></div>
        </div>
      }
    >
      <Show
        when={!error()}
        fallback={
          <div class="p-4 bg-red-500/10 border border-red-200/30 rounded-lg">
            <p class="text-red-400 text-sm">
              {t("errors.loadDirectory", { error: error() })}
            </p>
          </div>
        }
      >
        <SettingContainer
          title={t("settings.about.appDataDirectory.title")}
          description={t("settings.about.appDataDirectory.description")}
          descriptionMode={props.descriptionMode}
          grouped={props.grouped ?? false}
          layout="stacked"
        >
          <PathDisplay
            path={appDirPath()}
            onOpen={handleOpen}
            disabled={!appDirPath()}
          />
        </SettingContainer>
      </Show>
    </Show>
  );
};
