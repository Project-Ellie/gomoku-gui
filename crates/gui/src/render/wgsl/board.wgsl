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

/// The wood, in board-space uv: a colour and a height for the normal.
///
/// The grain runs along the x axis, so it varies with y. Real grain is not a set
/// of even stripes: the bands wander, they are spaced unevenly, and their
/// contrast varies along the length of the board. Two warped band layers plus a
/// broad tone variation give that.
fn wood(uv: vec2<f32>) -> vec3<f32> {
    let frequency = g.wood_a.w;

    // Slow wander across the grain, and a second layer at a different scale, so
    // that bands never repeat at a fixed interval.
    let wander = (fbm(vec2<f32>(uv.x * 0.55, uv.y * 0.30), 4) - 0.5) * 1.9;
    let drift = (fbm(vec2<f32>(uv.x * 0.13, uv.y * 0.11), 3) - 0.5) * 3.4;

    // Two band systems, neither of them evenly spaced.
    let phase_a = uv.y * frequency + wander + drift;
    let phase_b = uv.y * (frequency * 0.47) + wander * 0.6 + drift * 0.5;
    let band_a = 0.5 + 0.5 * sin(phase_a * 6.2831853);
    let band_b = 0.5 + 0.5 * sin(phase_b * 6.2831853);

    // Soft, asymmetric bands: a wide light body and a narrow dark line.
    let grain_a = pow(band_a, 1.7);
    let grain_b = pow(band_b, 2.6);
    var grain = mix(grain_a, grain_b, 0.45);

    // A slow drift of tone across the board, as if it came from one part of a
    // log. Kept small: a strong version reads as a stain rather than as wood.
    let figure = fbm(vec2<f32>(uv.x * 0.09, uv.y * 0.08), 3);
    grain = clamp(grain * (0.90 + figure * 0.20), 0.0, 1.0);

    // Fine pores: short, thin, dark marks that follow the grain.
    let pores = smoothstep(0.62, 0.92, fbm(vec2<f32>(uv.x * 0.9, uv.y * 96.0), 4));

    let body = mix(g.wood_b.rgb, g.wood_a.rgb, grain * g.wood_b.w);
    let albedo = mix(body, g.wood_c.rgb, pores * g.wood_c.w);

    // The height field: one gentle ridge per band, and the pores cut into it.
    let height = grain * 0.30 - pores * 0.40;
    let slope_x = (fbm(vec2<f32>((uv.x + 0.02) * 0.9, uv.y * 96.0), 3)
        - fbm(vec2<f32>((uv.x - 0.02) * 0.9, uv.y * 96.0), 3)) * pores;
    let n = normalize(vec3<f32>(-slope_x * 3.0, -(height - 0.5) * 0.55, 1.0));

    let v = vec3<f32>(0.0, 0.0, 1.0);
    let l = key_light();
    let n_dot_l = max(dot(n, l), 0.0);
    let n_dot_v = max(dot(n, v), 1e-4);

    // Diffuse, with a little wrap so that the surface never goes fully black.
    let wrap = n_dot_l * 0.5 + 0.5;
    let diffuse = albedo * mix(g.env_a.w, 1.0, wrap);

    // Two specular lobes: a tight varnish highlight along the grain, and a wide
    // soft sheen. The board is varnished, so both are present.
    let along = g.wood_d.x;
    let across = g.wood_d.y;
    let tight = specular(n, v, l, along, 0.055);
    let wide = specular(n, v, l, across, 0.035);
    let sheen = pow(1.0 - n_dot_v, 4.0) * g.wood_d.z;

    // The varnish also reflects the room, which is what makes wood look wet.
    let reflection = environment(reflect(-v, n), mix(along, across, 0.5)) * g.wood_d.w;

    return tone_map(diffuse + vec3<f32>(tight * 0.55 + wide * 0.30 + sheen * 0.05) + reflection * 0.10);
}

@fragment
fn fs_board(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = unproject_board(frag.xy);
    let d = slab_sdf(uv);

    // One pixel in board units, taken from the screen-space derivative.
    let texel = max(length(fwidth(uv)), 1e-6);

    // The table: a dark neutral, a soft pool of light behind the board, and a
    // little noise so that it does not band.
    let centre = vec2<f32>(0.5, 0.5);
    let radial = length((frag.xy / g.window.xy - centre) * vec2<f32>(1.0, 1.25));
    let pool = 1.0 - smoothstep(0.15, 0.95, radial);
    var table = mix(vec3<f32>(0.012, 0.013, 0.016), vec3<f32>(0.045, 0.048, 0.056), pool);
    table += dither(frag.xy) * 0.004;

    // The slab casts a soft shadow, away from the light.
    let offset = -normalize(g.light.xy) * 0.30;
    let shadow = smoothstep(0.75, -0.02, slab_sdf(uv + offset));
    let ambient_occlusion = smoothstep(0.55, -0.05, slab_sdf(uv));
    var colour = table * (1.0 - shadow * 0.40) * (1.0 - ambient_occlusion * 0.25);

    if (d > 0.0) {
        return vec4<f32>(colour, 1.0);
    }

    // The top face, and the chamfer that runs around it.
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
        let chamfer_normal = normalize(vec3<f32>(-inward * (1.0 - bevel), 0.30));
        let lit = max(dot(chamfer_normal, key_light()), 0.0);
        // The chamfer is the same wood, lit at a different angle, and it catches
        // a bright line where it faces the light.
        surface *= mix(0.50, 1.35, lit);
        surface += vec3<f32>(0.05) * pow(lit, 6.0);
    }

    // Grid lines: thin, warm, and slightly engraved. A dark line with a faint
    // light line beside it reads as a cut into the wood rather than as paint.
    let half_width = texel * 0.55;
    let dist = min(abs(uv.x - round(uv.x)), abs(uv.y - round(uv.y)));
    let inside = step(0.0, uv.x) * step(uv.x, LINE_LAST) * step(0.0, uv.y) * step(uv.y, LINE_LAST);
    let line = inside * (1.0 - smoothstep(half_width * 0.5, half_width * 1.4, dist));
    let groove = inside * (1.0 - smoothstep(half_width * 1.6, half_width * 3.4, dist));
    surface = mix(surface, vec3<f32>(0.030, 0.021, 0.013), line * 0.85);
    surface *= 1.0 + groove * 0.05;

    return vec4<f32>(tone_map(surface), 1.0);
}
