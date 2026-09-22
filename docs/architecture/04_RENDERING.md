# 04 — Rendering

## Coordinate systems

| Space | Unit | Origin | Use |
|---|---|---|---|
| Board space | 1 cell | `[0,0]` is the top-left intersection, y grows downward | All geometry, textures, hit testing |
| Screen space | 1 physical pixel | Top-left of the window | Input events, egui overlays |

Board space is the anchor. A point at board `(3.0, 5.0)` sits on the
intersection of column 3 and row 5, and the wood grain is fixed to that plane.
The grain therefore belongs to the board and does not slide when the view moves
or zooms.

Geometry sizes in board units:

| Item | Value (cells) |
|---|---|
| Cell pitch | 1.0 |
| Intersections | integers `0..=14` in both axes |
| Slab half extent | 7.85, so the slab spans `-0.85..=14.85` |
| Border from the outer line to the slab edge | 0.85 |
| Slab corner radius | 0.25 |
| Slab bevel width | 0.22 |
| Stone base radius | 0.40 |
| Stone equator radius | 0.47 |
| Stone apex height | 0.175 |

Stones never overlap: two stones are at least 1.0 cell apart and their widest
radius is 0.47 cell. There is no depth buffer, and the stone pass order is
irrelevant.

## Camera

The camera is an orthographic top-down view. The uniform is three values.

```wgsl
struct View {
    center: vec2<f32>,        // board space, in cells
    pixels_per_cell: f32,
    window: vec2<u32>,        // physical pixels
    flipped: u32,
}
```

Screen position of a board point:

```
d = board_pos - center
if flipped: d = -d
screen = window * 0.5 + d * pixels_per_cell
```

The inverse is the same expression solved for `board_pos`.

Derived values:

- `fit = min(window.x, window.y) / 16.3`, the scale at which the whole slab
  plus a small margin is visible.
- Zoom range `[fit, fit * 40]`. At `fit` the board fills the window. At
  `fit * 40` one cell spans about 2.4 times the smaller window dimension, which
  exposes full grain detail.
- A zoom keeps the board point under the pointer fixed:
  `center' = p + (center - p) * (ppc / ppc')`.
- Pan clamp. Let `half = window / (2 * ppc)` be the visible half extent in
  cells. On each axis:
  - if `half >= 8.15`, the whole board fits. The centre snaps to `7.0`.
  - otherwise the centre is clamped to `[7.0 - (8.15 - half), 7.0 + (8.15 - half)]`,
    so the visible rectangle never moves more than `half` beyond the slab edge.

These rules are pure functions with unit tests. `fit` depends on the window
size, so a window resize recomputes the clamp and re-clamps the centre. A
resize never changes `pixels_per_cell` except through the clamp, so a resized
window does not lose the zoom level.

## Passes and targets

| # | Pass | Target | Contents |
|---|---|---|---|
| 1 | Board | MSAA 4x colour | Background, slab, wood, grid |
| 2 | Shadow | Same MSAA target | One instanced quad per stone, darkening blend |
| 3 | Stone | Same MSAA target | One instanced lens per stone |
| 4 | Resolve | Surface texture | MSAA resolve |
| 5 | egui | Surface texture, load | Chrome, text, markers, dialogs |

Surface format: a format from the surface capabilities where `is_srgb()` is
true, which is the usual case on macOS. Alpha mode from the capabilities,
preferring `Opaque`. Present mode `Fifo`.

No depth attachment. No stencil. One MSAA colour texture, recreated on resize.

## Board pass

One full-screen triangle. The fragment shader computes everything from board
space, so the pass has no vertices to speak of and no texture binding.

### Slab and background

A rounded-rectangle distance field `d` in board space gives the slab, the
bevel, and the anti-aliased edge from one expression. With
`s = 1 / pixels_per_cell` as one pixel in board units:

1. `d > 0` is outside. The pixel is background.
2. `-0.22 < d <= 0` is the bevel. The height rises linearly and the shading
   uses the bevel slope, so the light-facing side is brighter and the opposite
   side is darker. A thin dark line at `d` near 0 separates the board from the
   background.
3. `d <= -0.22` is the top face.

The background is a dark neutral vertical gradient. A small hash-based dither
of one bit per channel removes banding.

Outside the slab, the shadow term `shadow = 0.35 * smoothstep(0.55, 0.0, d_offset)`
darkens the background, where `d_offset` is the distance field evaluated at a
point shifted against the light direction. That gives the slab a soft drop
shadow without a second pass.

### Wood

`uv` is board space. The grain follows the light direction's cross axis, so
the material is rotated to match the light rather than the screen.

```
grain = vec2(uv.x * 0.55, uv.y * 2.60)      // features stretched along x
warp  = fbm(grain * 0.35)                    // slow distortion of the rings
rings = fract(grain.x * 1.35 + warp * 1.9)   // ring bands
ring_profile = pow(abs(2.0 * rings - 1.0), 0.55)
pores = smoothstep(0.63, 0.79, fbm(grain * 6.5))
```

