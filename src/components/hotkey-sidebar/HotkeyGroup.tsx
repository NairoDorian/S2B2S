import React from "react";
import { useTranslation } from "react-i18next";
import type { ShortcutBinding } from "@/bindings";
import { getShortcutAnchorId } from "@/lib/hotkeyGuide";
import { formatKeyCombination } from "@/lib/utils/keyboard";
import { useOsType } from "@/hooks/useOsType";

interface HotkeyGroupProps {
  title: string;
  hotkeys: ShortcutBinding[];
  onHotkeyClick: (shortcutId: string) => void;
}

/** One category of the cheat sheet: a label and a `<kbd>` chip per binding. */
export const HotkeyGroup: React.FC<HotkeyGroupProps> = ({
  title,
  hotkeys,
  onHotkeyClick,
}) => {
  const { t } = useTranslation();
  const osType = useOsType();

  if (hotkeys.length === 0) return null;

  return (
    <div className="mb-4">
      <h3 className="text-xs font-semibold text-mid-gray uppercase tracking-wider mb-2 px-1">
        {title}
      </h3>
      <div className="flex flex-col gap-1.5">
        {hotkeys.map((hotkey) => {
          const displayName = t(
            `settings.general.shortcut.bindings.${hotkey.id}.name`,
            hotkey.name,
          );
          return (
            <a
              key={hotkey.id}
              href={`#${getShortcutAnchorId(hotkey.id)}`}
              onClick={(event) => {
                event.preventDefault();
                onHotkeyClick(hotkey.id);
              }}
              title={t("hotkeySidebar.jumpTo", { name: displayName })}
              className="flex items-center justify-between gap-2 px-3 py-2 rounded-lg bg-mid-gray/10 hover:bg-accent/15 transition-colors focus:outline-none focus:ring-2 focus:ring-accent/50"
            >
              <span className="min-w-0 flex-1 text-sm text-text truncate">
                {displayName}
              </span>
              <kbd className="text-xs font-mono text-text bg-accent/15 border border-accent/30 px-2 py-1 rounded whitespace-nowrap">
                {formatKeyCombination(hotkey.current_binding, osType)}
              </kbd>
            </a>
          );
        })}
      </div>
    </div>
  );
};
