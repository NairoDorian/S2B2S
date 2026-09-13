import {
  createSignal,
  createEffect,
  createMemo,
  For,
  Show,
  onCleanup,
} from "solid-js";
import { useTranslation } from "@/i18n/useTranslation";
import { Keyboard, X } from "@/components/icons/lucide";
import { useSettings } from "@/hooks/useSettings";
import { setSection } from "@/stores/navigationStore";
import { navigateToSettingsAnchor } from "@/lib/anchorNavigation";
import {
  buildHotkeyGuideCategories,
  getShortcutAnchorId,
  getShortcutSettingsSection,
} from "@/lib/hotkeyGuide";
import { Button } from "../ui/Button";
import { HotkeyGroup } from "./HotkeyGroup";
import type { JSX } from "@solidjs/web";

export const HotkeySidebar = (): JSX.Element => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const [isOpen, setIsOpen] = createSignal(false);
  let rootRef: HTMLDivElement | null = null;

  const categories = createMemo(() => buildHotkeyGuideCategories(settings()));
  const hasAnyHotkeys = categories().length > 0;

  const close = () => setIsOpen(false);
  const handleHotkeyClick = (shortcutId: string) => {
    navigateToSettingsAnchor({
      activateSection: () => setSection(getShortcutSettingsSection(shortcutId)),
      targetId: getShortcutAnchorId(shortcutId),
      block: "center",
    });
    close();
  };
  const goToGeneral = () => {
    navigateToSettingsAnchor({
      activateSection: () => setSection("general"),
      targetId: getShortcutAnchorId("transcribe"),
      block: "center",
    });
    close();
  };

  createEffect(
    () => isOpen(),
    (open) => {
      if (!open) return;
      const onKey = (e: KeyboardEvent) => {
        if (e.key === "Escape") close();
      };
      const onPointer = (e: MouseEvent) => {
        if (rootRef && !rootRef.contains(e.target as Node)) close();
      };
      window.addEventListener("keydown", onKey);
      window.addEventListener("mousedown", onPointer);
      onCleanup(() => {
        window.removeEventListener("keydown", onKey);
        window.removeEventListener("mousedown", onPointer);
      });
    },
  );
  const toggleLabel = isOpen()
    ? t("hotkeySidebar.closeSidebar")
    : t("hotkeySidebar.openSidebar");

  return (
    <div
      ref={(el) => {
        rootRef = el;
      }}
      class="fixed top-3 end-3 z-50"
    >
      <button
        type="button"
        onClick={() => setIsOpen((prev) => !prev)}
        aria-controls="hotkey-overlay-panel"
        aria-expanded={isOpen() ? "true" : "false"}
        aria-label={toggleLabel}
        title={toggleLabel}
        class={`flex items-center gap-1.5 h-8 px-2 border text-xs font-medium cursor-pointer transition-colors ${isOpen() ? "border-accent bg-accent/15 text-accent" : "border-mid-gray/30 bg-background text-text/70 hover:border-accent hover:text-text"}`}
      >
        <Keyboard class="w-4 h-4" />
        {!hasAnyHotkeys && (
          <span class="w-1.5 h-1.5 bg-warning" aria-hidden="true" />
        )}
      </button>
      <Show when={isOpen()}>
        <div
          id="hotkey-overlay-panel"
          role="dialog"
          aria-label={t("hotkeySidebar.title")}
          class="absolute top-full end-0 mt-2 w-[min(22rem,calc(100vw-2rem))] bg-background border border-mid-gray/30 shadow-xl flex flex-col max-h-[calc(100vh-5rem)]"
        >
          <div class="flex items-center justify-between px-3 py-2 border-b border-mid-gray/20">
            <h2 class="text-xs font-semibold uppercase tracking-wide text-text/70 flex items-center gap-2">
              <Keyboard class="w-4 h-4 text-accent" />
              {t("hotkeySidebar.title")}
            </h2>
            <button
              type="button"
              onClick={close}
              aria-label={t("hotkeySidebar.closeSidebar")}
              class="p-1 text-text/50 hover:text-text cursor-pointer"
            >
              <X class="w-4 h-4" />
            </button>
          </div>
          <div class="flex-1 overflow-y-auto px-3 py-3">
            <Show when={hasAnyHotkeys}>
              <For each={categories()}>
                {(category) => (
                  <HotkeyGroup
                    title={t(category.titleKey)}
                    hotkeys={category.hotkeys}
                    onHotkeyClick={handleHotkeyClick}
                  />
                )}
              </For>
            </Show>
            <Show when={!hasAnyHotkeys}>
              <div class="flex flex-col gap-3 border border-warning/30 bg-warning/10 p-3">
                <p class="text-sm font-medium text-text">
                  {t("hotkeySidebar.empty.title")}
                </p>
                <p class="text-xs text-text/70">
                  {t("hotkeySidebar.empty.message")}
                </p>
                <Button variant="primary-soft" size="sm" onClick={goToGeneral}>
                  {t("hotkeySidebar.empty.action")}
                </Button>
              </div>
            </Show>
          </div>
          <div class="px-3 py-2 border-t border-mid-gray/20 text-center">
            <span class="text-xs text-text/50">
              {t("hotkeySidebar.configureHint")}
            </span>
          </div>
        </div>
      </Show>
    </div>
  );
};
