// WGSL shader — Cursor trail ribbon + click ripple SDF.
// Ported from Cross_Platform_Rust_WebGPU_CursorFX + TD_Web_Trail glow recipe.

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) alpha: f32,
}

struct Uniforms {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

// ── Trail ribbon vertex ──────────────────────────────────────────────
// Input: physical screen-space path with per-point width and alpha.
@vertex
fn ribbon_vs(
    @location(0) pos: vec2<f32>,
    @location(1) next_pos: vec2<f32>,
    @location(2) width: f32,
    @location(3) alpha: f32,
    @location(4) side: f32,
) -> VertexOutput {
    let dir = normalize(next_pos - pos);
    let perp = vec2<f32>(-dir.y, dir.x) * side;
    let offset = perp * width * 0.5;
    let world_pos = vec4<f32>(pos + offset, 0.0, 1.0);
    return VertexOutput(
        uniforms.view_proj * world_pos,
        vec2<f32>(0.0),
        alpha,
    );
}

// ── Trail fragment ───────────────────────────────────────────────────
@fragment
fn ribbon_fs(in: VertexOutput) -> @location(0) vec4<f32> {
    let base = vec4<f32>(0.486, 0.227, 0.929, 1.0); // #7c3aed
    return base * in.alpha;
}

// ── Click ripple uniforms ────────────────────────────────────────────
struct RippleUniforms {
    view_proj: mat4x4<f32>,
    center: vec2<f32>,
    radius: f32,
    thickness: f32,
    opacity: f32,
}

@group(0) @binding(1) var<uniform> ripple: RippleUniforms;

// ── Ripple SDF circle vertex ─────────────────────────────────────────
@vertex
fn ripple_vs(
    @location(0) pos: vec2<f32>,
) -> VertexOutput {
    return VertexOutput(
        uniforms.view_proj * vec4<f32>(pos, 0.0, 1.0),
        pos,
        1.0,
    );
}

// ── Ripple SDF fragment ──────────────────────────────────────────────
@fragment
fn ripple_fs(in: VertexOutput) -> @location(0) vec4<f32> {
    let dist = length(in.uv - ripple.center);
    let inner = smoothstep(
        ripple.radius - ripple.thickness,
        ripple.radius,
        dist,
    );
    let outer = 1.0 - smoothstep(
        ripple.radius,
        ripple.radius + ripple.thickness,
        dist,
    );
    let ring = inner * outer;
    return vec4<f32>(0.486, 0.227, 0.929, ring * ripple.opacity);
}
