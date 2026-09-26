// Stones: a lens mesh per stone, drawn instanced, with slate and shell
// materials.
//
// A stone is a smooth, slightly translucent solid. What sells it is a coherent
// specular highlight, a reflection of the room, and a bright rim where light
// passes through the stone. Roughness is therefore small and the surface
// perturbation is gentle: a broken highlight reads as plastic, not as stone.

struct MeshIn {
    // Local lens position in cells. z is the height above the board.
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // Per-instance data.
    @location(2) centre: vec2<f32>,
    @location(3) colour: vec4<f32>,
    @location(4) params: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) local: vec2<f32>,
    @location(2) colour: vec4<f32>,
    @location(3) params: vec4<f32>,
};

@vertex
fn vs_stone(in: MeshIn) -> VsOut {
    let world = vec2<f32>(in.position.xy + in.centre);
    var out: VsOut;
    out.clip = to_clip(project_board(world));
    out.normal = in.normal;
    out.local = in.position.xy;
    out.colour = in.colour;
    out.params = in.params;
    return out;
}

/// The relief of slate: coarse granular grain, the speckle a polished slate
/// shows at arm's length. A stone is under a cell across, so the features have to
/// be a good fraction of that, or they average away to nothing.
fn height_slate(p: vec2<f32>) -> f32 {
    return fbm(p * 2.0, 4) * 0.62 + fbm(p * 5.5, 3) * 0.38;
}

/// The relief of shell: soft streaks along one direction, over fine speckle.
fn height_shell(p: vec2<f32>) -> f32 {
    let dir = vec2<f32>(0.866, 0.5);
    let along = dot(p, dir) * 9.0;
    let across = dot(p, vec2<f32>(-dir.y, dir.x)) * 1.6;
    return fbm(vec2<f32>(along, across), 4) * 0.66 + fbm(p * 5.0, 3) * 0.34;
}

/// A finer layer, for when the stone is large on screen. It fades out when its
/// features would be smaller than a pixel, so it never shimmers.
fn fino_slate(p: vec2<f32>) -> f32 {
    return fbm(p * 30.0, 3);
}

fn fino_shell(p: vec2<f32>) -> f32 {
    return fbm(p * 26.0, 3);
}

/// Perturb the mesh normal with a shallow height field.
fn perturb(base: vec3<f32>, p: vec2<f32>, kind: f32, strength: f32) -> vec3<f32> {
    let e = 0.03;
    var h0 = height_slate(p);
    var hx = height_slate(p + vec2<f32>(e, 0.0));
    var hy = height_slate(p + vec2<f32>(0.0, e));
    if (kind > 0.5) {
        h0 = height_shell(p);
        hx = height_shell(p + vec2<f32>(e, 0.0));
        hy = height_shell(p + vec2<f32>(0.0, e));
    }
    let slope = vec3<f32>(-(hx - h0) / e, -(hy - h0) / e, 0.0) * strength;
    return normalize(base + slope);
}

