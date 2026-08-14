import { useCallback, useEffect, useRef, useState } from "react";
import { commands } from "@/bindings";

type CaptureState = "idle" | "creating" | "selected" | "moving" | "resizing";
type HandlePosition = "nw" | "n" | "ne" | "w" | "e" | "sw" | "s" | "se";

interface Region {
  x: number;
  y: number;
  width: number;
  height: number;
}

interface VirtualScreenInfo {
  offset_x: number;
  offset_y: number;
  total_width: number;
  total_height: number;
  scale_factor: number | null;
}

const MIN_REGION_SIZE = 10;
const HANDLE_SIZE = 10;

/**
 * Full-screen transparent overlay for selecting a screen region.
 * Coordinates are converted from logical (CSS) to physical pixels at the
 * confirm boundary using the primary monitor's scale factor.
 */
export const RegionCaptureOverlay: React.FC = () => {
  const [virtualScreen, setVirtualScreen] = useState<VirtualScreenInfo | null>(
    null,
  );
  const [state, setState] = useState<CaptureState>("idle");
  const [region, setRegion] = useState<Region | null>(null);
  const [activeHandle, setActiveHandle] = useState<HandlePosition | null>(null);
  const [dragStart, setDragStart] = useState<{ x: number; y: number } | null>(
    null,
  );
  const [regionStart, setRegionStart] = useState<Region | null>(null);
  const [error, setError] = useState<string | null>(null);

  const containerRef = useRef<HTMLDivElement>(null);

  // Region saved at the START of a click sequence (for double-click confirm).
  const savedRegionRef = useRef<Region | null>(null);
  const lastMouseDownTime = useRef<number>(0);

  useEffect(() => {
    const fetchData = async () => {
      const result = await commands.regionCaptureGetData();
      if (result.status === "ok") {
        setVirtualScreen(result.data.virtual_screen);
      } else {
        setError(String(result.error));
      }
    };
    void fetchData();
  }, []);

  const handleConfirm = useCallback(
    (regionToConfirm?: Region) => {
      const r = regionToConfirm || region;
      if (!virtualScreen) return;
      if (!r || r.width <= MIN_REGION_SIZE || r.height <= MIN_REGION_SIZE) {
        return;
      }
      // Logical → physical at the confirm boundary: the overlay window is
      // sized in logical pixels while the capture reads physical pixels.
      const scale = virtualScreen.scale_factor || 1;
      void commands.regionCaptureConfirm({
        x: Math.round(r.x * scale),
        y: Math.round(r.y * scale),
        width: Math.round(r.width * scale),
        height: Math.round(r.height * scale),
      });
    },
    [region, virtualScreen],
  );

  const handleDoubleClick = useCallback(() => {
    if (!virtualScreen) return;
    const savedRegion = savedRegionRef.current;
    // Clear the visible selection before confirming so the frame never
    // appears in the screenshot.
    setRegion(null);
    requestAnimationFrame(() => {
      if (
        savedRegion &&
        savedRegion.width > MIN_REGION_SIZE &&
        savedRegion.height > MIN_REGION_SIZE
      ) {
        handleConfirm(savedRegion);
      } else {
        handleConfirm({
          x: 0,
          y: 0,
          width: virtualScreen.total_width / (virtualScreen.scale_factor || 1),
          height:
            virtualScreen.total_height / (virtualScreen.scale_factor || 1),
        });
      }
    });
  }, [virtualScreen, handleConfirm]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        void commands.regionCaptureCancel();
      } else if (e.key === "Enter") {
        e.preventDefault();
        handleConfirm();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleConfirm]);

  const getHandleAtPoint = useCallback(
    (x: number, y: number): HandlePosition | null => {
      if (!region) return null;
      const handles: Record<HandlePosition, { x: number; y: number }> = {
        nw: { x: region.x, y: region.y },
        n: { x: region.x + region.width / 2, y: region.y },
        ne: { x: region.x + region.width, y: region.y },
        w: { x: region.x, y: region.y + region.height / 2 },
        e: { x: region.x + region.width, y: region.y + region.height / 2 },
        sw: { x: region.x, y: region.y + region.height },
        s: { x: region.x + region.width / 2, y: region.y + region.height },
        se: { x: region.x + region.width, y: region.y + region.height },
      };
      for (const [name, pos] of Object.entries(handles)) {
        if (
          Math.abs(x - pos.x) < HANDLE_SIZE &&
          Math.abs(y - pos.y) < HANDLE_SIZE
        ) {
          return name as HandlePosition;
        }
      }
      return null;
    },
    [region],
  );

  const isPointInRegion = useCallback(
    (x: number, y: number): boolean => {
      if (!region) return false;
      return (
        x >= region.x &&
        x <= region.x + region.width &&
        y >= region.y &&
        y <= region.y + region.height
      );
    },
    [region],
  );

  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      const rect = containerRef.current?.getBoundingClientRect();
      if (!rect) return;

      const now = Date.now();
      if (now - lastMouseDownTime.current > 400) {
        savedRegionRef.current = region;
      }
      lastMouseDownTime.current = now;

      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;
      setDragStart({ x, y });

      if (state === "idle") {
        setState("creating");
        setRegion({ x, y, width: 0, height: 0 });
      } else if (state === "selected") {
        const handle = getHandleAtPoint(x, y);
        if (handle) {
          setState("resizing");
          setActiveHandle(handle);
          setRegionStart(region ? { ...region } : null);
        } else if (isPointInRegion(x, y)) {
          setState("moving");
          setRegionStart(region ? { ...region } : null);
        } else {
          setState("creating");
          setRegion({ x, y, width: 0, height: 0 });
        }
      }
    },
    [state, region, getHandleAtPoint, isPointInRegion],
  );

  const handleMouseMove = useCallback(
    (e: React.MouseEvent) => {
      const rect = containerRef.current?.getBoundingClientRect();
      if (!rect || !dragStart) return;
      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;

      if (state === "creating") {
        setRegion({
          x: Math.min(dragStart.x, x),
          y: Math.min(dragStart.y, y),
          width: Math.abs(x - dragStart.x),
          height: Math.abs(y - dragStart.y),
        });
      } else if (state === "moving" && regionStart) {
        const deltaX = x - dragStart.x;
        const deltaY = y - dragStart.y;
        setRegion({
          ...regionStart,
          x: regionStart.x + deltaX,
          y: regionStart.y + deltaY,
        });
      } else if (state === "resizing" && regionStart && activeHandle) {
        const deltaX = x - dragStart.x;
        const deltaY = y - dragStart.y;
        const newRegion = { ...regionStart };

        if (activeHandle.includes("w")) {
          newRegion.x = regionStart.x + deltaX;
          newRegion.width = regionStart.width - deltaX;
        } else if (activeHandle.includes("e")) {
          newRegion.width = regionStart.width + deltaX;
        }
        if (activeHandle.includes("n")) {
          newRegion.y = regionStart.y + deltaY;
          newRegion.height = regionStart.height - deltaY;
        } else if (activeHandle.includes("s")) {
          newRegion.height = regionStart.height + deltaY;
        }

        if (newRegion.width < MIN_REGION_SIZE) {
          if (activeHandle.includes("w")) {
            newRegion.x = regionStart.x + regionStart.width - MIN_REGION_SIZE;
          }
          newRegion.width = MIN_REGION_SIZE;
        }
        if (newRegion.height < MIN_REGION_SIZE) {
          if (activeHandle.includes("n")) {
            newRegion.y = regionStart.y + regionStart.height - MIN_REGION_SIZE;
          }
          newRegion.height = MIN_REGION_SIZE;
        }

        setRegion(newRegion);
      }
    },
    [state, dragStart, regionStart, activeHandle],
  );

  const handleMouseUp = useCallback(() => {
    if (state === "creating") {
      if (
        region &&
        region.width > MIN_REGION_SIZE &&
        region.height > MIN_REGION_SIZE
      ) {
        setState("selected");
      } else {
        setRegion(null);
        setState("idle");
      }
    } else if (state === "moving" || state === "resizing") {
      setState("selected");
    }
    setDragStart(null);
    setRegionStart(null);
    setActiveHandle(null);
  }, [state, region]);

  const getDimStyles = () => {
    if (!region) {
      return {
        top: { height: "100%" },
        bottom: { height: 0 },
        left: { top: 0, height: 0, width: 0 },
        right: { top: 0, height: 0, width: 0 },
      };
    }
    return {
      top: { height: region.y },
      bottom: {
        top: region.y + region.height,
        height: `calc(100% - ${region.y + region.height}px)`,
      },
      left: { top: region.y, height: region.height, width: region.x },
      right: {
        top: region.y,
        height: region.height,
        left: region.x + region.width,
      },
    };
  };

  const dimStyles = getDimStyles();

  const getHintText = () => {
    if (error) return `Error: ${error}`;
    if (state === "idle")
      return "Drag to select a region, or double-click for full screen";
    if (state === "creating") return "Release to finish selection";
    if (state === "selected")
      return "Drag to move, use handles to resize. Enter or double-click to confirm, Escape to cancel";
    if (state === "moving") return "Release to finish moving";
    if (state === "resizing") return "Release to finish resizing";
    return "";
  };

  const containerClass = [
    "region-capture-container",
    state === "moving" ? "state-moving" : "",
    state === "resizing" && activeHandle
      ? `state-resizing-${activeHandle}`
      : "",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <div
      ref={containerRef}
      className={containerClass}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      onMouseLeave={handleMouseUp}
      onDoubleClick={handleDoubleClick}
    >
      <div className="dim-overlay dim-top" style={dimStyles.top} />
      <div className="dim-overlay dim-bottom" style={dimStyles.bottom} />
      <div className="dim-overlay dim-left" style={dimStyles.left} />
      <div className="dim-overlay dim-right" style={dimStyles.right} />

      {region && region.width > 0 && region.height > 0 && (
        <div
          className="selection-region"
          style={{
            left: region.x,
            top: region.y,
            width: region.width,
            height: region.height,
          }}
        >
          {state === "selected" && (
            <>
              <div className="resize-handle handle-nw" />
              <div className="resize-handle handle-n" />
              <div className="resize-handle handle-ne" />
              <div className="resize-handle handle-w" />
              <div className="resize-handle handle-e" />
              <div className="resize-handle handle-sw" />
              <div className="resize-handle handle-s" />
              <div className="resize-handle handle-se" />
            </>
          )}
        </div>
      )}

      <div className="hint-text">{getHintText()}</div>
    </div>
  );
};
