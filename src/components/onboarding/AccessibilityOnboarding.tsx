import {
  createSignal,
  createEffect,
  Match,
  onSettled,
  Switch,
  untrack,
} from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { platform } from "@tauri-apps/plugin-os";
import {
  checkAccessibilityPermission,
  requestAccessibilityPermission,
  checkMicrophonePermission,
  requestMicrophonePermission,
} from "tauri-plugin-macos-permissions-api";
import { sessionToast as toast } from "@/lib/sessionToast";
import { commands } from "@/bindings";
import { useSettingsStore } from "@/stores/settingsStore";
import BrandLockup from "../icons/BrandLockup";
import { Keyboard, Mic, Check, Loader2 } from "@/components/icons/lucide";

interface AccessibilityOnboardingProps {
  onComplete: () => void;
  preview?: boolean;
}

type PermissionStatus = "checking" | "needed" | "waiting" | "granted";
type PermissionPlatform = "macos" | "windows" | "other";

interface PermissionsState {
  accessibility: PermissionStatus;
  microphone: PermissionStatus;
}

const hasWindowsMicrophoneAccess = async (): Promise<boolean> => {
  const microphoneStatus =
    await commands.getWindowsMicrophonePermissionStatus();

  if (!microphoneStatus.supported) {
    return true;
  }

  return microphoneStatus.overall_access !== "denied";
};

/**
 * Brings up keyboard simulation and the global shortcuts once accessibility is
 * granted. Both commands report failure as a result rather than a rejection,
 * so each result is inspected; a failure is logged and never blocks onboarding.
 */
const initializeAfterAccessibilityGrant = async (): Promise<void> => {
  try {
    const [enigo, shortcuts] = await Promise.all([
      commands.initializeEnigo(),
      commands.initializeShortcuts(),
    ]);
    if (enigo.status === "error")
      console.warn("Failed to initialize keyboard simulation:", enigo.error);
    if (shortcuts.status === "error")
      console.warn("Failed to initialize shortcuts:", shortcuts.error);
  } catch (e) {
    console.warn("Failed to initialize after permission grant:", e);
  }
};