@fragment
fn fs_stone(in: VsOut) -> @location(0) vec4<f32> {
    let kind = in.params.y;
    let seed = in.params.x;
    let p = in.local * 3.0 + vec2<f32>(seed * 13.7, seed * 5.3);

    // Two layers of texture. The coarse one is always visible, because it is the
    // speckle a stone shows at arm's length. The fine one appears only when the
    // stone is large on screen, and fades out when its features would be smaller
    // than a pixel, so it never shimmers.
    let texel = max(length(fwidth(in.local)), 1e-6);
    let coarse = smoothstep(0.9, 2.4, 1.0 / max(6.0 * texel, 1e-6));
    let fine = smoothstep(1.2, 3.0, 1.0 / max(30.0 * texel, 1e-6));
    let detail = mix(1.0, coarse, 0.8);
    let grain_detail = mix(1.0, fine, 0.85);

    // The look dials of the stone's own material: how far the texture
    // modulates the surface, and how the body is darkened.
    let knobs = mix(g.slate_knobs, g.shell_knobs, vec4<f32>(kind));
    let contrast = knobs.y;
    // How dull the stone may go: where the highlight stops spreading. A low cap
    // keeps a tight sparkle; a high one lets the light smear into a soft sheen.
    let cap = select(0.45, in.params.z, in.params.z > 0.01);

    var albedo = in.colour.rgb;
    var roughness = in.colour.w;
    if (kind > 0.5) {
        // Shell: a milky body with soft streaks and speckle, as a real shell stone
        // has. The streaks vary the roughness, which is what breaks the highlight
        // into the wide soft band a polished stone shows.
        let streaks = height_shell(p);
        albedo *= mix(1.0, 0.88 + streaks * 0.22 * detail, contrast);
        albedo *= mix(1.0, 0.95 + fino_shell(p) * 0.10 * grain_detail, contrast);
        roughness = clamp(roughness * mix(1.0, 0.70 + streaks * 0.60 * detail, contrast), 0.05, cap);
    } else {
        // Slate: coarse speckle over a darker body, with faint grey veining. The
        // veining is ridged noise, so it forms thin lines rather than blobs, and
        // the speckle varies the roughness, which is what makes polished slate
        // glitter rather than shine.
        let speckle = height_slate(p);
        let vein = 1.0 - abs(2.0 * fbm(p * 1.1, 3) - 1.0);
        albedo *= mix(1.0, 0.74 + speckle * 0.40 * detail, contrast);
        albedo *= mix(1.0, 0.96 + fino_slate(p) * 0.09 * grain_detail, contrast);
        albedo *= mix(1.0, 1.0 - vein * 0.16 * detail, contrast);
        roughness = clamp(roughness * mix(1.0, 0.62 + speckle * 0.85 * detail, contrast), 0.05, cap);
    }
    albedo *= knobs.z;

    let v = vec3<f32>(0.0, 0.0, 1.0);
    let l = key_light();
    let strength = select(0.16, 0.08, kind > 0.5) * detail;
    var n = perturb(normalize(in.normal), p, kind, strength);

    // The tooth of the surface: white noise at the pixel scale, one cell per
    // screen pixel, so the stone is not glassy smooth. No structure is added
    // that the eye could follow — the cells are too small to read — but the
    // highlight and the sheen break up on the smallest scale, which is what a
    // honed stone shows.
    let grain = g.toggles.z;
    if (grain > 0.001) {
        let pixel = floor(in.local / texel + vec2<f32>(seed * 31.7, seed * 17.3));
        let w1 = hash21(pixel);
        let w2 = hash21(pixel + vec2<f32>(41.0, 13.0));
        n = normalize(n + vec3<f32>(w1 - 0.5, w2 - 0.5, 0.0) * grain * 0.45);
        roughness = clamp(roughness * mix(1.0, 0.72 + w1 * 0.56, grain), 0.05, cap);
    }

    let n_dot_l = max(dot(n, l), 0.0);
    let n_dot_v = max(dot(n, v), 1e-4);

    // Diffuse with a little wrap, so the shaded side is lit by the room rather
    // than turning black. The wrap is kept small: a stone must show that it is a
    // solid, and a wrap that is too generous flattens it into a disc.
    var diffuse = albedo * (0.16 + 0.84 * (n_dot_l * 0.5 + 0.5));

    // Two specular lobes. The wide one is the varnish-like sheen of a polished
    // stone; the tight one is the reflection of the light itself.
    let wide = specular(n, v, l, min(roughness * 1.6, 0.7), 0.045);
    let tight = specular(n, v, l, roughness * 0.55, 0.055);
    // The gloss of the material decides how much of the light is reflected at
    // all. Slate scatters, so it takes a small share.
    let gloss = select(g.wood_b.x, g.wood_b.y, kind > 0.5);
    var shine = vec3<f32>(tight * 1.55 + wide * 0.50) * gloss * knobs.x;

    // The room, reflected. This is the term that makes a stone look glossy.
    let reflection = environment(reflect(-v, n), roughness);
    let weight = fresnel(n_dot_v, 0.05);
    shine += reflection * weight * 0.80 * gloss * knobs.w;

    if (kind > 0.5) {
        // Shell: light that has travelled through the stone and comes out at the
        // rim. Cool and soft, and it is why a real shell stone glows. It is kept
        // well below the point where the body clips to pure white, which is what
        // makes a white stone look like paper.
        let rim = pow(1.0 - n_dot_v, 2.6);
        let thickness = mix(0.35, 1.0, n_dot_l);
        diffuse += vec3<f32>(0.18, 0.21, 0.26) * rim * thickness * 0.50;
        shine *= vec3<f32>(0.92, 0.97, 1.0);
    } else {
        // Slate: only the faintest separation from the board, and a warm, weak
        // highlight. A bright ring here is what makes a dark stone look like a
        // washer, so there is almost none.
        let rim = pow(1.0 - n_dot_v, 4.0);
        diffuse += vec3<f32>(0.020, 0.022, 0.026) * rim;
        shine *= vec3<f32>(1.0, 0.95, 0.88);
    }

    // Contact darkening at the base, so the stone sits on the board. It is spread
    // over most of the stone rather than gathered at the edge: a narrow dark band
    // beside a lighter one reads as a ring.
    let radius = length(in.local);
    let contact = smoothstep(0.50, 0.22, radius);
    var colour = diffuse * mix(0.80, 1.0, contact) + shine;

    if (kind > 0.5) {
        let edge = smoothstep(0.36, 0.47, radius);
        colour *= 1.0 - edge * 0.06;
    }

    // The soft edge. A thing that rounds away from the eye does not end in a
    // hard line: the last sliver lets the board show through, as a photograph
    // blurs the silhouette of a stone. The feather is measured in pixels, not
    // in the turn of the surface: the lentil's rim is so steep that the turn
    // runs its course within a pixel or two, and a turn-based feather would
    // never be seen. 0.47 is the radius of the lens mesh at its widest.
    let feather = texel * (0.5 + g.toggles.w * 5.0);
    let alpha = select(
        1.0,
        smoothstep(0.47, 0.47 - feather, radius),
        g.toggles.w > 0.001
    );

    return vec4<f32>(encode(tone_map(colour)), alpha);
}
