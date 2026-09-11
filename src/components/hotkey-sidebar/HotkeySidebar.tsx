import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { Keyboard, X } from "lucide-react";
import { useSettings } from "@/hooks/useSettings";
import { useNavigationStore } from "@/stores/navigationStore";
import { navigateToSettingsAnchor } from "@/lib/anchorNavigation";
import {
  buildHotkeyGuideCategories,
  getShortcutAnchorId,
  getShortcutSettingsSection,
} from "@/lib/hotkeyGuide";
import { Button } from "../ui/Button";
import { HotkeyGroup } from "./HotkeyGroup";

/**
 * Shortcut cheat sheet as a top-right overlay: a keyboard button in the
 * window's top-right corner toggles a panel listing the assigned hotkeys by
 * category; each row jumps to the control that changes it. Available from
 * every page; closes on Escape, on click outside, and after a jump.
 */
export const HotkeySidebar: React.FC = () => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const setSection = useNavigationStore((state) => state.setSection);
  const [isOpen, setIsOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  const categories = useMemo(
    () => buildHotkeyGuideCategories(settings),
    [settings],
  );
  const hasAnyHotkeys = categories.length > 0;

  const close = useCallback(() => setIsOpen(false), []);

  const handleHotkeyClick = useCallback(
    (shortcutId: string) => {
      navigateToSettingsAnchor({
        activateSection: () =>
          setSection(getShortcutSettingsSection(shortcutId)),
        targetId: getShortcutAnchorId(shortcutId),
        block: "center",
      });
      close();
    },
    [close, setSection],
  );

  const goToGeneral = useCallback(() => {
    navigateToSettingsAnchor({
      activateSection: () => setSection("general"),
      targetId: getShortcutAnchorId("transcribe"),
      block: "center",
    });
    close();
  }, [close, setSection]);

  // Escape and click-outside dismiss the overlay.
  useEffect(() => {
    if (!isOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    const onPointer = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        close();
      }
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("mousedown", onPointer);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("mousedown", onPointer);
    };
  }, [isOpen, close]);

  const toggleLabel = isOpen
    ? t("hotkeySidebar.closeSidebar")
    : t("hotkeySidebar.openSidebar");

  return (
    <div ref={rootRef} className="fixed top-3 end-3 z-50">
      <button
        type="button"
        onClick={() => setIsOpen((prev) => !prev)}
        aria-controls="hotkey-overlay-panel"
        aria-expanded={isOpen}
        aria-label={toggleLabel}
        title={toggleLabel}
        className={`flex items-center gap-1.5 h-8 px-2 border text-xs font-medium cursor-pointer transition-colors ${
          isOpen
            ? "border-logo-primary bg-logo-primary/15 text-logo-primary"
            : "border-mid-gray/30 bg-background text-text/70 hover:border-logo-primary hover:text-text"
        }`}
      >
        <Keyboard className="w-4 h-4" />
        {!hasAnyHotkeys && (
          <span className="w-1.5 h-1.5 bg-warning" aria-hidden="true" />
        )}
      </button>

      {isOpen && (
        <div
          id="hotkey-overlay-panel"
          role="dialog"
          aria-label={t("hotkeySidebar.title")}
          className="absolute top-full end-0 mt-2 w-[min(22rem,calc(100vw-2rem))] bg-background border border-mid-gray/30 shadow-xl flex flex-col max-h-[calc(100vh-5rem)]"
        >
          <div className="flex items-center justify-between px-3 py-2 border-b border-mid-gray/20">
            <h2 className="text-xs font-semibold uppercase tracking-wide text-text/70 flex items-center gap-2">
              <Keyboard className="w-4 h-4 text-logo-primary" />
              {t("hotkeySidebar.title")}
            </h2>
            <button
              type="button"
              onClick={close}
              aria-label={t("hotkeySidebar.closeSidebar")}
              className="p-1 text-text/50 hover:text-text cursor-pointer"
            >
              <X className="w-4 h-4" />
            </button>
          </div>

          <div className="flex-1 overflow-y-auto px-3 py-3">
            {hasAnyHotkeys ? (
              categories.map((category) => (
                <HotkeyGroup
                  key={category.id}
                  title={t(category.titleKey)}
                  hotkeys={category.hotkeys}
                  onHotkeyClick={handleHotkeyClick}
                />
              ))
            ) : (
              <div className="flex flex-col gap-3 border border-warning/30 bg-warning/10 p-3">
                <p className="text-sm font-medium text-text">
                  {t("hotkeySidebar.empty.title")}
                </p>
                <p className="text-xs text-text/70">
                  {t("hotkeySidebar.empty.message")}
                </p>
                <Button variant="primary-soft" size="sm" onClick={goToGeneral}>
                  {t("hotkeySidebar.empty.action")}
                </Button>
              </div>
            )}
          </div>

          <div className="px-3 py-2 border-t border-mid-gray/20 text-center">
            <span className="text-xs text-text/50">
              {t("hotkeySidebar.configureHint")}
            </span>
          </div>
        </div>
      )}
    </div>
  );
};
