// Stones: a lens mesh per stone, drawn instanced, with slate and shell
// materials.

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

fn height_slate(p: vec2<f32>) -> f32 {
    return fbm(p * 9.0, 4) * 0.65 + fbm(p * 34.0, 3) * 0.35;
}

fn height_shell(p: vec2<f32>) -> f32 {
    // Fine streaks along one direction: the structure of a shell stone.
    let dir = vec2<f32>(0.866, 0.5);
    let along = dot(p, dir) * 24.0;
    let across = dot(p, vec2<f32>(-dir.y, dir.x)) * 3.0;
    return fbm(vec2<f32>(along, across), 4) * 0.72 + fbm(p * 44.0, 3) * 0.28;
}

/// Perturb the mesh normal with the surface height field of the material.
fn perturb(base: vec3<f32>, p: vec2<f32>, kind: f32) -> vec3<f32> {
    let e = 0.02;
    var h0 = height_slate(p);
    var hx = height_slate(p + vec2<f32>(e, 0.0));
    var hy = height_slate(p + vec2<f32>(0.0, e));
    if (kind > 0.5) {
        h0 = height_shell(p);
        hx = height_shell(p + vec2<f32>(e, 0.0));
        hy = height_shell(p + vec2<f32>(0.0, e));
    }
    var strength = 0.45;
    if (kind > 0.5) {
        strength = 0.22;
    }
    let slope = vec3<f32>(-(hx - h0) / e, -(hy - h0) / e, 0.0) * strength;
    return normalize(base + slope);
}

@fragment
fn fs_stone(in: VsOut) -> @location(0) vec4<f32> {
    let kind = in.params.y;
    let seed = in.params.x;
    let p = in.local * 3.4 + vec2<f32>(seed * 13.7, seed * 5.3);

    let n = perturb(normalize(in.normal), p, kind);
    let l = normalize(g.light.xyz);
    let v = vec3<f32>(0.0, 0.0, 1.0);
    let ndl = max(dot(n, l), 0.0);

    var albedo = in.colour.rgb;
    var roughness = in.colour.w;

    // Fine surface variation in the shading itself.
    if (kind > 0.5) {
        albedo *= 0.94 + height_shell(p) * 0.10;
        roughness *= 0.85 + height_shell(p * 1.7) * 0.30;
    } else {
        albedo *= 0.90 + height_slate(p) * 0.16;
        roughness *= 0.85 + height_slate(p * 1.5) * 0.35;
    }

    let h = normalize(l + v);
    let ndh = max(dot(n, h), 0.0);
    let power = 2.0 / max(roughness * roughness, 1e-3) - 2.0;
    let spec = pow(ndh, power) * (1.0 - roughness);

    // Ambient occlusion at the base, so the stone sits on the board.
    let radius = length(in.local);
    let contact = smoothstep(0.47, 0.34, radius);

    var colour = albedo * (0.34 + 0.66 * ndl) * mix(0.72, 1.0, contact);

    if (kind > 0.5) {
        // Shell: a cool, tight highlight and light bleeding through the rim.
        colour += vec3<f32>(0.88, 0.94, 1.0) * spec * 0.75;
        let rim = pow(1.0 - max(dot(n, v), 0.0), 3.0);
        colour += vec3<f32>(0.30, 0.38, 0.46) * rim * 0.45;
    } else {
        // Slate: a weak, warm highlight.
        colour += vec3<f32>(1.0, 0.93, 0.84) * spec * 0.22;
        let rim = pow(1.0 - max(dot(n, v), 0.0), 4.0);
        colour += vec3<f32>(0.16, 0.17, 0.20) * rim * 0.9;
    }

    return vec4<f32>(colour, 1.0);
}
