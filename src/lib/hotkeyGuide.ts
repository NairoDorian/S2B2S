import type { AppSettings, ShortcutBinding } from "@/bindings";
import type { SidebarSection } from "@/components/Sidebar";

/**
 * Which hotkeys the cheat-sheet sidebar lists, grouped, and where each one is
 * configured. Kept as one table so the sidebar and the anchor links cannot
 * drift apart. Adapted from AIVORelay's hotkey guide manifest for this fork's
 * four bindings.
 */
interface HotkeyGuideEntry {
  id: string;
  section: SidebarSection;
  /** Setting that must be on for the binding to be live; `null` = always. */
  gate: keyof AppSettings | null;
}

interface HotkeyGuideCategory {
  id: string;
  titleKey: string;
  entries: readonly HotkeyGuideEntry[];
}

export const HOTKEY_GUIDE: readonly HotkeyGuideCategory[] = [
  {
    id: "recording",
    titleKey: "hotkeySidebar.categories.recording",
    entries: [
      { id: "transcribe", section: "general", gate: null },
      { id: "cancel", section: "general", gate: null },
    ],
  },
  {
    id: "modes",
    titleKey: "hotkeySidebar.categories.modes",
    entries: [
      {
        id: "transcribe_with_post_process",
        section: "postprocessing",
        gate: "post_process_enabled",
      },
      {
        id: "multi_stt_transcribe",
        section: "multiStt",
        gate: "multi_stt_enabled",
      },
    ],
  },
] as const;

export interface HotkeyGuideCategoryItems {
  id: string;
  titleKey: string;
  hotkeys: ShortcutBinding[];
}

const SECTION_BY_ID: Record<string, SidebarSection> = Object.fromEntries(
  HOTKEY_GUIDE.flatMap((c) => c.entries.map((e) => [e.id, e.section])),
);

/** DOM id of the control that edits `shortcutId` (see `ShortcutInput`). */
export const getShortcutAnchorId = (shortcutId: string): string =>
  `shortcut-${shortcutId.replace(/[^a-zA-Z0-9_-]/g, "-")}`;

/** Settings page that hosts the control for `shortcutId`. */
export const getShortcutSettingsSection = (
  shortcutId: string,
): SidebarSection => SECTION_BY_ID[shortcutId] ?? "general";

/**
 * The bindings worth showing: assigned (non-empty) and not switched off by
 * their feature gate. Categories with nothing to show are dropped.
 */
export const buildHotkeyGuideCategories = (
  settings: AppSettings | null,
): HotkeyGuideCategoryItems[] => {
  if (!settings) return [];
  const bindings = settings.bindings ?? {};
  return HOTKEY_GUIDE.map((category) => ({
    id: category.id,
    titleKey: category.titleKey,
    hotkeys: category.entries.flatMap((entry) => {
      const binding = bindings[entry.id];
      if (!binding || !binding.current_binding?.trim()) return [];
      if (entry.gate && !settings[entry.gate]) return [];
      return [binding];
    }),
  })).filter((category) => category.hotkeys.length > 0);
};
