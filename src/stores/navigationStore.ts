import type { SidebarSection } from "@/components/Sidebar";
import { createSolidStore } from "@/lib/solidStore";

/**
 * Which settings page is shown, plus the hand-off slot the Help page reads
 * when a QuickHelp banner (the only caller of `openHelp`) opens Help at a
 * particular section. Lives outside `App` so any component can navigate
 * without prop drilling.
 */
interface NavigationStore {
  section: SidebarSection;
  /**
   * Set by `openHelp` from a QuickHelp banner; the Help page scrolls to it
   * once mounted, then clears it.
   */
  pendingHelpAnchor: string | null;
  liveCanvas: boolean;
  setSection: (section: SidebarSection) => void;
  openHelp: (anchor?: string) => void;
  consumePendingHelpAnchor: () => void;
  setLiveCanvas: (active: boolean) => void;
}

const navigationState = createSolidStore<NavigationStore>((set) => ({
  section: "general",
  pendingHelpAnchor: null,
  liveCanvas: false,
  setSection: (section) => set({ section }),
  openHelp: (anchor) =>
    set({ section: "help", pendingHelpAnchor: anchor ?? null }),
  consumePendingHelpAnchor: () => set({ pendingHelpAnchor: null }),
  setLiveCanvas: (liveCanvas) => set({ liveCanvas }),
}));

export function useNavigationStore() {
  return navigationState;
}

export const setSection = (section: SidebarSection) => {
  navigationState.setSection(section);
};

export const openHelp = (anchor?: string) => {
  navigationState.openHelp(anchor);
};

export const consumePendingHelpAnchor = () => {
  navigationState.consumePendingHelpAnchor();
};

export const setLiveCanvas = (active: boolean) => {
  navigationState.setLiveCanvas(active);
};
