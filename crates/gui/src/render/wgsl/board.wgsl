// The board: background, wooden slab, grid lines, and the slab shadow.

const SLAB_HALF: f32 = 7.85;
const SLAB_RADIUS: f32 = 0.25;
const BEVEL: f32 = 0.22;
const LINE_LAST: f32 = 14.0;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
};

/// A full-screen triangle. The fragment shader works out the board point.
@vertex
fn vs_board(@builtin(vertex_index) index: u32) -> VsOut {
    var x = -1.0;
    var y = -1.0;
    if (index == 1u) {
        x = 3.0;
    }
    if (index == 2u) {
        y = 3.0;
    }
    var out: VsOut;
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    return out;
}

/// A rounded-rectangle distance field in board space. Negative inside.
fn slab_sdf(p: vec2<f32>) -> f32 {
    let half = vec2<f32>(SLAB_HALF - SLAB_RADIUS, SLAB_HALF - SLAB_RADIUS);
    let q = abs(p - vec2<f32>(7.0, 7.0)) - half;
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - SLAB_RADIUS;
}

/// Wood colour from board-space uv, lit and varnished.
fn wood(uv: vec2<f32>) -> vec3<f32> {
    // The grain runs along the x axis: low frequency in x, high in y.
    let grain_y = uv.y * g.wood_a.w;
    let warp = fbm(vec2<f32>(uv.x * 0.30, uv.y * 0.75), 4) - 0.5;
    let rings = fract(grain_y + warp * 2.4);
    let profile = pow(abs(2.0 * rings - 1.0), 0.6);

    // Thin pores that follow the grain.
    let pores = smoothstep(0.55, 0.88, fbm(vec2<f32>(uv.x * 1.8, uv.y * 42.0), 4));

    let body = mix(g.wood_b.rgb, g.wood_a.rgb, 1.0 - profile * g.wood_b.w);
    let albedo = mix(body, g.wood_c.rgb, pores * g.wood_c.w);

    // A height field from the grain and the pores drives the normal.
    let slope = (profile - 0.5) * 0.30 - pores * 0.45;
    let n = normalize(vec3<f32>(-slope * 0.9, -slope * 0.35, 1.0));
    let l = normalize(g.light.xyz);
    let v = vec3<f32>(0.0, 0.0, 1.0);

    let diffuse = 0.45 + 0.55 * max(dot(n, l), 0.0);
    // Anisotropy: a tight highlight across the grain, a broad sheen along it.
    let exponent = mix(20.0, 130.0, profile);
    let spec = pow(max(dot(reflect(-l, n), v), 0.0), exponent);
    let grazing = pow(1.0 - max(dot(n, v), 0.0), 3.0);

    return albedo * diffuse + vec3<f32>(spec * g.wood_d.b * 0.35 + grazing * g.wood_d.b * 0.10);
}

@fragment
fn fs_board(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = unproject_board(frag.xy);
    let d = slab_sdf(uv);

    // One pixel in board units, taken from the screen-space derivative.
    let texel = max(length(fwidth(uv)), 1e-6);

    // Background: a dark gradient, dithered so that it does not band.
    let depth = clamp(frag.y / max(g.window.y, 1.0), 0.0, 1.0);
    let base = mix(vec3<f32>(0.058, 0.061, 0.070), vec3<f32>(0.020, 0.022, 0.026), depth);
    var colour = base + dither(frag.xy) * 0.010;

    // The slab drops a soft shadow on the background, away from the light.
    let offset = -normalize(g.light.xy) * 0.40;
    let shadow = smoothstep(0.70, -0.05, slab_sdf(uv + offset));
    colour *= 1.0 - shadow * 0.60;

    if (d > 0.0) {
        return vec4<f32>(colour, 1.0);
    }

    // The top face, and the bevel that runs around it.
    var wood_uv = uv;
    var bevel = 0.0;
    if (d > -BEVEL) {
        bevel = clamp(1.0 + d / BEVEL, 0.0, 1.0);
        let inward = normalize(vec2<f32>(7.0, 7.0) - uv + vec2<f32>(1e-5, 1e-5));
        wood_uv = uv + inward * (1.0 - bevel) * 1.5;
    }
    var surface = wood(wood_uv);

    if (bevel > 0.0) {
        let inward = normalize(vec2<f32>(7.0, 7.0) - uv + vec2<f32>(1e-5, 1e-5));
        let n = normalize(vec3<f32>(-inward * (1.0 - bevel), 0.30));
        let lit = max(dot(n, normalize(g.light.xyz)), 0.0);
        surface *= mix(0.45, 1.30, lit);
    }

    // Grid lines, computed so that they stay about one pixel wide at any zoom.
    let half_width = texel * 0.62;
    let dist = min(abs(uv.x - round(uv.x)), abs(uv.y - round(uv.y)));
    let inside = step(0.0, uv.x) * step(uv.x, LINE_LAST) * step(0.0, uv.y) * step(uv.y, LINE_LAST);
    let coverage = inside * (1.0 - smoothstep(half_width * 0.5, half_width * 1.5, dist));
    surface = mix(surface, vec3<f32>(0.045, 0.030, 0.018), coverage * 0.90);

    return vec4<f32>(surface, 1.0);
}
