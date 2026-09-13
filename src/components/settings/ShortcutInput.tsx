import { useSettings } from "../../hooks/useSettings";
import { GlobalShortcutInput } from "./GlobalShortcutInput";
import { NativeKeysShortcutInput } from "./NativeKeysShortcutInput";
import { getShortcutAnchorId } from "@/lib/hotkeyGuide";
import type { JSX } from "@solidjs/web";

interface ShortcutInputProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  shortcutId: string;
  disabled?: boolean;
}

/**
 * Wrapper component that selects the appropriate shortcut input implementation
 * based on the keyboard_implementation setting.
 *
 * - "tauri" (default): Uses GlobalShortcutInput with JS keyboard events
 * - "handy_keys": Uses NativeKeysShortcutInput with backend key events
 *
 * The wrapper carries a stable `shortcut-<id>` element id so the hotkey
 * sidebar and Help links can scroll to and highlight this control.
 */
export const ShortcutInput = (props: ShortcutInputProps): JSX.Element => {
  const { getSetting } = useSettings();
  const keyboardImplementation = getSetting("keyboard_implementation");

  // Default to Tauri implementation if not set
  const input =
    keyboardImplementation === "handy_keys" ? (
      <NativeKeysShortcutInput {...props} />
    ) : (
      <GlobalShortcutInput {...props} />
    );

  return (
    <div
      id={getShortcutAnchorId(props.shortcutId)}
      tabindex={-1}
      class="settings-anchor rounded-lg focus:outline-none"
    >
      {input}
    </div>
  );
};
