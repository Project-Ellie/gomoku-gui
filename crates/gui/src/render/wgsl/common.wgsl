// Shared by the board, stone, and shadow shaders: the uniform block, the
// board-to-screen projection, noise, and the lighting functions.

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
    // roughness along the grain, roughness across, sheen, clearcoat
    wood_d: vec4<f32>,
    // slate rgb, roughness
    stone_a: vec4<f32>,
    // shell rgb, roughness
    stone_b: vec4<f32>,
    // sky colour rgb, ambient strength
    env_a: vec4<f32>,
    // ground colour rgb, exposure
    env_b: vec4<f32>,
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

/// The direction to the key light, normalised.
fn key_light() -> vec3<f32> {
    return normalize(g.light.xyz);
}

// --- noise ---------------------------------------------------------------

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
        q = q * 2.03 + vec2<f32>(19.1, 7.7);
        amplitude *= 0.5;
    }
    return sum / max(norm, 1e-5);
}

/// A small signed noise, used to break up smooth gradients.
fn dither(p: vec2<f32>) -> f32 {
    return hash21(p) - 0.5;
}

// --- lighting ------------------------------------------------------------

/// The Fresnel term at normal incidence `f0`.
fn fresnel(n_dot_v: f32, f0: f32) -> f32 {
    return f0 + (1.0 - f0) * pow(1.0 - n_dot_v, 5.0);
}

/// The Trowbridge-Reitz (GGX) normal distribution.
fn distribution(n_dot_h: f32, roughness: f32) -> f32 {
    let a = max(roughness * roughness, 1e-3);
    let a2 = a * a;
    let d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / max(3.14159265 * d * d, 1e-6);
}

/// The Smith geometry term, Schlick approximation.
fn geometry(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    let k = (roughness + 1.0) * (roughness + 1.0) / 8.0;
    let gv = n_dot_v / (n_dot_v * (1.0 - k) + k);
    let gl = n_dot_l / (n_dot_l * (1.0 - k) + k);
    return gv * gl;
}

/// One specular lobe of a Cook-Torrance BRDF.
fn specular(
    n: vec3<f32>,
    v: vec3<f32>,
    l: vec3<f32>,
    roughness: f32,
    f0: f32,
) -> f32 {
    let h = normalize(l + v);
    let n_dot_l = max(dot(n, l), 0.0);
    let n_dot_v = max(dot(n, v), 1e-4);
    let n_dot_h = max(dot(n, h), 0.0);
    let v_dot_h = max(dot(v, h), 0.0);
    let d = distribution(n_dot_h, roughness);
    let g = geometry(n_dot_v, n_dot_l, roughness);
    let f = fresnel(v_dot_h, f0);
    return d * g * f / max(4.0 * n_dot_v * n_dot_l, 1e-4);
}

/// A soft environment: a bright sky above, a dark floor below, and a bright
/// patch where the key light is. It is what makes a smooth surface read as
/// glossy rather than as a flat colour.
fn environment(n: vec3<f32>, roughness: f32) -> vec3<f32> {
    let sky = g.env_a.rgb;
    let ground = g.env_b.rgb;
    let up = 0.5 + 0.5 * n.z;
    var colour = mix(ground, sky, pow(up, 1.4));

    // A broad reflection of the light source, widened by the roughness.
    let l = key_light();
    let spot = pow(max(dot(n, l), 0.0), mix(180.0, 6.0, clamp(roughness, 0.0, 1.0)));
    colour += sky * spot * 0.55;
    return colour;
}

/// A gentle filmic response, so that bright areas roll off instead of clipping.
fn tone_map(c: vec3<f32>) -> vec3<f32> {
    let x = max(c, vec3<f32>(0.0)) * g.env_b.w;
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), vec3<f32>(0.0), vec3<f32>(1.0));
}
