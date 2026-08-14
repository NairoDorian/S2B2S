import { useEffect, useRef, type FC } from "react";
import {
  Command,
  X,
  Mic,
  Volume2,
  Sparkles,
  Navigation,
  Layers,
  type LucideIcon,
} from "lucide-react";
import { APP_SHORTCUTS, type KeyboardShortcut } from "../lib/shortcuts";
import { useSettings } from "../hooks/useSettings";

interface KeyboardShortcutsModalProps {
  isOpen: boolean;
  onClose: () => void;
}

const CATEGORY_ICONS: Record<string, LucideIcon> = {
  "Voice & AI": Mic,
  Navigation: Navigation,
  General: Layers,
};

/**
 * Keyboard Shortcuts modal cheat sheet with accessible ARIA dialog markup.
 */
export const KeyboardShortcutsModal: FC<KeyboardShortcutsModalProps> = ({
  isOpen,
  onClose,
}) => {
  const modalRef = useRef<HTMLDivElement>(null);
  const { settings } = useSettings();

  useEffect(() => {
    if (!isOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  const categories: KeyboardShortcut["category"][] = [
    "Voice & AI",
    "Navigation",
    "General",
  ];

  // Helper to get actual configured shortcut if available in settings
  const getShortcutDisplay = (shortcut: KeyboardShortcut): string => {
    if (shortcut.key === "transcribe") {
      const binding = settings?.bindings?.transcribe?.current_binding;
      return binding ? binding.toUpperCase() : shortcut.label;
    }
    if (shortcut.key === "converse") {
      const binding = settings?.bindings?.converse?.current_binding;
      return binding ? binding.toUpperCase() : shortcut.label;
    }
    if (shortcut.key === "speak_selection") {
      const binding = settings?.bindings?.speak_selection?.current_binding;
      return binding ? binding.toUpperCase() : shortcut.label;
    }
    return shortcut.label;
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-sm animate-in fade-in duration-200"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      role="presentation"
    >
      <div
        className="relative w-full max-w-xl max-h-[85vh] flex flex-col rounded-2xl border border-neutral-800 bg-neutral-950/95 shadow-2xl overflow-hidden"
        role="dialog"
        aria-modal="true"
        aria-labelledby="shortcuts-modal-title"
        ref={modalRef}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-neutral-800/80 bg-neutral-900/40">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-xl bg-amber-500/10 border border-amber-500/20 text-amber-400">
              <Command size={18} />
            </div>
            <div>
              <h2
                id="shortcuts-modal-title"
                className="text-base font-semibold text-neutral-100"
              >
                Keyboard Shortcuts
              </h2>
              <p className="text-xs text-neutral-400">
                Quick reference for global voice actions and app navigation
              </p>
            </div>
          </div>
          <button
            type="button"
            className="p-1.5 rounded-lg text-neutral-400 hover:text-neutral-100 hover:bg-neutral-800 transition-colors"
            onClick={onClose}
            aria-label="Close shortcuts dialog"
          >
            <X size={16} />
          </button>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6">
          {categories.map((cat) => {
            const list = APP_SHORTCUTS.filter((s) => s.category === cat);
            if (list.length === 0) return null;
            const Icon = CATEGORY_ICONS[cat] ?? Command;

            return (
              <div key={cat} className="space-y-2.5">
                <div className="flex items-center gap-2 text-xs font-semibold text-neutral-400 uppercase tracking-wider">
                  <Icon size={13} className="text-amber-400" />
                  <span>{cat}</span>
                </div>
                <div className="rounded-xl border border-neutral-800/80 bg-neutral-900/30 divide-y divide-neutral-800/50 overflow-hidden">
                  {list.map((sc) => (
                    <div
                      key={sc.key}
                      className="flex items-center justify-between px-4 py-2.5 text-xs hover:bg-neutral-800/30 transition-colors"
                    >
                      <span className="text-neutral-300 font-medium">
                        {sc.description}
                      </span>
                      <kbd className="px-2.5 py-1 text-[11px] font-mono font-semibold text-amber-300 bg-neutral-800/90 border border-neutral-700 rounded-md shadow-inner whitespace-nowrap">
                        {getShortcutDisplay(sc)}
                      </kbd>
                    </div>
                  ))}
                </div>
              </div>
            );
          })}
        </div>

        {/* Footer */}
        <div className="px-6 py-3 border-t border-neutral-800/80 bg-neutral-900/40 flex items-center justify-between text-xs text-neutral-500">
          <span>
            Press{" "}
            <kbd className="px-1.5 py-0.5 font-mono text-[10px] bg-neutral-800 rounded border border-neutral-700 text-neutral-300">
              Esc
            </kbd>{" "}
            or{" "}
            <kbd className="px-1.5 py-0.5 font-mono text-[10px] bg-neutral-800 rounded border border-neutral-700 text-neutral-300">
              ?
            </kbd>{" "}
            to dismiss
          </span>
          <span className="text-neutral-400">
            S2B2S v
            {typeof __APP_VERSION__ !== "undefined" ? __APP_VERSION__ : "0.1.4"}
          </span>
        </div>
      </div>
    </div>
  );
};
