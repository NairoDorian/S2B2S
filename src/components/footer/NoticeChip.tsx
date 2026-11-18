/* oxlint-disable jsx-a11y/prefer-tag-over-role */
import { createEffect, For, Show } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import type { Notice } from "@/bindings";
import { TriangleAlert, X } from "@/components/icons/lucide";
import { useTranslation } from "@/i18n/useTranslation";
import { noticeDisplay, noticePage } from "@/lib/notices";
import { setSection } from "@/stores/navigationStore";
import { useNoticeStore } from "@/stores/noticeStore";

const color = (notice: Pick<Notice, "level">) =>
  notice.level === "error" ? "text-error" : "text-warning";

/**
 * Counts the active notices in the footer and opens the panel that lists them.
 * Renders nothing while there are none.
 */
function NoticeChip() {
  const { t } = useTranslation();
  const store = useNoticeStore();
  let root: HTMLDivElement | undefined;

  createEffect(
    () => undefined,
    () => {
      void store.initialize();
      // The tray's notice line opens the window on the panel.
      const unlisten = listen("open-notices", () => store.setPanelOpen(true));
      return () => {
        void unlisten.then((fn) => fn());
      };
    },
  );

  createEffect(
    () => store.panelOpen,
    (open) => {
      if (!open) return;
      const handlePointerDown = (event: MouseEvent) => {
        if (root && !root.contains(event.target as Node))
          store.setPanelOpen(false);
      };
      const handleKeyDown = (event: KeyboardEvent) => {
        if (event.key === "Escape") store.setPanelOpen(false);
      };
      document.addEventListener("mousedown", handlePointerDown);
      document.addEventListener("keydown", handleKeyDown);
      return () => {
        document.removeEventListener("mousedown", handlePointerDown);
        document.removeEventListener("keydown", handleKeyDown);
      };
    },
  );

  const errors = () =>
    store.notices.filter((notice) => notice.level === "error").length;
  const warnings = () => store.notices.length - errors();
  // Plain keys, not plural ones: languages with more plural forms than the
  // app's `_one` / `_other` would fall back to English for a count of 2.
  const chipText = () => {
    const errorText = t(
      errors() === 1 ? "notices.errorOne" : "notices.errorMany",
      { n: errors() },
    );
    const warningText = t(
      warnings() === 1 ? "notices.warningOne" : "notices.warningMany",
      { n: warnings() },
    );
    if (errors() > 0 && warnings() > 0)
      return t("notices.errorsAndWarnings", {
        errors: errorText,
        warnings: warningText,
      });
    return errors() > 0 ? errorText : warningText;
  };

  const show = (notice: Notice) => {
    setSection(noticePage(notice.source));
    store.dismiss(notice);
    store.setPanelOpen(false);
  };

  return (
    <Show when={store.notices.length > 0}>
      <div ref={(el) => (root = el)} class="relative shrink-0" data-notices>
        <button
          type="button"
          aria-haspopup="dialog"
          aria-expanded={store.panelOpen ? "true" : "false"}
          onClick={() => store.setPanelOpen(!store.panelOpen)}
          class="flex items-center gap-1.5 whitespace-nowrap rounded-md border border-transparent px-1.5 py-1 transition-colors hover:border-mid-gray/25 hover:bg-mid-gray/10 hover:text-text/90"
        >
          <TriangleAlert
            class={`h-3.5 w-3.5 ${errors() > 0 ? "text-error" : "text-warning"}`}
          />
          {chipText()}
        </button>
        <Show when={store.panelOpen}>
          <div
            role="dialog"
            aria-label={t("notices.title")}
            class="absolute bottom-full end-0 z-50 mb-2 w-[min(24rem,calc(100vw-2rem))] overflow-hidden rounded-lg border border-mid-gray/25 bg-background shadow-xl"
          >
            <div class="flex items-center justify-between gap-2 border-b border-mid-gray/20 px-3 py-2">
              <div class="min-w-0">
                <h3 class="truncate text-xs font-semibold uppercase tracking-wide text-text/70">
                  {t("notices.title")}
                </h3>
                <p class="mt-0.5 whitespace-normal text-[11px] leading-snug text-text/45">
                  {t("notices.subtitle")}
                </p>
              </div>
              <button
                type="button"
                onClick={() => store.dismissAll()}
                class="shrink-0 text-[11px] text-text/60 underline-offset-2 hover:text-text hover:underline"
              >
                {t("notices.dismissAll")}
              </button>
            </div>
            <div class="max-h-[50vh] divide-y divide-mid-gray/15 overflow-y-auto">
              <For each={store.notices}>
                {(notice) => {
                  const display = noticeDisplay(notice.code);
                  return (
                    <div class="flex items-start gap-2.5 px-3 py-2.5 whitespace-normal">
                      <TriangleAlert
                        class={`mt-[3px] h-3.5 w-3.5 shrink-0 ${color(notice)}`}
                      />
                      <div class="min-w-0 flex-1 select-text break-words text-xs text-text/70">
                        <p class="text-sm font-medium text-text">
                          {t(display.title)}
                        </p>
                        <Show when={display.explanation}>
                          {(explain) => <p class="mt-0.5">{explain()(t)}</p>}
                        </Show>
                        <Show when={notice.detail}>
                          {(detail) => (
                            <p class="mt-0.5 font-mono text-[11px] text-text/50">
                              {detail()}
                            </p>
                          )}
                        </Show>
                        <button
                          type="button"
                          onClick={() => show(notice)}
                          class="mt-1 text-text underline underline-offset-2 hover:text-accent"
                        >
                          {t("notices.show")}
                        </button>
                      </div>
                      <button
                        type="button"
                        aria-label={t("notices.dismiss")}
                        title={t("notices.dismiss")}
                        onClick={() => store.dismiss(notice)}
                        class="-me-1 -mt-0.5 shrink-0 rounded-md p-1 text-text/40 transition-colors hover:text-text"
                      >
                        <X class="h-3.5 w-3.5" />
                      </button>
                    </div>
                  );
                }}
              </For>
            </div>
          </div>
        </Show>
      </div>
    </Show>
  );
}

export default NoticeChip;
