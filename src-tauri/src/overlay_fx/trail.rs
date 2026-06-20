//! Cursor trail physics engine — ported from TD_Web_Trail.
//!
//! Spring-friction chain + lazy-brush + Catmull-Rom spline interpolation.
//! Designed to be called once per frame from the wgpu render loop (Track B)
//! or from a JS animation loop (Track A webview fallback).

use crate::settings::WgpuTrailConfig;

/// A single point in the trail chain.
#[derive(Clone, Copy, Debug)]
pub struct TrailPoint {
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
}

impl TrailPoint {
    pub fn zero() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            dx: 0.0,
            dy: 0.0,
        }
    }
}

/// The cursor trail state machine.
pub struct TrailSystem {
    /// Physics points (head = index 0, tail = N-1).
    points: Vec<TrailPoint>,
    /// Upsampled spline points for rendering.
    samples: Vec<TrailPoint>,
    /// Lazy-brush position (filtered cursor).
    brush: (f32, f32),
    /// Last raw cursor position for movement detection.
    last_cursor: (f32, f32),
    /// Currently animating (particles, fade, etc.) — keeps render loop alive.
    pub is_animating: bool,
    /// Frames since last movement (for idle-sleep).
    still_frames: u32,
}

impl TrailSystem {
    /// Create a new trail with the given number of segments.
    pub fn new(segments: usize) -> Self {
        Self {
            points: vec![TrailPoint::zero(); segments],
            samples: Vec::new(),
            brush: (0.0, 0.0),
            last_cursor: (0.0, 0.0),
            is_animating: false,
            still_frames: 0,
        }
    }

    /// Update physics for one frame. Call once per render frame.
    /// Returns true if a redraw is needed.
    pub fn update(&mut self, cursor_x: f32, cursor_y: f32, config: &WgpuTrailConfig) -> bool {
        let len = self.points.len();
        if len == 0 {
            return false;
        }

        // ── Lazy-brush filter ──────────────────────────────────────
        let lazy_radius = config.lazy_radius;
        let lazy_friction = config.lazy_friction;
        if lazy_radius > 0.0 {
            let dx = cursor_x - self.brush.0;
            let dy = cursor_y - self.brush.1;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > lazy_radius {
                let angle = dy.atan2(dx);
                let overshoot = dist - lazy_radius;
                if lazy_friction > 0.0 {
                    // Non-linear damping: factor = 1 - sqrt(1 - u²)
                    let u = 1.0 - lazy_friction.clamp(0.0, 0.999);
                    let factor = 1.0 - (1.0 - u * u).sqrt();
                    self.brush.0 += angle.cos() * overshoot * factor;
                    self.brush.1 += angle.sin() * overshoot * factor;
                } else {
                    self.brush.0 += angle.cos() * overshoot;
                    self.brush.1 += angle.sin() * overshoot;
                }
            }
        } else {
            self.brush = (cursor_x, cursor_y);
        }

        // ── Movement detection ────────────────────────────────────
        let moved = (cursor_x - self.last_cursor.0).abs() > 0.5
            || (cursor_y - self.last_cursor.1).abs() > 0.5;
        self.last_cursor = (cursor_x, cursor_y);

        if moved {
            self.still_frames = 0;
            self.is_animating = true;
        } else {
            self.still_frames += 1;
        }

        // ── Spring-friction chain ─────────────────────────────────
        let spring = config.spring;
        let friction = config.friction;
        let head_spring_factor = 0.5;
        let head_spring = spring * head_spring_factor;

        for i in 0..len {
            if i == 0 {
                // Head: chase the lazy-brush position
                let head = &mut self.points[0];
                head.dx += (self.brush.0 - head.x) * head_spring;
                head.dy += (self.brush.1 - head.y) * head_spring;
                head.dx *= friction;
                head.dy *= friction;
                head.x += head.dx;
                head.y += head.dy;
            } else {
                // Body: chase the previous point (copy to avoid split-borrow)
                let (prev_x, prev_y) = {
                    let prev = &self.points[i - 1];
                    (prev.x, prev.y)
                };
                let pt = &mut self.points[i];
                pt.dx += (prev_x - pt.x) * spring;
                pt.dy += (prev_y - pt.y) * spring;
                pt.dx *= friction;
                pt.dy *= friction;
                pt.x += pt.dx;
                pt.y += pt.dy;
            }
        }

        // ── Idle sleep ────────────────────────────────────────────
        let max_still = 120; // ~2 seconds at 60 fps
        if self.still_frames > max_still {
            self.is_animating = false;
        }

        self.is_animating
    }

