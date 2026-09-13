import { createSignal, createEffect } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { listen } from "@tauri-apps/api/event";
import { formatKeyCombination } from "../../lib/utils/keyboard";
import { ResetButton } from "../ui/ResetButton";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import { commands } from "@/bindings";
import { sessionToast as toast } from "@/lib/sessionToast";
import { openUrl } from "@tauri-apps/plugin-opener";
import { SECURE_INPUT_HELP_URL } from "../SecureInputWarning";
import type { JSX } from "@solidjs/web";

interface NativeKeysShortcutInputProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  shortcutId: string;
  disabled?: boolean;
}

interface NativeKeysEvent {
  modifiers: string[];
  key: string | null;
  is_key_down: boolean;
  hotkey_string: string;
}

export const NativeKeysShortcutInput = ({
  descriptionMode = "tooltip",
  grouped = false,
  shortcutId,
  disabled = false,
}: NativeKeysShortcutInputProps): JSX.Element => {
  const { t } = useTranslation();
  const { getSetting, updateBinding, resetBinding, isUpdating, isLoading } =
    useSettings();
  const [isRecording, setIsRecording] = createSignal(false);
  const [currentKeys, setCurrentKeys] = createSignal<string>("");
  const [originalBinding, setOriginalBinding] = createSignal<string>("");
  let shortcutRef: HTMLDivElement | null = null;
  let unlistenRef: (() => void) | null = null;
  let keyedShortcutRef = "";
  let modifierOnlyShortcutRef = "";
  const osType = useOsType();

  const bindings = getSetting("bindings") || {};

  const cancelRecording = async () => {
    if (!isRecording()) return;

    if (unlistenRef) {
      unlistenRef();
      unlistenRef = null;
    }

    await commands.stopNativeKeysRecording().catch(console.error);

    if (originalBinding()) {
      try {
        await updateBinding(shortcutId, originalBinding());
      } catch (error) {
        console.error("Failed to restore original binding:", error);
        toast.error(t("settings.general.shortcut.errors.restore"));
      }
    }

    setIsRecording(false);
    setCurrentKeys("");
    keyedShortcutRef = "";
    modifierOnlyShortcutRef = "";
    setOriginalBinding("");
  };

  createEffect(
    () => undefined,
    () => {
      if (!isRecording()) return;

      let cleanup = false;

      const setupListener = async () => {
        const commitAndStop = async (keysToCommit: string) => {
          try {
            await updateBinding(shortcutId, keysToCommit);
          } catch (error) {
            console.error("Failed to change binding:", error);
            toast.error(
              t("settings.general.shortcut.errors.set", {
                error: String(error),
              }),
            );

            if (originalBinding()) {
              try {
                await updateBinding(shortcutId, originalBinding());
              } catch (resetError) {
                console.error("Failed to reset binding:", resetError);
                toast.error(t("settings.general.shortcut.errors.reset"));
              }
            }
          }

          if (unlistenRef) {
            unlistenRef();
            unlistenRef = null;
          }
          await commands.stopNativeKeysRecording().catch(console.error);
          setIsRecording(false);
          setCurrentKeys("");
          keyedShortcutRef = "";
          modifierOnlyShortcutRef = "";
          setOriginalBinding("");
        };

        const unlisten = await listen<NativeKeysEvent>(
          "native-keys-event",
          async (event) => {
            if (cleanup) return;

            const { hotkey_string, is_key_down, key } = event.payload;

            if (is_key_down && hotkey_string) {
              if (key) {
                keyedShortcutRef = hotkey_string;
              } else {
                modifierOnlyShortcutRef = hotkey_string;
              }
              setCurrentKeys(hotkey_string);
            } else if (!is_key_down && key) {
              const keysToCommit = keyedShortcutRef || hotkey_string;
              if (keysToCommit) {
                await commitAndStop(keysToCommit);
              }
            } else if (
              !is_key_down &&
              keyedShortcutRef &&
              modifierOnlyShortcutRef
            ) {
              await commitAndStop(modifierOnlyShortcutRef);
            }
          },
        );

        unlistenRef = unlisten;
      };

      setupListener();

      return () => {
        cleanup = true;
        if (unlistenRef) {
          unlistenRef();
          unlistenRef = null;
        }
        commands.stopNativeKeysRecording().catch(console.error);
      };
    },
  );

  createEffect(
    () => undefined,
    () => {
      if (!isRecording()) return;

      const handleClickOutside = (e: MouseEvent) => {
        if (shortcutRef && !shortcutRef.contains(e.target as Node)) {
          cancelRecording();
        }
      };

      window.addEventListener("click", handleClickOutside);
      return () => window.removeEventListener("click", handleClickOutside);
    },
  );

  const startRecording = async () => {
    if (isRecording()) return;

    setOriginalBinding(bindings[shortcutId]?.current_binding || "");

    try {
      const result = await commands.startNativeKeysRecording(shortcutId);
      if (result.status === "error") {
        if (String(result.error).includes("secure-input-active")) {
          toast.error(t("secureInput.recorderBlocked"), {
            action: {
              label: t("secureInput.learnMore"),
              onClick: () => openUrl(SECURE_INPUT_HELP_URL),
            },
          });
        } else {
          toast.error(
            t("settings.general.shortcut.errors.set", {
              error: String(result.error),
            }),
          );
        }
        return;
      }
      setIsRecording(true);
      setCurrentKeys("");
      keyedShortcutRef = "";
      modifierOnlyShortcutRef = "";
    } catch (error) {
      console.error("Failed to start recording:", error);
      toast.error(
        t("settings.general.shortcut.errors.set", { error: String(error) }),
      );
    }
  };

  const formatCurrentKeys = (): string => {
    if (!currentKeys()) return t("settings.general.shortcut.pressKeys");
    return formatKeyCombination(currentKeys(), osType);
  };

  if (isLoading()) {
    return (
      <SettingContainer
        title={t("settings.general.shortcut.title")}
        description={t("settings.general.shortcut.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <div class="text-sm text-mid-gray">
          {t("settings.general.shortcut.loading")}
        </div>
      </SettingContainer>
    );
  }

  if (Object.keys(bindings).length === 0) {
    return (
      <SettingContainer
        title={t("settings.general.shortcut.title")}
        description={t("settings.general.shortcut.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <div class="text-sm text-mid-gray">
          {t("settings.general.shortcut.none")}
        </div>
      </SettingContainer>
    );
  }

  const binding = bindings[shortcutId];
  if (!binding) {
    return (
      <SettingContainer
        title={t("settings.general.shortcut.title")}
        description={t("settings.general.shortcut.notFound")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <div class="text-sm text-mid-gray">
          {t("settings.general.shortcut.none")}
        </div>
      </SettingContainer>
    );
  }

  const translatedName = t(
    `settings.general.shortcut.bindings.${shortcutId}.name`,
    binding.name,
  );
  const translatedDescription = t(
    `settings.general.shortcut.bindings.${shortcutId}.description`,
    binding.description,
  );

  return (
    <SettingContainer
      title={translatedName}
      description={translatedDescription}
      descriptionMode={descriptionMode}
      grouped={grouped}
      disabled={disabled}
      layout="horizontal"
    >
      <div class="flex items-center space-x-1">
        {isRecording() ? (
          <div
            ref={(el) => (shortcutRef = el)}
            class="px-2 py-1 text-sm font-semibold border border-accent bg-accent/30 rounded-md"
          >
            {formatCurrentKeys()}
          </div>
        ) : (
          <div
            class="px-2 py-1 text-sm font-semibold bg-mid-gray/10 border border-mid-gray/80 hover:bg-accent/10 rounded-md cursor-pointer hover:border-accent"
            onClick={startRecording}
          >
            {formatKeyCombination(binding.current_binding, osType)}
          </div>
        )}
        <ResetButton
          onClick={() => resetBinding(shortcutId)}
          disabled={isUpdating(`binding_${shortcutId}`)}
        />
      </div>
    </SettingContainer>
  );
};