Then:

```
body   = mix(preset.dark, preset.light, 1.0 - ring_profile * preset.ring_contrast)
albedo = mix(body, preset.pore, pores * preset.pore_depth)
height = ring_profile * 0.35 - pores * 0.55     // for the normal
```

Normal perturbation comes from the analytic derivative of the height field,
scaled so the grain reads as a surface, not as a painted stripe.

Lighting is a two-lobe anisotropic GGX with the tangent along the grain:

- roughness along the grain `0.28`, across the grain `0.55`,
- a narrow lobe for the sheen and a wide lobe for the diffuse falloff,
- a grazing-angle sheen term (`fresnel * 0.35`) that only appears along the
  ring crests. This term is what makes the board read as varnished wood.

Value ranges are per preset. See the preset table below.

### Detail without aliasing at any zoom

Every noise octave carries a fade factor from the screen-space derivative:

```
fade(octave) = 1 - smoothstep(0.45, 1.0, fwidth(grain) * exp2(octave))
```

An octave disappears when one screen pixel covers more than about one
feature. Zooming out fades the fine octaves, so the board never shimmers.
Zooming in brings them back, so the board never goes soft. The pore layer uses
the same rule.

This is the reason the textures are procedural instead of a baked image: a
fixed-resolution image would be blurry at the maximum zoom of `fit * 40`, and
a texture large enough to fix that would be hundreds of megabytes.

### Grid lines

Lines are computed, not drawn as geometry, so they stay one pixel wide at any
zoom:

```
p = fwidth(uv) * 1.15                        // line width in uv units
dist_x = abs(uv.x - round(uv.x))
dist_y = abs(uv.y - round(uv.y))
inside = step(0.0, uv.x) * step(uv.x, 14.0) * step(0.0, uv.y) * step(uv.y, 14.0)
coverage = inside * (1.0 - smoothstep(p * 0.5, p * 1.2, min(dist_x, dist_y)))
wood = mix(wood, line_color, coverage * 0.92)
```

A faint darkening at the intersections is an accepted artifact of the distance
field and reads as a real board. Optional star points are not drawn; a 15x15
gomoku board has none.

### Board materials

| Preset | Light | Dark | Pores | Roughness (along / across) | Sheen | Kind |
|---|---|---|---|---|---|---|
| `aged_wood` (default) | `#b8823f` | `#6f4520` | `#3a2410` | 0.28 / 0.55 | 0.35 | wood |
| `walnut` | `#6b4526` | `#3a2113` | `#1d0f06` | 0.26 / 0.50 | 0.40 | wood |
| `dark_marble` | `#26272e` | `#121317` | veins `#ccd2dd` | 0.14 / 0.18 | 0.55 | marble |
| `green_marble` | `#365a4d` | `#1c352c` | veins `#cfe4d8` | 0.15 / 0.20 | 0.50 | marble |

The marble presets reuse the same noise with `kind = marble`, which replaces
the ring bands with sparse high-contrast veins:
`vein = smoothstep(0.72, 0.78, 1.0 - abs(2.0 * fbm(grain * 0.9) - 1.0))`.

The owner asked for a board a little darker than kaya. `aged_wood` is that
board: a warm medium brown, darker than the `#e0c088`-ish tone of new kaya,
with visible pores and a sheen.

Colours in the table are sRGB and are converted to linear at load time. The
shader works in linear values and the surface format is sRGB, so the
conversion happens once, in the right place.

## Stone pass

### Mesh

One lens mesh, generated once on the CPU with a lathe over a profile:

```
r(0.000) = 0.40     y = 0.000      base
r(0.085) = 0.47     y = 0.085      equator
r(0.140) = 0.36     y = 0.150
r(0.000) = 0.00     y = 0.175      apex
```

The profile is sampled at 12 rings with a smooth interpolation between the
control points, and revolved with 48 segments, which gives about 1100 triangles
and a silhouette that stays smooth at the maximum zoom. Normals come from the
profile tangent, so the equator highlight is correct.

### Instances

| Attribute | Type | Meaning |
|---|---|---|
| `center` | `vec2<f32>` | Board-space position of the intersection |
| `kind` | `u32` | 0 = dark, 1 = white |
| `seed` | `f32` | Per-stone variation of the texture |
| `albedo` | `vec3<f32>` | Linear albedo |
| `roughness` | `f32` | Base roughness |

Two `vec4` attributes, 32 bytes per instance, 225 instances, one draw call.

### Dark stone (slate)

- Albedo `(0.055, 0.058, 0.068)` linear, so it is dark grey-blue, never pure
  black.
- Granular noise at medium frequency perturbs the normal and varies the
  roughness by `±0.12`, which gives the matte, slightly rough surface of slate.
