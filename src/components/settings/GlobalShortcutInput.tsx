import { createSignal, createEffect } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import {
  getKeyName,
  formatKeyCombination,
  normalizeKey,
} from "../../lib/utils/keyboard";
import { ResetButton } from "../ui/ResetButton";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import { commands } from "@/bindings";
import { sessionToast as toast } from "@/lib/sessionToast";

interface GlobalShortcutInputProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  shortcutId: string;
  disabled?: boolean;
}

export const GlobalShortcutInput = ({
  descriptionMode = "tooltip",
  grouped = false,
  shortcutId,
  disabled = false,
}: GlobalShortcutInputProps) => {
  const { t } = useTranslation();
  const { getSetting, updateBinding, resetBinding, isUpdating, isLoading } =
    useSettings();
  const [keyPressed, setKeyPressed] = createSignal<string[]>([]);
  const [recordedKeys, setRecordedKeys] = createSignal<string[]>([]);
  const [editingShortcutId, setEditingShortcutId] = createSignal<string | null>(
    null,
  );
  const [originalBinding, setOriginalBinding] = createSignal<string>("");
  const shortcutRefs = new Map<string, HTMLDivElement | null>();
  const osType = useOsType();

  const bindings = getSetting("bindings") || {};

  const startRecording = async (id: string) => {
    if (editingShortcutId() === id) return;

    await commands.suspendAllBindings().catch(console.error);

    setOriginalBinding(bindings[id]?.current_binding || "");
    setEditingShortcutId(id);
    setKeyPressed([]);
    setRecordedKeys([]);
  };

  const formatCurrentKeys = (): string => {
    if (recordedKeys().length === 0)
      return t("settings.general.shortcut.pressKeys");

    return formatKeyCombination(recordedKeys().join("+"), osType);
  };

  const setShortcutRef = (id: string, ref: HTMLDivElement | null) => {
    shortcutRefs.set(id, ref);
  };

  createEffect(
    () => undefined,
    () => {
      const id = editingShortcutId();
      if (id === null) return;

      let cleanup = false;

      const handleKeyDown = async (e: KeyboardEvent) => {
        if (cleanup) return;
        if (e.repeat) return;
        e.preventDefault();

        const rawKey = getKeyName(e, osType);
        const key = normalizeKey(rawKey);

        if (!keyPressed().includes(key)) {
          setKeyPressed((prev) => [...prev, key]);
          if (!recordedKeys().includes(key)) {
            setRecordedKeys((prev) => [...prev, key]);
          }
        }
      };

      const handleKeyUp = async (e: KeyboardEvent) => {
        if (cleanup) return;
        e.preventDefault();

        const rawKey = getKeyName(e, osType);
        const key = normalizeKey(rawKey);

        setKeyPressed((prev) => prev.filter((k) => k !== key));

        const updatedKeyPressed = keyPressed().filter((k) => k !== key);
        if (updatedKeyPressed.length === 0 && recordedKeys().length > 0) {
          const modifiers = [
            "ctrl",
            "control",
            "shift",
            "alt",
            "option",
            "meta",
            "command",
            "cmd",
            "super",
            "win",
            "windows",
          ];
          const sortedKeys = recordedKeys().sort((a, b) => {
            const aIsModifier = modifiers.includes(a.toLowerCase());
            const bIsModifier = modifiers.includes(b.toLowerCase());
            if (aIsModifier && !bIsModifier) return -1;
            if (!aIsModifier && bIsModifier) return 1;
            return 0;
          });
          const newShortcut = sortedKeys.join("+");

          if (bindings[id]) {
            try {
              await updateBinding(id, newShortcut);
            } catch (error) {
              console.error("Failed to change binding:", error);
              toast.error(
                t("settings.general.shortcut.errors.set", {
                  error: String(error),
                }),
              );
              if (originalBinding()) {
                try {
                  await updateBinding(id, originalBinding());
                } catch (resetError) {
                  console.error("Failed to reset binding:", resetError);
                  toast.error(t("settings.general.shortcut.errors.reset"));
                }
              }
            }

            await commands.resumeAllBindings().catch(console.error);

            setEditingShortcutId(null);
            setKeyPressed([]);
            setRecordedKeys([]);
            setOriginalBinding("");
          }
        }
      };

      const handleClickOutside = async (e: MouseEvent) => {
        if (cleanup) return;
        const activeElement = shortcutRefs.get(id);
        if (activeElement && !activeElement.contains(e.target as Node)) {
          if (originalBinding()) {
            try {
              await updateBinding(id, originalBinding());
            } catch (error) {
              console.error("Failed to restore original binding:", error);
              toast.error(t("settings.general.shortcut.errors.restore"));
            }
          }
          await commands.resumeAllBindings().catch(console.error);
          setEditingShortcutId(null);
          setKeyPressed([]);
          setRecordedKeys([]);
          setOriginalBinding("");
        }
      };

      window.addEventListener("keydown", handleKeyDown);
      window.addEventListener("keyup", handleKeyUp);
      window.addEventListener("click", handleClickOutside);

      return () => {
        cleanup = true;
        window.removeEventListener("keydown", handleKeyDown);
        window.removeEventListener("keyup", handleKeyUp);
        window.removeEventListener("click", handleClickOutside);
      };
    },
  );

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
        {editingShortcutId() === shortcutId ? (
          <div
            ref={(ref) => setShortcutRef(shortcutId, ref)}
            class="px-2 py-1 text-sm font-semibold border border-accent bg-accent/30 rounded-md"
          >
            {formatCurrentKeys()}
          </div>
        ) : (
          <div
            class="px-2 py-1 text-sm font-semibold bg-mid-gray/10 border border-mid-gray/80 hover:bg-accent/10 rounded-md cursor-pointer hover:border-accent"
            onClick={() => startRecording(shortcutId)}
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
