// Contact shadows: one soft ellipse per stone, drawn as a darkening quad.

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) corner: vec2<f32>,
};

/// A quad per instance, built from the vertex index. Only the instance centre
/// is read from the instance buffer.
@vertex
fn vs_shadow(
    @builtin(vertex_index) index: u32,
    @location(2) centre: vec2<f32>,
) -> VsOut {
    var corner = vec2<f32>(-1.0, -1.0);
    if (index == 1u) { corner = vec2<f32>(1.0, -1.0); }
    if (index == 2u) { corner = vec2<f32>(-1.0, 1.0); }
    if (index == 3u) { corner = vec2<f32>(1.0, -1.0); }
    if (index == 4u) { corner = vec2<f32>(1.0, 1.0); }
    if (index == 5u) { corner = vec2<f32>(-1.0, 1.0); }

    let radius = 0.80;
    let across = 0.88;
    let offset = normalize(g.light.xy) * 0.16;
    let world = centre + offset
        + vec2<f32>(corner.x * radius, corner.y * radius * across);

    var out: VsOut;
    out.clip = to_clip(project_board(world));
    out.corner = corner;
    return out;
}

@fragment
fn fs_shadow(in: VsOut) -> @location(0) vec4<f32> {
    // A soft elliptical falloff. The blend state turns the alpha into
    // darkening, so the colour is irrelevant.
    let r = length(in.corner);
    let alpha = pow(1.0 - clamp(r, 0.0, 1.0), 2.2) * 0.50;
    return vec4<f32>(0.0, 0.0, 0.0, alpha);
}
