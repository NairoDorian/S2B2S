import { createSignal, onSettled, Show } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { type } from "@tauri-apps/plugin-os";
import {
  checkAccessibilityPermission,
  requestAccessibilityPermission,
} from "tauri-plugin-macos-permissions-api";

type PermissionState = "request" | "verify" | "granted";

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

  onSettled(() => {
    if (!isMacOS) return;

    // Unlike `checkPermissions`, a missing permission at mount means it was
    // never requested: offer the request, not the verify step.
    void checkAccessibilityPermission().then((hasPermissions) => {
      setHasAccessibility(hasPermissions);
      setPermissionState(hasPermissions ? "granted" : "request");
    });
  });

  // The button's text and style must be read reactively: permissionState()
  // moves between request/verify/granted as the async checks resolve, and a
  // body-level snapshot would freeze the label at whatever state mount saw.
  const buttonConfig = () => {
    switch (permissionState()) {
      case "request":
        return {
          text: t("accessibility.openSettings"),
          class:
            "px-2 py-1 text-sm font-semibold bg-mid-gray/10 border  border-mid-gray/80 hover:bg-accent/10 rounded cursor-pointer hover:border-accent",
        };
      case "verify":
        return {
          text: t("accessibility.openSettings"),
          class:
            "bg-gray-100 hover:bg-gray-200 text-gray-800 font-medium py-1 px-3 rounded-md text-sm flex items-center justify-center cursor-pointer",
        };
      case "granted":
        return null;
    }
  };

  return (
    <Show when={isMacOS && !hasAccessibility()}>
      <div class="p-4 w-full rounded-lg border border-mid-gray">
        <div class="flex justify-between items-center gap-2">
          <div class="">
            <p class="text-sm font-medium">
              {t("accessibility.permissionsDescription")}
            </p>
          </div>
          <button
            onClick={handleButtonClick}
            class={`min-h-10 ${buttonConfig()?.class ?? ""}`}
          >
            {buttonConfig()?.text}
          </button>
        </div>
      </div>
    </Show>
  );
};

export default AccessibilityPermissions;
