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

/// The micro-relief of slate: fine, even grain.
fn height_slate(p: vec2<f32>) -> f32 {
    return fbm(p * 11.0, 4) * 0.7 + fbm(p * 42.0, 3) * 0.3;
}

/// The micro-relief of shell: faint streaks along one direction.
fn height_shell(p: vec2<f32>) -> f32 {
    let dir = vec2<f32>(0.866, 0.5);
    let along = dot(p, dir) * 18.0;
    let across = dot(p, vec2<f32>(-dir.y, dir.x)) * 2.4;
    return fbm(vec2<f32>(along, across), 4) * 0.6 + fbm(p * 38.0, 3) * 0.4;
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

    // The stone's micro-texture is finer than a pixel when the board is small on
    // screen. It fades with the pixel footprint so that it never shimmers, which
    // would look like noise rather than like stone.
    let texel = max(length(fwidth(in.local)), 1e-6);
    let detail_frequency = 11.0 * 3.0;
    let sample = smoothstep(0.8, 2.6, 1.0 / max(detail_frequency * texel, 1e-6));
    let detail = mix(1.0, sample, 0.85);

    var albedo = in.colour.rgb;
    var roughness = in.colour.w;
    if (kind > 0.5) {
        // Shell: a faint streak pattern, and a milky body.
        albedo *= 1.0 + (height_shell(p) - 0.5) * 0.07 * detail;
        roughness *= 0.88 + (height_shell(p * 1.6) - 0.5) * 0.22 * detail;
    } else {
        // Slate: fine grain, slightly rougher at the surface.
        albedo *= 1.0 + (height_slate(p) - 0.5) * 0.14 * detail;
        roughness *= 0.88 + (height_slate(p * 1.4) - 0.5) * 0.28 * detail;
    }
    roughness = clamp(roughness, 0.05, 0.6);

    let v = vec3<f32>(0.0, 0.0, 1.0);
    let l = key_light();
    let strength = select(0.16, 0.08, kind > 0.5) * detail;
    let n = perturb(normalize(in.normal), p, kind, strength);

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
    var shine = vec3<f32>(tight * 1.15 + wide * 0.45);

    // The room, reflected. This is the term that makes a stone look glossy.
    let reflection = environment(reflect(-v, n), roughness);
    let weight = fresnel(n_dot_v, 0.05);
    shine += reflection * weight * 0.55;

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
        // Slate: a cool, narrow rim that separates it from the board, and a
        // slightly warm highlight.
        let rim = pow(1.0 - n_dot_v, 3.5);
        diffuse += vec3<f32>(0.10, 0.11, 0.13) * rim;
        shine *= vec3<f32>(1.0, 0.95, 0.88);
    }

    // Contact darkening at the base, so the stone sits on the board.
    let radius = length(in.local);
    let contact = smoothstep(0.44, 0.30, radius);
    var colour = diffuse * mix(0.65, 1.0, contact) + shine;

    if (kind > 0.5) {
        let edge = smoothstep(0.36, 0.47, radius);
        colour *= 1.0 - edge * 0.06;
    }

    return vec4<f32>(tone_map(colour), 1.0);
}
