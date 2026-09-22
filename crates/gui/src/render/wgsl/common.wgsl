// Shared by the board, stone, and shadow shaders: the uniform block, the
// board-to-screen projection, and noise.

struct Globals {
    // centre.xy in cells, pixels per cell, flipped (0.0 or 1.0)
    view: vec4<f32>,
    // framebuffer size in physical pixels, unused, unused
    window: vec4<f32>,
    // light direction, unused
    light: vec4<f32>,
    // wood lighter rgb, grain frequency
    wood_a: vec4<f32>,
    // wood darker rgb, ring contrast
    wood_b: vec4<f32>,
    // pore rgb, pore depth
    wood_c: vec4<f32>,
    // roughness along the grain, roughness across, sheen, unused
    wood_d: vec4<f32>,
    // slate rgb, roughness
    stone_a: vec4<f32>,
    // shell rgb, roughness
    stone_b: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;

/// A board point in cells to framebuffer pixels.
fn project_board(p: vec2<f32>) -> vec2<f32> {
    var d = p - g.view.xy;
    if (g.view.w > 0.5) {
        d = -d;
    }
    return g.window.xy * 0.5 + d * g.view.z;
}

/// A framebuffer pixel to a board point in cells. The inverse of the above.
fn unproject_board(frag: vec2<f32>) -> vec2<f32> {
    var d = (frag - g.window.xy * 0.5) / g.view.z;
    if (g.view.w > 0.5) {
        d = -d;
    }
    return g.view.xy + d;
}

/// Framebuffer pixels to clip space, with y up.
fn to_clip(screen: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(
        screen.x / g.window.x * 2.0 - 1.0,
        1.0 - screen.y / g.window.y * 2.0,
        0.0,
        1.0,
    );
}

fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, vec3<f32>(p3.y, p3.z, p3.x) + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>, octaves: i32) -> f32 {
    var q = p;
    var sum = 0.0;
    var amplitude = 0.5;
    var norm = 0.0;
    for (var i = 0; i < octaves; i += 1) {
        sum += amplitude * value_noise(q);
        norm += amplitude;
        q *= 2.03;
        amplitude *= 0.5;
    }
    return sum / max(norm, 1e-5);
}

/// A small signed noise, used to break up smooth gradients.
fn dither(p: vec2<f32>) -> f32 {
    return hash21(p) - 0.5;
}
