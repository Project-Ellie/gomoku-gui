// Contact shadows: one soft ellipse per stone, drawn as a darkening quad.
//
// The darkening is strongest where the stone meets the board and fades quickly,
// so it reads as contact rather than as a decal pasted under the stone.

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

    // The stone casts a shadow a little wider than itself, pushed away from the
    // light and squashed along it.
    let radius = 0.62;
    let squash = 0.92;
    // The light comes in at about 46 degrees, and the stone is 0.40 cells tall,
    // so the shadow of its top edge falls about 0.39 cells away. The base sits
    // on the board, so the visible shadow is a blend of the two.
    let offset = normalize(g.light.xy) * 0.18;
    let world = centre + offset
        + vec2<f32>(corner.x * radius, corner.y * radius * squash);

    var out: VsOut;
    out.clip = to_clip(project_board(world));
    out.corner = corner;
    return out;
}

@fragment
fn fs_shadow(in: VsOut) -> @location(0) vec4<f32> {
    // The quad reaches 0.62 cells, and the stone covers 0.47 of that, so the
    // visible part of the shadow is the ring from about 0.75 to 1.0 of the quad
    // radius. The falloff is therefore built to be strongest just outside the
    // stone's footprint and to vanish at the edge of the quad: a falloff that
    // peaks at the centre spends its strength where the stone hides it.
    let r = clamp(length(in.corner), 0.0, 1.0);
    let body = 1.0 - smoothstep(0.50, 1.0, r);
    // Extra weight right at the contact, where a real shadow is darkest.
    let contact = 1.0 - smoothstep(0.72, 0.95, r);
    return vec4<f32>(0.0, 0.0, 0.0, clamp(body * 0.46 + contact * 0.16, 0.0, 0.70));
}
