import { Show } from "solid-js";
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
 * - "handy_keys" (the default on Windows and macOS): NativeKeysShortcutInput
 *   with backend key events
 * - "tauri" (the default on Linux): GlobalShortcutInput with JS keyboard events
 *
 * The setting is read reactively, so switching the backend on the Advanced
 * page swaps the recorder without a remount.
 *
 * The wrapper carries a stable `shortcut-<id>` element id so the hotkey
 * sidebar and Help links can scroll to and highlight this control.
 */
export const ShortcutInput = (props: ShortcutInputProps): JSX.Element => {
  const { getSetting } = useSettings();

  return (
    <div
      id={getShortcutAnchorId(props.shortcutId)}
      tabindex={-1}
      class="settings-anchor rounded-lg focus:outline-none"
    >
      <Show
        when={getSetting("keyboard_implementation") === "handy_keys"}
        fallback={<GlobalShortcutInput {...props} />}
      >
        <NativeKeysShortcutInput {...props} />
      </Show>
    </div>
  );
};
