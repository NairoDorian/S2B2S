import { createSignal, createEffect } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { type } from "@tauri-apps/plugin-os";
import {
  checkAccessibilityPermission,
  requestAccessibilityPermission,
} from "tauri-plugin-macos-permissions-api";

type PermissionState = "request" | "verify" | "granted";

interface ButtonConfig {
  text: string;
  class: string;
}

const AccessibilityPermissions = () => {
  const { t } = useTranslation();
  const [hasAccessibility, setHasAccessibility] = createSignal<boolean>(false);
  const [permissionState, setPermissionState] =
    createSignal<PermissionState>("request");

  const isMacOS = type() === "macos";

  const checkPermissions = async (): Promise<boolean> => {
    const hasPermissions: boolean = await checkAccessibilityPermission();
    setHasAccessibility(hasPermissions);
    setPermissionState(hasPermissions ? "granted" : "verify");
    return hasPermissions;
  };

  const handleButtonClick = async (): Promise<void> => {
    if (permissionState() === "request") {
      try {
        await requestAccessibilityPermission();
        setPermissionState("verify");
      } catch (error) {
        console.error("Error requesting permissions:", error);
        setPermissionState("verify");
      }
    } else if (permissionState() === "verify") {
      await checkPermissions();
    }
  };

  createEffect(
    () => undefined,
    () => {
      if (!isMacOS) return;

      const initialSetup = async (): Promise<void> => {
        const hasPermissions: boolean = await checkPermissions();
        setHasAccessibility(hasPermissions);
        setPermissionState(hasPermissions ? "granted" : "request");
      };

      initialSetup();
    },
  );

  if (!isMacOS || hasAccessibility()) {
    return null;
  }

  const buttonConfig: Record<PermissionState, ButtonConfig | null> = {
    request: {
      text: t("accessibility.openSettings"),
      class:
        "px-2 py-1 text-sm font-semibold bg-mid-gray/10 border  border-mid-gray/80 hover:bg-accent/10 rounded cursor-pointer hover:border-accent",
    },
    verify: {
      text: t("accessibility.openSettings"),
      class:
        "bg-gray-100 hover:bg-gray-200 text-gray-800 font-medium py-1 px-3 rounded-md text-sm flex items-center justify-center cursor-pointer",
    },
    granted: null,
  };

  const config = buttonConfig[permissionState()] as ButtonConfig;

  return (
    <div class="p-4 w-full rounded-lg border border-mid-gray">
      <div class="flex justify-between items-center gap-2">
        <div class="">
          <p class="text-sm font-medium">
            {t("accessibility.permissionsDescription")}
          </p>
        </div>
        <button onClick={handleButtonClick} class={`min-h-10 ${config.class}`}>
          {config.text}
        </button>
      </div>
    </div>
  );
};

export default AccessibilityPermissions;
