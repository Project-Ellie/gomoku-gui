// The board: background, wooden slab, grid lines, coordinate labels, dimples,
// and the slab shadow.
//
// The wood is a photograph of a real board, supplied by the owner. It is tiled
// with mirrored edges and two offset copies are crossfaded, so no seam and no
// obvious repeat shows. Everything the photograph cannot provide at close range
// is added procedurally on top, and the lighting never comes from the image.

const SLAB_HALF: f32 = 7.85;
const SLAB_RADIUS: f32 = 0.25;
const BEVEL: f32 = 0.22;
const LINE_LAST: f32 = 14.0;

/// Distance from the outer grid line to the slab edge, measured from the
/// rounded-rectangle geometry in `slab_sdf`.
const SLAB_EDGE: f32 = SLAB_HALF - 7.0;
/// The chamfer begins one `BEVEL` inside the slab edge, so the flat margin is
/// the band from the outer grid line to `SLAB_EDGE - BEVEL`.
const LABEL_MARGIN: f32 = (SLAB_EDGE - BEVEL) * 0.5;
/// The board-space size of one glyph cell.  Letters and single digits are one
/// cell wide; two-digit numbers are one cell plus one digit advance wide.
const LABEL_SIZE: f32 = 0.34;
const LABEL_HALF: f32 = LABEL_SIZE * 0.5;
/// Horizontal advance from one digit origin to the next in the kerned pairs.
/// Menlo Bold digits at the atlas font size advance ~60.1 px in a 128 px cell,
/// so the natural step is ~0.47 of the glyph cell.
const DIGIT_ADVANCE: f32 = LABEL_SIZE * 0.47;
const PAIR_WIDTH: f32 = LABEL_SIZE + DIGIT_ADVANCE;
const PAIR_HALF: f32 = PAIR_WIDTH * 0.5;

/// Saturated red for the last-move glow.  The small green/blue components keep
/// it from reading as a magenta light, but it stays visibly red.
const GLOW_HUE: vec3<f32> = vec3<f32>(0.95, 0.04, 0.02);
const GLOW_STRENGTH: f32 = 5.5;

/// How much of the board one copy of the photograph covers, in cells. The
/// proportions of the image are kept, so the grain is not stretched.
const TILE: vec2<f32> = vec2<f32>(5.0, 8.6);

/// Atlas layout: 16 columns by 2 rows.  Row 0 is A-O, row 1 is 0-9.
const GLYPH_ATLAS_COLS: f32 = 16.0;
const GLYPH_ATLAS_ROWS: f32 = 2.0;
const GLYPH_ROW_LETTERS: i32 = 0;
const GLYPH_ROW_DIGITS: i32 = 1;

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

    // The photograph has a broad bright patch and a broad dark one, and on a real
    // board that reads as a stain. The local tone is measured by averaging five
    // samples spread over about two cells, which is far wider than a grain line
    // and far narrower than the blotches, and the colour is then divided by it.
    let reach = vec2<f32>(1.6, 1.6);
    let broad = 0.2 * (
        luminance(albedo)
        + luminance(sample_wood((uv + vec2<f32>(reach.x, 0.0)) / TILE))
        + luminance(sample_wood((uv - vec2<f32>(reach.x, 0.0)) / TILE))
        + luminance(sample_wood((uv + vec2<f32>(0.0, reach.y)) / TILE))
        + luminance(sample_wood((uv - vec2<f32>(0.0, reach.y)) / TILE))
    );
    let average_broad = 0.055;
    albedo *= clamp(pow(average_broad / max(broad, 0.004), 0.85), 0.30, 2.6);

    // A slow drift of tone across the board, so the eye cannot find the tiles.
    let figure = fbm(vec2<f32>(uv.x * 0.09, uv.y * 0.08), 3) - 0.5;
    albedo *= 1.0 + figure * 0.08;

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

/// Red glow around the last-placed stone.  The result is blended onto the
/// already tone-mapped board surface so the brightest ring stays saturated red
/// instead of clipping the green/blue channels to yellow.
fn last_move_glow(uv: vec2<f32>, surface: vec3<f32>) -> vec3<f32> {
    if (g.last_move.z < 0.5) {
        return surface;
    }
    let r = length(uv - g.last_move.xy);
    // Dark under the stone itself, peaking right at the stone perimeter
    // (~0.44 cell), and essentially gone by one cell out so the glow hugs a
    // single intersection instead of washing over a 3x3 region.
    let inner = smoothstep(0.0, 0.44, r);
    let outer = smoothstep(1.05, 0.44, r);
    let falloff = inner * outer;
    if (falloff < 1e-4) {
        return surface;
    }

    // Tone-map the falloff so the peak is bright but never clips to white.
    let raw = falloff * GLOW_STRENGTH;
    let weight = raw / (1.0 + raw);

    // A saturated red with roughly the same brightness as the wood underneath,
    // so the glow reads as a red tint rather than a dark overlay.
    let surface_lum = luminance(surface);
    let glow_lum = luminance(GLOW_HUE);
    let glow_target = GLOW_HUE * clamp(surface_lum / glow_lum, 0.0, 5.0);

    return mix(surface, glow_target, weight);
}