const AccessibilityOnboarding = (props: AccessibilityOnboardingProps) => {
  const { t } = useTranslation();
  const settingsStore = useSettingsStore();
  // Stable action references, read once (see hooks/useSettings.ts).
  const { refreshAudioDevices, refreshOutputDevices } = untrack(() => ({
    refreshAudioDevices: settingsStore.refreshAudioDevices,
    refreshOutputDevices: settingsStore.refreshOutputDevices,
  }));
  const [permissionPlatform, setPermissionPlatform] =
    createSignal<PermissionPlatform | null>(null);
  const [permissions, setPermissions] = createSignal<PermissionsState>({
    accessibility: "checking",
    microphone: "checking",
  });
  let pollingRef: ReturnType<typeof setInterval> | null = null;
  let timeoutRef: ReturnType<typeof setTimeout> | null = null;
  let errorCountRef = 0;
  const MAX_POLLING_ERRORS = 3;

  // Accessors, not consts: the platform is only known after the mount effect
  // below has run, and the permission states change while the page is shown.
  const isMacOS = () => permissionPlatform() === "macos";
  const isWindows = () => permissionPlatform() === "windows";
  const showMicrophonePermission = () => isMacOS() || isWindows();
  const showAccessibilityPermission = isMacOS;

  const allGranted = () =>
    isMacOS()
      ? permissions().accessibility === "granted" &&
        permissions().microphone === "granted"
      : isWindows()
        ? permissions().microphone === "granted"
        : true;

  const completeOnboarding = async () => {
    await Promise.all([refreshAudioDevices(), refreshOutputDevices()]);
    timeoutRef = setTimeout(() => props.onComplete(), 300);
  };

  const startPolling = () => {
    if (pollingRef || permissionPlatform() === null) return;

    pollingRef = setInterval(async () => {
      try {
        if (permissionPlatform() === "windows") {
          const microphoneGranted = await hasWindowsMicrophoneAccess();

          if (microphoneGranted) {
            setPermissions((prev) => ({ ...prev, microphone: "granted" }));

            if (pollingRef) {
              clearInterval(pollingRef);
              pollingRef = null;
            }

            await completeOnboarding();
          }

          errorCountRef = 0;
          return;
        }

        const [accessibilityGranted, microphoneGranted] = await Promise.all([
          checkAccessibilityPermission(),
          checkMicrophonePermission(),
        ]);

        setPermissions((prev) => {
          const newState = { ...prev };

          if (accessibilityGranted && prev.accessibility !== "granted") {
            newState.accessibility = "granted";
            void initializeAfterAccessibilityGrant();
          }

          if (microphoneGranted && prev.microphone !== "granted") {
            newState.microphone = "granted";
          }

          return newState;
        });

        if (accessibilityGranted && microphoneGranted) {
          if (pollingRef) {
            clearInterval(pollingRef);
            pollingRef = null;
          }
          await completeOnboarding();
        }

        errorCountRef = 0;
      } catch (error) {
        console.error("Error checking permissions:", error);
        errorCountRef += 1;

        if (errorCountRef >= MAX_POLLING_ERRORS) {
          if (pollingRef) {
            clearInterval(pollingRef);
            pollingRef = null;
          }
          toast.error(t("onboarding.permissions.errors.checkFailed"));
        }
      }
    }, 1000);
  };

  createEffect(
    () => undefined,
    () => {
      const currentPlatform = platform();
      const nextPlatform: PermissionPlatform =
        currentPlatform === "macos"
          ? "macos"
          : currentPlatform === "windows"
            ? "windows"
            : "other";

      setPermissionPlatform(nextPlatform);

      if (props.preview) {
        setPermissions({
          accessibility: nextPlatform === "macos" ? "needed" : "granted",
          microphone: nextPlatform === "other" ? "granted" : "needed",
        });
        return;
      }

      if (nextPlatform === "other") {
        props.onComplete();
        return;
      }

      const checkInitial = async () => {
        if (nextPlatform === "macos") {
          try {
            const [accessibilityGranted, microphoneGranted] = await Promise.all(
              [checkAccessibilityPermission(), checkMicrophonePermission()],
            );

            if (accessibilityGranted) {
              await initializeAfterAccessibilityGrant();
            }

            const newState: PermissionsState = {
              accessibility: accessibilityGranted ? "granted" : "needed",
              microphone: microphoneGranted ? "granted" : "needed",
            };

            setPermissions(newState);

            if (accessibilityGranted && microphoneGranted) {
              await completeOnboarding();
            }
          } catch (error) {
            console.error("Failed to check macOS permissions:", error);
            toast.error(t("onboarding.permissions.errors.checkFailed"));
            setPermissions({
              accessibility: "needed",
              microphone: "needed",
            });
          }

          return;
        }

        try {
          const microphoneGranted = await hasWindowsMicrophoneAccess();

          setPermissions({
            accessibility: "granted",
            microphone: microphoneGranted ? "granted" : "needed",
          });

          if (microphoneGranted) {
            await completeOnboarding();
          }
        } catch (error) {
          console.warn(
            "Failed to check Windows microphone permissions:",
            error,
          );
          setPermissions({
            accessibility: "granted",
            microphone: "granted",
          });
          await completeOnboarding();
        }
      };

      checkInitial();
    },
  );

  onSettled(() => () => {
    if (pollingRef) {
      clearInterval(pollingRef);
    }
    if (timeoutRef) {
      clearTimeout(timeoutRef);
    }
  });

  const handleGrantAccessibility = async () => {
    if (props.preview) return;

    try {
      await requestAccessibilityPermission();
      setPermissions((prev) => ({ ...prev, accessibility: "waiting" }));
      startPolling();
    } catch (error) {
      console.error("Failed to request accessibility permission:", error);
      toast.error(t("onboarding.permissions.errors.requestFailed"));
    }
  };

  const handleGrantMicrophone = async () => {
    if (props.preview) return;

    try {
      if (isWindows()) {
        const result = await commands.openMicrophonePrivacySettings();
        if (result.status === "error") throw new Error(result.error);
      } else {
        await requestMicrophonePermission();
      }

      setPermissions((prev) => ({ ...prev, microphone: "waiting" }));
      startPolling();
    } catch (error) {
      console.error("Failed to request microphone permission:", error);
      toast.error(t("onboarding.permissions.errors.requestFailed"));
    }
  };

  const isChecking = () =>
    permissionPlatform() === null ||
    (isMacOS() &&
      permissions().accessibility === "checking" &&
      permissions().microphone === "checking") ||
    (isWindows() && permissions().microphone === "checking");

  return (
    <Switch>
      <Match when={isChecking()}>
        <div class="h-screen w-full flex items-center justify-center">
          <Loader2 class="w-8 h-8 animate-spin text-text/50" />
        </div>
      </Match>
      <Match when={allGranted()}>
        <div class="h-screen w-full flex flex-col items-center justify-center gap-4">
          <div class="p-4 rounded-full bg-emerald-500/20">
            <Check class="w-12 h-12 text-emerald-400" />
          </div>
          <p class="text-lg font-medium text-text">
            {t("onboarding.permissions.allGranted")}
          </p>
        </div>
      </Match>
      <Match when={true}>
        <div class="h-screen w-full flex flex-col p-6 gap-6 items-center justify-center">
          <div class="flex flex-col items-center gap-2">
            <BrandLockup size={44} />
          </div>

          <div class="max-w-md w-full flex flex-col items-center gap-4">
            <div class="text-center mb-2">
              <h2 class="text-xl font-semibold text-text mb-2">
                {t("onboarding.permissions.title")}
              </h2>
              <p class="text-text/70">
                {t("onboarding.permissions.description")}
              </p>
            </div>

            {showMicrophonePermission() && (
              <div class="w-full p-4 rounded-lg bg-white/5 border border-mid-gray/20">
                <div class="flex items-center gap-4">
                  <div class="p-3 rounded-full bg-accent/20 shrink-0">
                    <Mic class="w-6 h-6 text-accent" />
                  </div>
                  <div class="flex-1 min-w-0">
                    <h3 class="font-medium text-text">
                      {t("onboarding.permissions.microphone.title")}
                    </h3>
                    <p class="text-sm text-text/60 mb-3">
                      {t("onboarding.permissions.microphone.description")}
                    </p>
                    {permissions().microphone === "granted" ? (
                      <div class="flex items-center gap-2 text-emerald-400 text-sm">
                        <Check class="w-4 h-4" />
                        {t("onboarding.permissions.granted")}
                      </div>
                    ) : permissions().microphone === "waiting" ? (
                      <div class="flex items-center gap-2 text-text/50 text-sm">
                        <Loader2 class="w-4 h-4 animate-spin" />
                        {t("onboarding.permissions.waiting")}
                      </div>
                    ) : (
                      <button
                        onClick={handleGrantMicrophone}
                        class="px-4 py-2 rounded-lg bg-accent hover:bg-accent/90 text-white text-sm font-medium transition-colors"
                      >
                        {isWindows()
                          ? t("accessibility.openSettings")
                          : t("onboarding.permissions.grant")}
                      </button>
                    )}
                  </div>
                </div>
              </div>
            )}

            {showAccessibilityPermission() && (
              <div class="w-full p-4 rounded-lg bg-white/5 border border-mid-gray/20">
                <div class="flex items-center gap-4">
                  <div class="p-3 rounded-full bg-accent/20 shrink-0">
                    <Keyboard class="w-6 h-6 text-accent" />
                  </div>
                  <div class="flex-1 min-w-0">
                    <h3 class="font-medium text-text">
                      {t("onboarding.permissions.accessibility.title")}
                    </h3>
                    <p class="text-sm text-text/60 mb-3">
                      {t("onboarding.permissions.accessibility.description")}
                    </p>
                    {permissions().accessibility === "granted" ? (
                      <div class="flex items-center gap-2 text-emerald-400 text-sm">
                        <Check class="w-4 h-4" />
                        {t("onboarding.permissions.granted")}
                      </div>
                    ) : permissions().accessibility === "waiting" ? (
                      <div class="flex items-center gap-2 text-text/50 text-sm">
                        <Loader2 class="w-4 h-4 animate-spin" />
                        {t("onboarding.permissions.waiting")}
                      </div>
                    ) : (
                      <button
                        onClick={handleGrantAccessibility}
                        class="px-4 py-2 rounded-lg bg-accent hover:bg-accent/90 text-white text-sm font-medium transition-colors"
                      >
                        {t("onboarding.permissions.grant")}
                      </button>
                    )}
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>
      </Match>
    </Switch>
  );
};

export default AccessibilityOnboarding;
