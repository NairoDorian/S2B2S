import { createSignal, createEffect } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { arch, platform } from "@tauri-apps/plugin-os";
import { ProgressBar } from "../shared";
import { useSettings } from "../../hooks/useSettings";
import { commands } from "../../bindings";
import { RELEASES_URL } from "../../lib/appIdentity";
import { resolvePortableInstallerUrl } from "./portableInstaller";

interface UpdateCheckerProps {
  class?: string;
}

const UpdateChecker = (props: UpdateCheckerProps) => {
  const { t } = useTranslation();
  const [isChecking, setIsChecking] = createSignal(false);
  const [updateAvailable, setUpdateAvailable] = createSignal(false);
  const [isInstalling, setIsInstalling] = createSignal(false);
  const [downloadProgress, setDownloadProgress] = createSignal(0);
  const [showUpToDate, setShowUpToDate] = createSignal(false);
  const [showPortableUpdateDialog, setShowPortableUpdateDialog] =
    createSignal(false);
  const [portableInstallerUrl, setPortableInstallerUrl] =
    createSignal<string>(RELEASES_URL);

  const { settings, isLoading, updateChecksLocked } = useSettings();
  // Accessors: these gate the effect below and the footer's status text, and
  // both must follow the settings as they load and change.
  const settingsLoaded = () =>
    !isLoading() && settings() !== null && updateChecksLocked() !== null;
  const updateChecksEnabled = () =>
    (settings()?.update_checks_enabled ?? false) &&
    updateChecksLocked() === false;

  let upToDateTimeoutRef: ReturnType<typeof setTimeout> | undefined = undefined;
  let isManualCheckRef = false;
  let downloadedBytesRef = 0;
  let contentLengthRef = 0;

  // The compute tracks the two gates; the apply owns the check and the
  // manual-check listener, and returns their teardown.
  createEffect(
    () => [settingsLoaded(), updateChecksEnabled()] as const,
    ([loaded, enabled]) => {
      if (!loaded) return;

      if (!enabled) {
        if (upToDateTimeoutRef) {
          clearTimeout(upToDateTimeoutRef);
        }
        setIsChecking(false);
        setUpdateAvailable(false);
        setShowUpToDate(false);
        return;
      }

      checkForUpdates();

      const updateUnlisten = listen("check-for-updates", () => {
        handleManualUpdateCheck();
      });

      return () => {
        if (upToDateTimeoutRef) {
          clearTimeout(upToDateTimeoutRef);
        }
        updateUnlisten.then((fn) => fn());
      };
    },
  );

  const checkForUpdates = async () => {
    if (!updateChecksEnabled() || isChecking()) return;

    try {
      setIsChecking(true);
      const update = await check();

      if (update) {
        setUpdateAvailable(true);
        setShowUpToDate(false);
        setPortableInstallerUrl(
          resolvePortableInstallerUrl(update.rawJson, platform(), arch()),
        );
      } else {
        setUpdateAvailable(false);

        if (isManualCheckRef) {
          setShowUpToDate(true);
          if (upToDateTimeoutRef) {
            clearTimeout(upToDateTimeoutRef);
          }
          upToDateTimeoutRef = setTimeout(() => {
            setShowUpToDate(false);
          }, 3000);
        }
      }
    } catch (error) {
      console.error("Failed to check for updates:", error);
    } finally {
      setIsChecking(false);
      isManualCheckRef = false;
    }
  };

  const handleManualUpdateCheck = () => {
    if (!updateChecksEnabled()) return;
    isManualCheckRef = true;
    checkForUpdates();
  };

  const installUpdate = async () => {
    if (!updateChecksEnabled()) return;

    const portable = await commands.isPortable();
    if (portable) {
      setShowPortableUpdateDialog(true);
      return;
    }

    try {
      setIsInstalling(true);
      setDownloadProgress(0);
      downloadedBytesRef = 0;
      contentLengthRef = 0;
      const update = await check();

      if (!update) {
        console.log("No update available during install attempt");
        return;
      }

      await update.downloadAndInstall((event) => {
        switch (event.event) {
          case "Started":
            downloadedBytesRef = 0;
            contentLengthRef = event.data.contentLength ?? 0;
            break;
          case "Progress":
            downloadedBytesRef += event.data.chunkLength;
            const progress =
              contentLengthRef > 0
                ? Math.round((downloadedBytesRef / contentLengthRef) * 100)
                : 0;
            setDownloadProgress(Math.min(progress, 100));
            break;
        }
      });
      await relaunch();
    } catch (error) {
      console.error("Failed to install update:", error);
    } finally {
      setIsInstalling(false);
      setDownloadProgress(0);
      downloadedBytesRef = 0;
      contentLengthRef = 0;
    }
  };

  const getUpdateStatusText = () => {
    if (!updateChecksEnabled()) {
      return t("footer.updateCheckingDisabled");
    }
    if (isInstalling()) {
      return downloadProgress() > 0 && downloadProgress() < 100
        ? t("footer.downloading", {
            progress: downloadProgress().toString().padStart(3),
          })
        : downloadProgress() === 100
          ? t("footer.installing")
          : t("footer.preparing");
    }
    if (isChecking()) return t("footer.checkingUpdates");
    if (showUpToDate()) return t("footer.upToDate");
    if (updateAvailable()) return t("footer.updateAvailableShort");
    return t("footer.checkForUpdates");
  };

  const getUpdateStatusAction = () => {
    if (!updateChecksEnabled()) return undefined;
    if (updateAvailable() && !isInstalling()) return installUpdate;
    if (!isChecking() && !isInstalling() && !updateAvailable())
      return handleManualUpdateCheck;
    return undefined;
  };

  // Read in the JSX bindings, which re-run on every state change.
  const isUpdateDisabled = () =>
    !updateChecksEnabled() || isChecking() || isInstalling();
  const isUpdateClickable = () =>
    !isUpdateDisabled() &&
    (updateAvailable() || (!isChecking() && !showUpToDate()));

  const hasDirectInstaller = () => portableInstallerUrl() !== RELEASES_URL;

  return (
    <>
      {showPortableUpdateDialog() && (
        <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div class="bg-background border border-mid-gray/20 rounded-lg p-6 max-w-md w-full mx-4 space-y-4">
            <h2 class="text-base font-semibold">
              {t("footer.portableUpdateTitle")}
            </h2>
            <p class="text-sm text-text/70">
              {hasDirectInstaller()
                ? t("footer.portableUpdateMessage")
                : t("footer.portableUpdateBrowseMessage")}
            </p>
            <div class="flex gap-2 justify-end">
              <button
                class="px-3 py-1.5 text-sm rounded border border-mid-gray/20 hover:bg-mid-gray/10 transition-colors"
                onClick={() => setShowPortableUpdateDialog(false)}
              >
                {t("common.close")}
              </button>
              <button
                class="px-3 py-1.5 text-sm rounded bg-accent text-white hover:bg-accent/80 transition-colors"
                onClick={() => {
                  openUrl(portableInstallerUrl());
                  setShowPortableUpdateDialog(false);
                }}
              >
                {hasDirectInstaller()
                  ? t("footer.portableUpdateButton")
                  : t("footer.portableUpdateBrowseButton")}
              </button>
            </div>
          </div>
        </div>
      )}
      <div class={`flex items-center gap-3 ${props.class ?? ""}`}>
        {isUpdateClickable() ? (
          <button
            onClick={getUpdateStatusAction()}
            disabled={isUpdateDisabled()}
            class={`transition-colors disabled:opacity-50 tabular-nums ${
              updateAvailable()
                ? "text-accent hover:text-accent/80 font-medium"
                : "text-text/60 hover:text-text/80"
            }`}
          >
            {getUpdateStatusText()}
          </button>
        ) : (
          <span class="text-text/60 tabular-nums">{getUpdateStatusText()}</span>
        )}
        {isInstalling() &&
          downloadProgress() > 0 &&
          downloadProgress() < 100 && (
            <ProgressBar
              progress={[
                {
                  id: "update",
                  percentage: downloadProgress(),
                },
              ]}
              size="large"
            />
          )}
      </div>
    </>
  );
};

export default UpdateChecker;