/// Sample one glyph cell from the atlas and return (fill, lip).
fn sample_glyph(atlas_uv: vec2<f32>) -> vec2<f32> {
    let sdf = textureSample(glyph_texture, glyph_sampler, atlas_uv).r;
    // fwidth gives the right smoothstep width automatically at any zoom and
    // any mip level.
    let smoothing = max(fwidth(sdf), 1e-4) * 0.65;
    // The generated atlas stores dark glyphs on a light background, so the
    // edge is still 0.5 but "inside" is below it.
    let fill = 1.0 - smoothstep(0.5 - smoothing, 0.5 + smoothing, sdf);

    // The SDF gradient points outward from the glyph.  The mapping from atlas
    // cell to board cell is uniform, so this direction is valid in board space
    // and can be dotted straight onto the light.
    let grad = normalize(vec2<f32>(dpdx(sdf), dpdy(sdf)) + vec2<f32>(1e-6));
    let lit = max(dot(grad, normalize(g.light.xy)), 0.0);
    let lip_width = 0.045;
    // The lip is a narrow band just outside the glyph edge, on the side that
    // faces the light.
    let lip = lit
        * smoothstep(0.5, 0.5 + lip_width * 0.5, sdf)
        * (1.0 - smoothstep(0.5 + lip_width * 0.5, 0.5 + lip_width, sdf));
    return vec2<f32>(fill, lip);
}

/// Engraved coordinate labels: letters A-O on the top and bottom margins,
/// numbers 1-15 on the left and right margins.  Returns (fill, lip) where fill
/// is the dark cut and lip is the light-facing bevel highlight.
fn coordinate_labels(uv: vec2<f32>, texel: f32) -> vec2<f32> {
    if (g.toggles.x < 0.5) {
        return vec2<f32>(0.0);
    }
    var glyph_col = -1;
    var glyph_row = -1;
    // For two-digit numbers: which two digits to compose, x = tens, y = ones.
    var digits = vec2<i32>(-1, -1);
    var local = vec2<f32>(0.0);
    var is_wide = false;
    var margin_x = 0.0;

    // Top margin: letters A-O.
    if (uv.y > -LABEL_MARGIN - LABEL_HALF && uv.y < -LABEL_MARGIN + LABEL_HALF) {
        let col = i32(round(clamp(uv.x, 0.0, LINE_LAST)));
        local = (uv - vec2<f32>(f32(col), -LABEL_MARGIN)) / LABEL_SIZE + 0.5;
        glyph_col = col;
        glyph_row = GLYPH_ROW_LETTERS;
    // Bottom margin: letters A-O.
    } else if (uv.y > 14.0 + LABEL_MARGIN - LABEL_HALF && uv.y < 14.0 + LABEL_MARGIN + LABEL_HALF) {
        let col = i32(round(clamp(uv.x, 0.0, LINE_LAST)));
        local = (uv - vec2<f32>(f32(col), 14.0 + LABEL_MARGIN)) / LABEL_SIZE + 0.5;
        glyph_col = col;
        glyph_row = GLYPH_ROW_LETTERS;
    } else {
        // Left and right margins: numbers 1-15.  Two-digit labels are kerned
        // by placing the digit origins one advance apart rather than one full
        // glyph cell apart, so the pair reads as "12" rather than "1 2".
        let row = i32(round(clamp(uv.y, 0.0, LINE_LAST)));
        let n = 15 - row;
        is_wide = n >= 10;
        let half = select(LABEL_HALF, PAIR_HALF, is_wide);

        local.y = (uv.y - f32(row)) / LABEL_SIZE + 0.5;

        if (uv.x > -LABEL_MARGIN - half && uv.x < -LABEL_MARGIN + half) {
            margin_x = -LABEL_MARGIN;
            if (is_wide) {
                digits = vec2<i32>(1, n - 10);
            } else {
                local.x = (uv.x + LABEL_MARGIN) / LABEL_SIZE + 0.5;
                glyph_col = n;
                glyph_row = GLYPH_ROW_DIGITS;
            }
        } else if (uv.x > 14.0 + LABEL_MARGIN - half && uv.x < 14.0 + LABEL_MARGIN + half) {
            margin_x = 14.0 + LABEL_MARGIN;
            if (is_wide) {
                digits = vec2<i32>(1, n - 10);
            } else {
                local.x = (uv.x - (14.0 + LABEL_MARGIN)) / LABEL_SIZE + 0.5;
                glyph_col = n;
                glyph_row = GLYPH_ROW_DIGITS;
            }
        }
    }

    if (glyph_col >= 0) {
        if (local.x < 0.0 || local.x > 1.0 || local.y < 0.0 || local.y > 1.0) {
            return vec2<f32>(0.0);
        }
        let safe = clamp(local, vec2<f32>(0.005), vec2<f32>(0.995));
        let atlas_uv = vec2<f32>(
            (f32(glyph_col) + safe.x) / GLYPH_ATLAS_COLS,
            (f32(glyph_row) + safe.y) / GLYPH_ATLAS_ROWS,
        );
        return sample_glyph(atlas_uv);
    }

    if (is_wide && digits.x >= 0) {
        // The pair is centred on the margin line; each digit is centred on its
        // own origin, offset by half the advance on either side.
        let is_ones = uv.x >= margin_x;
        let dcol = select(digits.x, digits.y, is_ones);
        let origin_x = margin_x + select(-0.5, 0.5, is_ones) * DIGIT_ADVANCE;
        local.x = (uv.x - origin_x) / LABEL_SIZE + 0.5;
        if (local.x < 0.0 || local.x > 1.0 || local.y < 0.0 || local.y > 1.0) {
            return vec2<f32>(0.0);
        }
        let safe = clamp(local, vec2<f32>(0.005), vec2<f32>(0.995));
        let atlas_uv = vec2<f32>(
            (f32(dcol) + safe.x) / GLYPH_ATLAS_COLS,
            (f32(GLYPH_ROW_DIGITS) + safe.y) / GLYPH_ATLAS_ROWS,
        );
        return sample_glyph(atlas_uv);
    }

    return vec2<f32>(0.0);
}

