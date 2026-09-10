import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { ChevronLeft, ChevronRight, Keyboard, Pin, PinOff } from "lucide-react";
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

const DEFAULT_WIDTH = 320;
const MIN_WIDTH = 240;
const MAX_WIDTH = 560;
const PINNED_KEY = "handy.hotkeySidebar.pinned";
const WIDTH_KEY = "handy.hotkeySidebar.width";

const readStorage = <T,>(
  key: string,
  parse: (raw: string) => T | null,
): T | null => {
  try {
    const raw = window.localStorage.getItem(key);
    return raw === null ? null : parse(raw);
  } catch {
    return null;
  }
};

const writeStorage = (key: string, value: string) => {
  try {
    window.localStorage.setItem(key, value);
  } catch {
    // A UI preference only; keep working without storage.
  }
};

/**
 * Right-edge cheat sheet of the shortcuts currently assigned. A tab on the
 * window's right edge opens a pinnable, resizable panel; each row jumps to
 * the control that configures that shortcut. Ported from the AIVORelay fork,
 * with one change: when nothing is bound (a fresh install here ships without
 * a transcribe hotkey) the panel explains that and links to General instead
 * of hiding, so the reminder exists exactly when it is needed.
 */
export const HotkeySidebar: React.FC = () => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const setSection = useNavigationStore((state) => state.setSection);

  const [isPinned, setIsPinned] = useState(
    () => readStorage(PINNED_KEY, (raw) => raw === "true") ?? false,
  );
  const [isOpen, setIsOpen] = useState(isPinned);
  const [width, setWidth] = useState(() => {
    const saved = readStorage(WIDTH_KEY, (raw) => Number(raw));
    return saved && Number.isFinite(saved)
      ? Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, saved))
      : DEFAULT_WIDTH;
  });
  const [isDragging, setIsDragging] = useState(false);
  const [isResizing, setIsResizing] = useState(false);
  const dragStartX = useRef(0);
  const resizeStartX = useRef(0);
  const resizeStartWidth = useRef(DEFAULT_WIDTH);
  // A drag that toggled the panel is followed by a synthetic click on the
  // tab; swallow that one so it does not undo the drag.
  const suppressNextClick = useRef(false);

  const categories = useMemo(
    () => buildHotkeyGuideCategories(settings),
    [settings],
  );
  const hasAnyHotkeys = categories.length > 0;

  const handleTogglePin = useCallback(() => {
    setIsPinned((prev) => {
      const next = !prev;
      writeStorage(PINNED_KEY, String(next));
      if (next) setIsOpen(true);
      return next;
    });
  }, []);

  const handleToggleOpen = useCallback(() => {
    if (suppressNextClick.current) {
      suppressNextClick.current = false;
      return;
    }
    setIsOpen((prev) => !prev);
  }, []);

  const handleHotkeyClick = useCallback(
    (shortcutId: string) => {
      navigateToSettingsAnchor({
        activateSection: () =>
          setSection(getShortcutSettingsSection(shortcutId)),
        targetId: getShortcutAnchorId(shortcutId),
        block: "center",
      });
      if (!isPinned) setIsOpen(false);
    },
    [isPinned, setSection],
  );

  const goToGeneral = useCallback(() => {
    navigateToSettingsAnchor({
      activateSection: () => setSection("general"),
      targetId: getShortcutAnchorId("transcribe"),
      block: "center",
    });
    if (!isPinned) setIsOpen(false);
  }, [isPinned, setSection]);

  // Edge tab: click toggles, a horizontal drag past 50 px also toggles.
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    suppressNextClick.current = false;
    setIsDragging(true);
    dragStartX.current = e.clientX;
    e.preventDefault();
  }, []);

  useEffect(() => {
    if (!isDragging) return;
    const onMove = (e: MouseEvent) => {
      const deltaX = dragStartX.current - e.clientX;
      if (deltaX > 50 && !isOpen) {
        setIsOpen(true);
        setIsDragging(false);
        suppressNextClick.current = true;
      } else if (deltaX < -50 && isOpen) {
        setIsOpen(false);
        setIsDragging(false);
        suppressNextClick.current = true;
      }
    };
    const onUp = () => setIsDragging(false);
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, [isDragging, isOpen]);

  const handleResizeStart = useCallback(
    (e: React.MouseEvent) => {
      setIsResizing(true);
      resizeStartX.current = e.clientX;
      resizeStartWidth.current = width;
      e.preventDefault();
      e.stopPropagation();
    },
    [width],
  );

  useEffect(() => {
    if (!isResizing) return;
    const onMove = (e: MouseEvent) => {
      const deltaX = resizeStartX.current - e.clientX;
      setWidth(
        Math.min(
          MAX_WIDTH,
          Math.max(MIN_WIDTH, resizeStartWidth.current + deltaX),
        ),
      );
    };
    const onUp = () => {
      setIsResizing(false);
      setWidth((w) => {
        writeStorage(WIDTH_KEY, String(w));
        return w;
      });
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, [isResizing]);

  const toggleLabel = isOpen
    ? t("hotkeySidebar.closeSidebar")
    : t("hotkeySidebar.openSidebar");
  const pinLabel = isPinned
    ? t("hotkeySidebar.unpinSidebar")
    : t("hotkeySidebar.pinSidebar");

  return (
    <>
      {/* Edge tab */}
      <button
        type="button"
        className="fixed top-1/2 -translate-y-1/2 z-50 cursor-pointer select-none transition-all duration-300 ease-out"
        style={{ right: isOpen ? width : 0 }}
        onMouseDown={handleMouseDown}
        onClick={handleToggleOpen}
        aria-controls="hotkey-sidebar-panel"
        aria-expanded={isOpen}
        aria-label={toggleLabel}
        title={toggleLabel}
      >
        <div
          className={`relative flex items-center justify-center h-20 rounded-s-lg bg-background border border-e-0 border-mid-gray/30 shadow-lg transition-all duration-200 hover:bg-logo-primary/15 ${
            isDragging ? "w-8 bg-logo-primary/20" : "w-6 hover:w-8"
          }`}
        >
          {isOpen ? (
            <ChevronRight className="w-4 h-4 text-text/60" />
          ) : (
            <div className="flex flex-col items-center gap-1">
              <ChevronLeft className="w-4 h-4 text-text/60" />
              <Keyboard className="w-3.5 h-3.5 text-logo-primary" />
            </div>
          )}
        </div>
      </button>

      {/* Panel */}
      <div
        id="hotkey-sidebar-panel"
        role="complementary"
        aria-label={t("hotkeySidebar.title")}
        aria-hidden={!isOpen}
        className={`fixed top-0 right-0 h-full z-40 transition-transform duration-300 ease-out ${
          isOpen ? "translate-x-0" : "translate-x-full"
        }`}
        style={{ width }}
      >
        <div
          className="absolute left-0 top-0 w-1 h-full cursor-ew-resize hover:bg-logo-primary/40 transition-colors z-10"
          onMouseDown={handleResizeStart}
        />
        <div className="h-full bg-background border-s border-mid-gray/20 flex flex-col shadow-2xl">
          <div className="flex items-center justify-between px-4 py-4 border-b border-mid-gray/20">
            <div className="flex items-center gap-2">
              <Keyboard className="w-5 h-5 text-logo-primary" />
              <h2 className="text-base font-semibold text-text">
                {t("hotkeySidebar.title")}
              </h2>
            </div>
            <button
              type="button"
              onClick={handleTogglePin}
              aria-label={pinLabel}
              title={pinLabel}
              className={`p-2 rounded-lg transition-colors cursor-pointer ${
                isPinned
                  ? "bg-logo-primary/20 text-logo-primary"
                  : "text-text/50 hover:text-text hover:bg-mid-gray/20"
              }`}
            >
              {isPinned ? (
                <Pin className="w-4 h-4" />
              ) : (
                <PinOff className="w-4 h-4" />
              )}
            </button>
          </div>

          <div className="flex-1 overflow-y-auto px-4 py-4">
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
              <div className="flex flex-col gap-3 rounded-lg border border-warning/30 bg-warning/10 p-3">
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

          <div className="px-4 py-3 border-t border-mid-gray/20 text-center">
            <span className="text-xs text-text/50">
              {t("hotkeySidebar.configureHint")}
            </span>
          </div>
        </div>
      </div>
    </>
  );
};
