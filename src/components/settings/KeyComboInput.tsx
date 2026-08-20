import React, { useEffect, useState, useRef } from "react";
import { useTranslation } from "react-i18next";
import {
  getKeyName,
  formatKeyCombination,
  normalizeKey,
} from "../../lib/utils/keyboard";
import { ResetButton } from "../ui/ResetButton";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import { commands } from "@/bindings";
import { toast } from "sonner";

const MODIFIERS = [
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

export const KeyComboInput: React.FC<KeyComboInputProps> = ({
  settingKey,
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const osType = useOsType();

  const bindingValue = getSetting(settingKey) as string | undefined;
  const displayValue = bindingValue ?? "";

  const [editing, setEditing] = useState(false);
  const [keyPressed, setKeyPressed] = useState<string[]>([]);
  const [recordedKeys, setRecordedKeys] = useState<string[]>([]);
  const [originalValue, setOriginalValue] = useState<string>("");
  const inputRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!editing) return;

    let cleanup = false;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (cleanup) return;
      if (e.repeat) return;
      e.preventDefault();

      const rawKey = getKeyName(e, osType);
      const key = normalizeKey(rawKey);

      if (!keyPressed.includes(key)) {
        setKeyPressed((prev) => [...prev, key]);
        if (!recordedKeys.includes(key)) {
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

      const updatedKeyPressed = keyPressed.filter((k) => k !== key);
      if (updatedKeyPressed.length === 0 && recordedKeys.length > 0) {
        const sortedKeys = recordedKeys.slice().sort((a, b) => {
          const aIsMod = MODIFIERS.includes(a.toLowerCase());
          const bIsMod = MODIFIERS.includes(b.toLowerCase());
          if (aIsMod && !bIsMod) return -1;
          if (!aIsMod && bIsMod) return 1;
          return 0;
        });
        const newShortcut = sortedKeys.join("+");

        try {
          await updateSetting(settingKey, newShortcut);
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
      if (inputRef.current && !inputRef.current.contains(e.target as Node)) {
        if (originalValue) {
          void updateSetting(settingKey, originalValue);
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
  }, [
    keyPressed,
    recordedKeys,
    editing,
    originalValue,
    updateSetting,
    osType,
    settingKey,
    t,
  ]);

  const startEditing = async () => {
    if (editing) return;

    await commands.suspendAllBindings().catch(console.error);

    setOriginalValue(displayValue);
    setEditing(true);
    setKeyPressed([]);
    setRecordedKeys([]);
  };

  const formatCurrentKeys = (): string => {
    if (recordedKeys.length === 0)
      return t("settings.general.shortcut.pressKeys");

    return formatKeyCombination(recordedKeys.join("+"), osType);
  };

  const handleReset = () => {
    void updateSetting(settingKey, DEFAULT_SHORTCUTS[settingKey]);
  };

  return (
    <div
      ref={(ref) => {
        inputRef.current = ref;
      }}
      className={`flex items-center gap-2 ${grouped ? "" : "py-2"}`}
    >
      <div
        className={`px-3 py-1.5 text-sm font-mono font-semibold bg-mid-gray/10 border border-mid-gray/80 hover:bg-logo-primary/10 rounded-md cursor-pointer hover:border-logo-primary transition-colors min-w-[200px] ${
          editing ? "border-logo-primary bg-logo-primary/30" : ""
        } ${isUpdating(settingKey) ? "opacity-50" : ""}`}
        onClick={editing ? undefined : startEditing}
        onDoubleClick={editing ? undefined : startEditing}
      >
        {editing
          ? formatCurrentKeys()
          : displayValue
            ? formatKeyCombination(displayValue, osType)
            : ""}
      </div>
      {!editing && (
        <ResetButton onClick={handleReset} disabled={isUpdating(settingKey)} />
      )}
    </div>
  );
};
