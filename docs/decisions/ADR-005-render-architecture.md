# ADR-005: Custom board and stone passes, an egui overlay, and shader-generated materials

## Status

Accepted (owner-approved, 2026-09-22)

## Context

The visual requirement is a board that reads as real wood, and stones that read
as slate and shell. Three questions had to be settled:

1. How much of the frame is ours and how much belongs to the user-interface
   toolkit?
2. How do the board and stone textures get their detail?
3. How are text, coordinates, and markers drawn?

Options for textures: a baked image, a texture generated on the CPU at
start-up, or a procedural material in the shader. A baked image of sufficient
quality to survive the maximum zoom would need a very large texture. A CPU
texture has the same resolution limit. A procedural material has no resolution
limit at all.

Options for the chrome: hand-written panels, text, and hit testing, or an
overlay toolkit. Hand-rolling a toolkit is a large amount of fragile code with
no benefit for this application.

## Decision

1. The board and the stones are our own `wgpu` passes: one full-screen pass for
   the background, the slab, the wood, and the grid, then one instanced pass
   for the contact shadows, then one instanced pass for the stones.
2. `egui` draws everything else on the same device and in the same surface: the
   top bar, the side panel, the coordinates, the move numbers, the last-move
   ring, the winning line, the banners, and the dialogs.
3. Wood and stone surfaces are computed in the shader from board-space
   coordinates. Every noise octave carries a fade factor derived from
   `fwidth`, so fine detail appears as the view zooms in and disappears before
   it can alias as the view zooms out. Grid lines are computed and
   anti-aliased in the shader instead of being drawn as geometry, so they stay
   one pixel wide at every zoom.
4. Board and stone materials are value presets behind one shader, selected by a
   `kind` uniform. Four board presets ship: `aged_wood` (the default, a little
   darker than kaya), `walnut`, `dark_marble`, and `green_marble`.
5. A tuning panel under the View menu exposes the material parameters as
   sliders, so the look is dialled in by eye and then frozen into a preset.

## Consequences

- The board keeps full detail at every zoom level, with no texture memory and
  no asset files to ship or lose.
- Stones never overlap, because two stones are at least 1.0 cell apart and the
  widest radius is 0.47 cell. There is no depth buffer, and the pass order does
  not matter.
- Text is crisp and free, because egui renders it at device resolution, and
  egui's embedded fonts mean no font asset is needed either.
- Overlays are projected from board space, so they stay attached to the board
  under zoom, pan, and flip.
- The version coupling between `egui-wgpu` and `wgpu` becomes a maintenance
  constraint, recorded in ADR-001.
- The material is a model that is judged by eye. The tuning panel is the
  mitigation, and it is part of the product, not a temporary debugging tool.
