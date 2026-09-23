import { createSignal, createEffect } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import {
  getKeyName,
  formatKeyCombination,
  isSimulatableKey,
  MODIFIERS,
  normalizeKey,
} from "../../lib/utils/keyboard";
import { ResetButton } from "../ui/ResetButton";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import { commands } from "@/bindings";
import { sessionToast as toast } from "@/lib/sessionToast";
import type { JSX } from "@solidjs/web";

interface KeyComboInputProps {
  settingKey: StringSettingKey;
  grouped?: boolean;
}

type StringSettingKey =
  | "multi_stt_performance_mode_full_power_shortcut"
  | "multi_stt_performance_mode_normal_shortcut";

const DEFAULT_SHORTCUTS: Record<StringSettingKey, string> = {
  multi_stt_performance_mode_full_power_shortcut: "ctrl+space",
  multi_stt_performance_mode_normal_shortcut: "ctrl+alt+space",
};

export const KeyComboInput = (props: KeyComboInputProps): JSX.Element => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const osType = useOsType();

  // An accessor: the body runs once, and the stored combination changes both
  // here (a recorded shortcut) and from the reset button.
  const displayValue = () =>
    (getSetting(props.settingKey) as string | undefined) ?? "";

  const [editing, setEditing] = createSignal(false);
  const [keyPressed, setKeyPressed] = createSignal<string[]>([]);
  const [recordedKeys, setRecordedKeys] = createSignal<string[]>([]);
  const [originalValue, setOriginalValue] = createSignal<string>("");
  let inputRef: HTMLDivElement | null = null;

  createEffect(
    () => editing(),
    (isEditing) => {
      if (!isEditing) return;

      let cleanup = false;

      const handleKeyDown = (e: KeyboardEvent) => {
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
          const sortedKeys = recordedKeys().toSorted((a, b) => {
            const aIsMod = MODIFIERS.has(a.toLowerCase());
            const bIsMod = MODIFIERS.has(b.toLowerCase());
            if (aIsMod && !bIsMod) return -1;
            if (!aIsMod && bIsMod) return 1;
            return 0;
          });
          const newShortcut = sortedKeys.join("+");

          const unsupported = sortedKeys.filter(
            (k) => !isSimulatableKey(k.toLowerCase()),
          );
          const hasNonModifier = sortedKeys.some(
            (k) => !MODIFIERS.has(k.toLowerCase()),
          );
          if (unsupported.length > 0 || !hasNonModifier) {
            toast.error(
              t("settings.general.shortcut.errors.unsupportedCombo", {
                combo: formatKeyCombination(newShortcut, osType),
              }),
            );
            if (originalValue()) {
              void updateSetting(props.settingKey, originalValue());
            }
            await commands.resumeAllBindings().catch(console.error);
            setEditing(false);
            setKeyPressed([]);
            setRecordedKeys([]);
            setOriginalValue("");
            return;
          }

          try {
            await updateSetting(props.settingKey, newShortcut);
          } catch (error) {
            console.error("Failed to change setting:", error);
            toast.error(
              t("settings.general.shortcut.errors.set", {
                error: String(error),
              }),
            );
          }

          await commands.resumeAllBindings().catch(console.error);

          setEditing(false);
          setKeyPressed([]);
          setRecordedKeys([]);
          setOriginalValue("");
        }
      };

      const handleClickOutside = (e: MouseEvent) => {
        if (cleanup) return;
        if (inputRef && !inputRef.contains(e.target as Node)) {
          if (originalValue()) {
            void updateSetting(props.settingKey, originalValue());
          }
          void commands.resumeAllBindings().catch(console.error);
          setEditing(false);
          setKeyPressed([]);
          setRecordedKeys([]);
          setOriginalValue("");
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

  const startEditing = async () => {
    if (editing()) return;

    await commands.suspendAllBindings().catch(console.error);

    setOriginalValue(displayValue());
    setEditing(true);
    setKeyPressed([]);
    setRecordedKeys([]);
  };

  const formatCurrentKeys = (): string => {
    if (recordedKeys().length === 0)
      return t("settings.general.shortcut.pressKeys");

    return formatKeyCombination(recordedKeys().join("+"), osType);
  };

  const handleReset = () => {
    void updateSetting(props.settingKey, DEFAULT_SHORTCUTS[props.settingKey]);
  };

  return (
    <div
      ref={(ref) => {
        inputRef = ref;
      }}
      class={`flex items-center gap-2 ${props.grouped ? "" : "py-2"}`}
    >
      <button
        type="button"
        class={`px-3 py-1.5 text-sm font-mono font-semibold bg-mid-gray/10 border border-mid-gray/80 hover:bg-accent/10 rounded-md cursor-pointer hover:border-accent transition-colors min-w-[200px] text-start ${
          editing() ? "border-accent bg-accent/30" : ""
        } ${isUpdating(props.settingKey) ? "opacity-50" : ""}`}
        onClick={editing() ? undefined : startEditing}
        onDblClick={editing() ? undefined : startEditing}
      >
        {editing()
          ? formatCurrentKeys()
          : displayValue()
            ? formatKeyCombination(displayValue(), osType)
            : ""}
      </button>
      {!editing() && (
        <ResetButton
          onClick={handleReset}
          disabled={isUpdating(props.settingKey)}
        />
      )}
    </div>
  );
};
