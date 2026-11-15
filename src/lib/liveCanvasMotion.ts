export interface LiveCanvasMotion {
  speed: number;
  travel_ms: number;
  particle_fade_ms: number;
  character_fade_ms: number;
  signal_scale: number;
  correction_out_ms: number;
  correction_in_ms: number;
}

export const LIVE_CANVAS_MOTION_DEFAULTS: LiveCanvasMotion = {
  speed: 1,
  travel_ms: 96,
  particle_fade_ms: 8,
  character_fade_ms: 8,
  signal_scale: 1,
  correction_out_ms: 8,
  correction_in_ms: 8,
};

export const LIVE_CANVAS_MOTION_LIMITS = {
  speed: { min: 0.25, max: 4, step: 0.25 },
  travel_ms: { min: 8, max: 400, step: 1 },
  particle_fade_ms: { min: 2, max: 200, step: 1 },
  character_fade_ms: { min: 2, max: 200, step: 1 },
  signal_scale: { min: 0.1, max: 4, step: 0.1 },
  correction_out_ms: { min: 2, max: 200, step: 1 },
  correction_in_ms: { min: 2, max: 200, step: 1 },
};

export function normalizeLiveCanvasMotion(
  value: Partial<LiveCanvasMotion>,
): LiveCanvasMotion {
  const result = { ...LIVE_CANVAS_MOTION_DEFAULTS };
  for (const key of Object.keys(result) as (keyof LiveCanvasMotion)[]) {
    const candidate = value[key];
    const { min, max } = LIVE_CANVAS_MOTION_LIMITS[key];
    if (typeof candidate === "number" && Number.isFinite(candidate))
      result[key] = Math.max(min, Math.min(max, candidate));
  }
  return result;
}

export function liveCanvasTiming(value: LiveCanvasMotion) {
  const settings = normalizeLiveCanvasMotion(value);
  const travel = settings.travel_ms / settings.speed;
  const particleFade = settings.particle_fade_ms / settings.speed;
  const characterFade = settings.character_fade_ms / settings.speed;
  // Particle opacity uses a linear fade: exactly 25% remains at hand-off.
  const characterStart = travel + particleFade * 0.75;
  return {
    travel,
    particleFade,
    characterFade,
    characterStart,
    duration: Math.max(travel + particleFade, characterStart + characterFade),
  };
}
