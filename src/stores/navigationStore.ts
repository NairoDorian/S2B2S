import { create } from "zustand";
import type { SidebarSection } from "@/components/Sidebar";

/**
 * Which settings page is shown, plus the hand-off slot the Help page reads
 * when something (a QuickHelp banner, the hotkey sidebar's empty state)
 * opens Help at a particular section. Lives outside `App` so any component
 * can navigate without prop drilling.
 */
interface NavigationStore {
  section: SidebarSection;
  /** Set by `openHelp`; the Help page scrolls to it once mounted, then clears it. */
  pendingHelpAnchor: string | null;
  setSection: (section: SidebarSection) => void;
  openHelp: (anchor?: string) => void;
  consumePendingHelpAnchor: () => void;
}

export const useNavigationStore = create<NavigationStore>((set) => ({
  section: "general",
  pendingHelpAnchor: null,
  setSection: (section) => set({ section }),
  openHelp: (anchor) =>
    set({ section: "help", pendingHelpAnchor: anchor ?? null }),
  consumePendingHelpAnchor: () => set({ pendingHelpAnchor: null }),
}));
