// The board: background, wooden slab, grid lines, and the slab shadow.
//
// The wood is a photograph of a real board, supplied by the owner. It is tiled
// with mirrored edges and two offset copies are crossfaded, so no seam and no
// obvious repeat shows. Everything the photograph cannot provide at close range
// is added procedurally on top, and the lighting never comes from the image.

const SLAB_HALF: f32 = 7.85;
const SLAB_RADIUS: f32 = 0.25;
const BEVEL: f32 = 0.22;
const LINE_LAST: f32 = 14.0;

/// How much of the board one copy of the photograph covers, in cells. The
/// proportions of the image are kept, so the grain is not stretched.
const TILE: vec2<f32> = vec2<f32>(5.0, 8.6);

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

/// Sample the photograph, with its grain turned to run along the board.
///
/// The photograph's grain runs down the image, and a board's grain runs along its
/// long axis, so the two axes are swapped. The proportions of the tile are chosen
/// so that the swap keeps the grain at its natural width.
fn sample_wood(t: vec2<f32>) -> vec3<f32> {
    return textureSample(wood_texture, wood_sampler, mirror_uv(vec2<f32>(t.y, t.x))).rgb;
}

/// The wood, in board-space uv. `texel` is one screen pixel in board units.
fn wood(uv: vec2<f32>, texel: f32) -> vec3<f32> {
    let tiled = uv / TILE;

    // Three copies of the photograph, at different scales and offsets, mixed by
    // slow noise. Mirror tiling makes each copy's own edges match, so no seam
    // shows; the different scales stop the three from repeating in step, which is
    // what a single copy would do. The changing scale also varies the width of
    // the grain, as real wood does.
    let copy_a = sample_wood(tiled);
    let copy_b = sample_wood(tiled * 0.62 + vec2<f32>(0.31, 0.57));
    let copy_c = sample_wood(tiled * 1.61 + vec2<f32>(0.63, 0.19));

    // The weights vary faster than a tile does, so no single copy can own a whole
    // tile: that is what stops the repeat from showing at any shift.
    let weight_a = smoothstep(0.20, 0.80, fbm(vec2<f32>(uv.x * 0.15, uv.y * 0.12), 3));
    let weight_b = smoothstep(0.20, 0.80, fbm(vec2<f32>(uv.x * 0.12 + 5.2, uv.y * 0.16 + 1.7), 3));
    let total = max(weight_a + weight_b, 1e-3);
    var albedo = (copy_a * (1.0 - weight_a) + copy_b * weight_a * (1.0 - weight_b)
        + copy_c * weight_a * weight_b) / total;
    albedo *= g.wood_a.rgb;

    // A slow drift of tone across the board, so the eye cannot find the tiles.
    let figure = fbm(vec2<f32>(uv.x * 0.09, uv.y * 0.08), 3) - 0.5;
    albedo *= 1.0 + figure * 0.12;

    // Fine pores, added as a darkening. They fade out as one pixel starts to
    // cover a pore, so the board never shimmers.
    let pore_detail = smoothstep(0.6, 2.4, 1.0 / max(40.0 * texel, 1e-6));
    let pores = smoothstep(0.45, 0.80, fbm(vec2<f32>(uv.x * 0.9, uv.y * 40.0), 4)) * pore_detail;
    albedo *= mix(1.0, 0.55, pores * g.wood_c.w);

    // The normal comes from the photograph's own grain, so the grain lines catch
    // the light instead of looking printed. Only the gradient across the grain
    // matters, because the grain runs along the board.
    let offset = vec2<f32>(0.0, texel * 1.5);
    let above = luminance(sample_wood((uv + offset) / TILE));
    let below = luminance(sample_wood((uv - offset) / TILE));
    let slope = (above - below) / max(2.0 * offset.y, 1e-6);
    // A little relief across the grain too, from the mottle, so the surface is
    // not a perfect cylinder.
    let mottle = fbm(vec2<f32>(uv.x * 2.6, uv.y * 7.5), 3);
    let n = normalize(vec3<f32>(-(mottle - 0.5) * 0.9, -slope * 0.09, 1.0));

    let v = vec3<f32>(0.0, 0.0, 1.0);
    let l = key_light();
    let n_dot_l = max(dot(n, l), 0.0);
    let n_dot_v = max(dot(n, v), 1e-4);

    let wrap = n_dot_l * 0.5 + 0.5;
    let diffuse = albedo * mix(g.env_a.w, 1.0, wrap);

    // A varnished board has a tight highlight along the grain and a wide sheen,
    // and it reflects the room. That reflection is what makes it look polished.
    let along = g.wood_d.x;
    let across = g.wood_d.y;
    let tight = specular(n, v, l, along, 0.055);
    let wide = specular(n, v, l, across, 0.035);
    let sheen = pow(1.0 - n_dot_v, 4.0) * g.wood_d.z;
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
    var surface = wood(wood_uv, texel);

    if (bevel > 0.0) {
        let inward = normalize(vec2<f32>(7.0, 7.0) - uv + vec2<f32>(1e-5, 1e-5));
        let chamfer_normal = normalize(vec3<f32>(-inward * (1.0 - bevel), 0.30));
        let lit = max(dot(chamfer_normal, key_light()), 0.0);
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