- Sparse veins: a low-frequency ridged noise adds a slightly lighter, cooler
  streak. Subtle, high contrast is wrong for slate.
- Two-lobe specular with `F0 = 0.04` and a warm tint `(1.0, 0.94, 0.86)` on the
  narrow lobe. The sheen is weak but present, which is how slate behaves.
- A rim term brightens the outer 0.1 cell slightly, so the stone separates from
  the board when both are dark.

### White stone (shell)

- Albedo `(0.90, 0.92, 0.955)` linear, a cool milky white, never flat grey.
- Streaks: an anisotropic noise band at about 30 degrees to the stone axis,
  low contrast, plus fine granular noise. This is the shell structure.
- Roughness `0.19` with a streak-driven variation, so the highlight is tighter
  and longer than the slate highlight.
- A fake subsurface term: `sss = pow(saturate(1.0 - dot(-n, v)), 2.0) * 0.18`
  added with a cool tint. Light appears to pass through the rim, which is the
  signature of a real shell stone.
- `F0 = 0.05`.

### Both

- Contact darkening near the base: the last 0.05 cell of the radius is
  multiplied by `0.85`, which grounds the stone on the board.
- The noise offset is seeded per stone, so 225 identical stones do not look
  like one stamp. Variation is small, about 5 degrees of streak rotation and a
  small noise offset.
- The same `fwidth` fade rule applies, so the fine stone texture never aliases.

## Shadow pass

One quad per stone on the board plane:

- half extent 0.72 cell, squashed by 0.86 along the light axis,
- centre offset by `light.xy * 0.22` cell,
- alpha `= (1 - r)^2 * 0.42`, where `r` is the elliptical distance,
- blend state `BlendFactor::Zero` as the source factor and
  `BlendFactor::OneMinusSrcAlpha` as the destination factor, which only
darkens,
- no depth test, no MSAA sample shading needed.

A soft, slightly offset contact shadow is what makes the stones look placed on
the board instead of pasted onto it.

## Lighting

One directional key light and one ambient term, both fixed in board space:

```
key     = normalize(vec3(-0.42, -0.55, 0.72))
ambient = 0.22
fill    = 0.10 * max(0.0, dot(n, normalize(vec3(0.5, 0.9, 0.4))))
```

A fixed light means the shadows do not move when the view moves, which is what
the eye expects from a board on a table.

## WGSL layout

| File | Contents |
|---|---|
| `noise.wgsl` | Hash, gradient noise, `fbm`, `fbm_deriv`, octave fade helper |
| `board.wgsl` | Board entry points: vertex and fragment |
| `stone.wgsl` | Stone entry points and the slate and shell materials |
| `shadow.wgsl` | Shadow quad entry points |

WGSL has no include mechanism. The modules are concatenated in Rust:

```rust
const BOARD_WGSL: &str = concat!(
    include_str!("wgsl/noise.wgsl"), "\n",
    include_str!("wgsl/board.wgsl"));
```

A test parses every concatenated string with `naga`, so a shader syntax error
fails `cargo test` without a GPU.

## Overlays

Text and markers are drawn by egui in pass 5, not by our shaders. Positions are
projected from board space to screen space with the camera transform, so the
coordinates, the move numbers, the last-move ring, and the winning line stay
locked to the board.

Text size scales with `pixels_per_cell` and is clamped to `11..=44` physical
pixels, so labels stay legible when zoomed out and do not become absurd when
zoomed in.

| Overlay | Position | Style |
|---|---|---|
| Coordinates | 0.62 cell outside each edge, at every fifth intersection and both ends | Monospace, muted foreground, drawn on the slab |
| Move numbers | Intersection centre | Monospace, colour that contrasts with the stone |
| Last-move ring | Intersection centre, radius 0.40 cell | 2 px ring in a high-contrast accent |
| Winning line | From the first to the last stone of the line | 5 px line at 55 percent alpha, plus a ring on each stone |

## Materials tuning panel

A collapsible egui window under View, off by default, exposes the preset
values as sliders: grain scale, ring contrast, pore depth, both roughness
values, sheen, stone roughness, vein strength, light azimuth and elevation.
The panel writes into the same `BoardMaterial` and `StoneMaterial` structures
the presets fill, so tuning and presets are the same code path. A "Print
values" button writes the current values to the log in the preset table format,
which is how the tuned look is frozen.

The panel exists because a procedural material is judged by eye. Polishing it
without a live control would take many release cycles.

## Performance

| Cost | Value |
|---|---|
| Draw calls per frame | 4, plus the egui batches |
| Instance count | 225 stones, 225 shadow quads |
| Triangles | About 1100 per stone, about 250k for a full board |
| Texture memory | None for the board and stones |
| Target | Under 2 ms of GPU work per frame, comfortably inside a 120 Hz frame |

The board pass is a full-screen pass with a few dozen noise fetches, which is
the dominant cost and is still cheap. No optimisation pass is planned before
the look is right.