    /// Build Catmull-Rom spline samples from the physics points.
    /// Call once per frame before rendering.
    pub fn build_spline(&mut self, steps: usize) {
        let n = self.points.len();
        if n < 2 {
            self.samples = self.points.clone();
            return;
        }

        self.samples.clear();

        for i in 0..n - 1 {
            let p0 = self.points[if i == 0 { 0 } else { i - 1 }];
            let p1 = self.points[i];
            let p2 = self.points[i + 1];
            let p3 = self.points[if i + 2 < n { i + 2 } else { n - 1 }];

            for s in 0..steps {
                let t = s as f32 / steps as f32;
                let t2 = t * t;
                let t3 = t2 * t;

                // Standard Catmull-Rom
                let x = 0.5
                    * ((2.0 * p1.x)
                        + (-p0.x + p2.x) * t
                        + (2.0 * p0.x - 5.0 * p1.x + 4.0 * p2.x - p3.x) * t2
                        + (-p0.x + 3.0 * p1.x - 3.0 * p2.x + p3.x) * t3);
                let y = 0.5
                    * ((2.0 * p1.y)
                        + (-p0.y + p2.y) * t
                        + (2.0 * p0.y - 5.0 * p1.y + 4.0 * p2.y - p3.y) * t2
                        + (-p0.y + 3.0 * p1.y - 3.0 * p2.y + p3.y) * t3);

                // Velocity: linear interpolation between the two enclosing physics points
                let dx = p1.dx * (1.0 - t) + p2.dx * t;
                let dy = p1.dy * (1.0 - t) + p2.dy * t;

                self.samples.push(TrailPoint { x, y, dx, dy });
            }
        }

        // Append the last point
        if let Some(last) = self.points.last() {
            self.samples.push(*last);
        }
    }

    /// The upsampled spline points (for rendering).
    pub fn spline_points(&self) -> &[TrailPoint] {
        &self.samples
    }

    /// Reset the trail to a position (e.g., when trail first appears).
    pub fn reset_to(&mut self, x: f32, y: f32) {
        self.brush = (x, y);
        self.last_cursor = (x, y);
        self.still_frames = 0;
        for pt in &mut self.points {
            pt.x = x;
            pt.y = y;
            pt.dx = 0.0;
            pt.dy = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> WgpuTrailConfig {
        WgpuTrailConfig {
            enabled: true,
            segments: 10,
            spring: 0.39,
            friction: 0.5,
            lazy_radius: 8.0,
            lazy_friction: 0.4,
            ..Default::default()
        }
    }

    #[test]
    fn trail_initializes_to_origin() {
        let t = TrailSystem::new(10);
        for pt in &t.points {
            assert_eq!(pt.x, 0.0);
            assert_eq!(pt.y, 0.0);
        }
    }

    #[test]
    fn update_produces_movement() {
        let mut t = TrailSystem::new(10);
        let cfg = test_config();
        t.update(100.0, 100.0, &cfg);
        // Head should have moved (spring pulls it toward the cursor)
        assert!(t.points[0].x > 0.0 || t.points[0].y > 0.0);
    }

    #[test]
    fn spline_upsamples() {
        let mut t = TrailSystem::new(10);
        t.reset_to(50.0, 50.0);
        t.build_spline(4);
        // With 10 points and 4 steps, we get (9 * 4 + 1) = 37 spline points
        assert_eq!(t.spline_points().len(), 37);
    }
}
