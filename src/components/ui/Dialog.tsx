import React, { useEffect, useRef } from "react";

interface DialogProps {
  open: boolean;
  onClose: () => void;
  title: string;
  description?: string;
  children: React.ReactNode;
  footer?: React.ReactNode;
}

export const Dialog: React.FC<DialogProps> = ({
  open,
  onClose,
  title,
  description,
  children,
  footer,
}) => {
  const overlayRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div
      ref={overlayRef}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
      onMouseDown={(e) => {
        if (e.target === overlayRef.current) onClose();
      }}
    >
      <div className="relative w-full max-w-lg mx-4 bg-[#1a1a2e] border border-mid-gray/30 rounded-2xl shadow-2xl overflow-hidden">
        <div className="px-6 pt-6 pb-4 border-b border-mid-gray/15">
          <h2 className="text-lg font-semibold">{title}</h2>
          {description && (
            <p className="text-sm text-text/55 mt-1">{description}</p>
          )}
        </div>
        <div className="px-6 py-4 max-h-[60vh] overflow-y-auto">{children}</div>
        {footer && (
          <div className="px-6 py-4 border-t border-mid-gray/15 flex items-center gap-3">
            {footer}
          </div>
        )}
      </div>
    </div>
  );
};