/// A drilled dimple at each grid crossing.  Returns (darken, rim).
fn dimple(uv: vec2<f32>, texel: f32) -> vec2<f32> {
    let inside = step(0.0, uv.x) * step(uv.x, LINE_LAST) * step(0.0, uv.y) * step(uv.y, LINE_LAST);
    let cell = clamp(round(uv), vec2<f32>(0.0), vec2<f32>(LINE_LAST));
    let r = length(uv - cell);
    // A small drilling: a third of the first drilling's radius, so the dark
    // spot is a mark at the crossing rather than a blob that owns it.
    let radius = clamp(texel * 2.0, 0.053, 0.073);
    // A drilled hole has a sharp rim: the darkening keeps its strength all the
    // way out to the edge and is then cut within one pixel, rather than fading
    // away. One pixel of antialiasing, no more. `floor` is how dark the cup
    // still is at the rim: at one the drilling is as dark and as sharp as the
    // grid lines themselves.
    let floor = select(0.85, g.toggles.y, g.toggles.y > 0.01);
    let aa = max(texel, 1e-4);
    let t = clamp(r / radius, 0.0, 1.0);
    // The bottom of the cup is a little darker than its walls, but the walls
    // stay dark all the way out to the cut.
    let cup = 1.0 - pow(t, 2.0) * (1.0 - floor);
    let cut = 1.0 - smoothstep(radius - aa, radius + aa, r);
    let dark = inside * cup * cut;

    // The light-facing rim of the drilling sits at the outer edge of the
    // dimple, on the side that faces the key light. It ends at the same sharp
    // border as the cup.
    let light_dir = normalize(g.light.xy);
    let rim_centre = cell + light_dir * radius * 0.75;
    let rim_r = length(uv - rim_centre);
    let rim = inside
        * (1.0 - smoothstep(radius * 0.05, radius * 0.35, rim_r))
        * cut;
    return vec2<f32>(dark, rim);
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
    let radial = length((frag.xy / g.frame.xy - centre) * vec2<f32>(1.0, 1.25));
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

    // The last stone gets a soft red glow on the wood around it.
    surface = last_move_glow(uv, surface);

    // Grid lines: black, about two pixels wide at any zoom, and engraved rather
    // than painted. The line is dark; the narrow lighter band beside it is the
    // lip of the cut catching the light.
    let half_width = texel * 1.05;
    let dist = min(abs(uv.x - round(uv.x)), abs(uv.y - round(uv.y)));
    let inside = step(0.0, uv.x) * step(uv.x, LINE_LAST) * step(0.0, uv.y) * step(uv.y, LINE_LAST);
    let line = inside * (1.0 - smoothstep(half_width * 0.55, half_width * 1.25, dist));
    let lip = inside * (1.0 - smoothstep(half_width * 1.3, half_width * 2.6, dist));
    surface = mix(surface, vec3<f32>(0.004, 0.004, 0.005), line);
    surface *= 1.0 + lip * 0.10;

    // Dimples at the crossings, drawn after the grid so they stay visible on
    // the dark intersections.
    let dimple_effect = dimple(uv, texel);
    surface = mix(surface, vec3<f32>(0.004, 0.004, 0.005), dimple_effect.x);
    surface *= 1.0 + dimple_effect.y * 0.38;

    // Coordinate labels engraved into the margins.  The fill is a deep cut
    // and the light-facing bevel is bright so the glyphs punch through the
    // busy wood grain at a glance.
    let label_effect = coordinate_labels(uv, texel);
    surface = mix(surface, vec3<f32>(0.001, 0.001, 0.0015), label_effect.x);
    surface *= 1.0 + label_effect.y * 0.32;

    return vec4<f32>(encode(tone_map(surface)), 1.0);
}
