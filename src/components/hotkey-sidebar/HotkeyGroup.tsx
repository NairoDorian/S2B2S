import { For, Show } from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import type { ShortcutBinding } from "@/bindings";
import { getShortcutAnchorId } from "@/lib/hotkeyGuide";
import { formatKeyCombination } from "@/lib/utils/keyboard";
import { useOsType } from "@/hooks/useOsType";
import type { JSX } from "@solidjs/web";

interface HotkeyGroupProps {
  title: string;
  hotkeys: ShortcutBinding[];
  onHotkeyClick: (shortcutId: string) => void;
}

export const HotkeyGroup = (props: HotkeyGroupProps): JSX.Element | null => {
  const { t } = useTranslation();
  const osType = useOsType();

  return (
    <Show when={props.hotkeys.length > 0}>
      <div class="mb-4">
        <h3 class="text-xs font-semibold text-mid-gray uppercase tracking-wider mb-2 px-1">
          {props.title}
        </h3>
        <div class="flex flex-col gap-1.5">
          <For each={props.hotkeys}>
            {(hotkey) => (
              <a
                href={`#${getShortcutAnchorId(hotkey.id)}`}
                onClick={(event) => {
                  event.preventDefault();
                  props.onHotkeyClick(hotkey.id);
                }}
                title={t("hotkeySidebar.jumpTo", {
                  name: t(
                    `settings.general.shortcut.bindings.${hotkey.id}.name`,
                    hotkey.name,
                  ),
                })}
                class="flex items-center justify-between gap-2 px-3 py-2 rounded-lg bg-mid-gray/10 hover:bg-accent/15 transition-colors focus:outline-none focus:ring-2 focus:ring-accent/50"
              >
                <span class="min-w-0 flex-1 text-sm text-text truncate">
                  {t(
                    `settings.general.shortcut.bindings.${hotkey.id}.name`,
                    hotkey.name,
                  )}
                </span>
                <kbd class="text-xs font-mono text-text bg-accent/15 border border-accent/30 px-2 py-1 rounded whitespace-nowrap">
                  {formatKeyCombination(hotkey.current_binding, osType)}
                </kbd>
              </a>
            )}
          </For>
        </div>
      </div>
    </Show>
  );
};
