/**
 * The backend's active notices (`src-tauri/src/notices.rs`), mirrored for the
 * footer's chip and panel.
 *
 * A notice is a failure that must stay visible until it is resolved or
 * dismissed; transient messages stay toasts. A notice raised while the app runs
 * also pops up once as a toast (through `sessionToast`, so the Debug page's
 * toast history lists it), and then waits in the footer.
 */

import { commands, events, type Notice } from "@/bindings";
import i18n from "@/i18n";
import { noticeDisplay } from "@/lib/notices";
import { sessionToast as toast } from "@/lib/sessionToast";
import { createSolidStore } from "@/lib/solidStore";

interface NoticeStore {
  /** Every active notice, newest first. */
  notices: Notice[];
  panelOpen: boolean;
  initialized: boolean;
  initialize: () => Promise<void>;
  setPanelOpen: (open: boolean) => void;
  /** Ends a notice: here, in the tray and in the footer. */
  dismiss: (notice: Notice) => void;
  dismissAll: () => void;
}

/** The toast a newly raised notice pops up as. */
const toastNotice = (notice: Notice) => {
  const display = noticeDisplay(notice.code);
  const t = i18n.t.bind(i18n);
  const description =
    [display.explanation?.(t), notice.detail].filter(Boolean).join(" — ") ||
    undefined;
  const show = notice.level === "error" ? toast.error : toast.warning;
  show(t(display.title), { description });
};

export const noticeStore = createSolidStore<NoticeStore>((set, get) => ({
  notices: [],
  panelOpen: false,
  initialized: false,

  initialize: async () => {
    if (get().initialized) return;
    set({ initialized: true });

    // Listen first so nothing is missed; an event then outranks the snapshot,
    // which may have been read before it.
    let changed = false;
    await events.noticesChanged.listen(({ payload: { notices, raised } }) => {
      changed = true;
      set((state) => ({
        notices,
        panelOpen: state.panelOpen && notices.length > 0,
      }));
      if (raised) toastNotice(raised);
    });
    const notices = await commands.getActiveNotices();
    if (!changed) set({ notices });
  },

  setPanelOpen: (open) => set({ panelOpen: open && get().notices.length > 0 }),

  dismiss: (notice) => {
    void commands.dismissNotice(notice.source, notice.code);
  },

  dismissAll: () => {
    for (const notice of get().notices) {
      void commands.dismissNotice(notice.source, notice.code);
    }
  },
}));

export const useNoticeStore = () => noticeStore;
