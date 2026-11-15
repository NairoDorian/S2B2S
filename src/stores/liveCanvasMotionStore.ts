import { readPref, writePref } from "@/lib/appIdentity";
import { createSolidStore } from "@/lib/solidStore";
import {
  LIVE_CANVAS_MOTION_DEFAULTS,
  normalizeLiveCanvasMotion,
  type LiveCanvasMotion,
} from "@/lib/liveCanvasMotion";

const PREF = "liveCanvas.motion";
const initial = () => {
  try {
    const value: unknown = JSON.parse(readPref(PREF) ?? "{}");
    const motion = normalizeLiveCanvasMotion(
      value && typeof value === "object" ? value : {},
    );
    // Upgrade only the previous untouched defaults; preserve authored speeds.
    if (
      value &&
      typeof value === "object" &&
      !("signal_scale" in value) &&
      motion.speed === 1 &&
      motion.travel_ms === 48 &&
      motion.particle_fade_ms === 8 &&
      motion.character_fade_ms === 8
    )
      motion.travel_ms = LIVE_CANVAS_MOTION_DEFAULTS.travel_ms;
    return motion;
  } catch {
    return { ...LIVE_CANVAS_MOTION_DEFAULTS };
  }
};

const state = createSolidStore<{
  motion: LiveCanvasMotion;
  update: (key: keyof LiveCanvasMotion, value: number) => void;
  reset: () => void;
}>((set, get) => ({
  motion: initial(),
  update: (key, value) => {
    const motion = normalizeLiveCanvasMotion({ ...get().motion, [key]: value });
    writePref(PREF, JSON.stringify(motion));
    set({ motion });
  },
  reset: () => {
    const motion = {
      ...LIVE_CANVAS_MOTION_DEFAULTS,
      signal_scale: get().motion.signal_scale,
    };
    writePref(PREF, JSON.stringify(motion));
    set({ motion });
  },
}));

export const useLiveCanvasMotionStore = () => state;
