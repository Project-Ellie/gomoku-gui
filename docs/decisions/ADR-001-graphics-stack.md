# ADR-001: Graphics stack — wgpu 30 with winit 0.30 and hand-written WGSL

## Status

Accepted (owner-approved, 2026-09-22)

## Context

The owner asked for a board that looks like real wood and stones that look like
real slate and shell. That needs per-pixel material work: procedural grain,
anisotropic specular response, normal perturbation, and soft contact shadows.

Four candidate stacks were compared:

| Stack | Assessment |
|---|---|
| `wgpu` with `winit` and hand-written WGSL | Full control of every pixel. No asset pipeline needed. Most code to write. |
| `iced` 0.14 | Declarative, uses wgpu underneath, but custom shaders and per-pixel material maths fight the widget model. |
| `egui` or `eframe` alone | Fastest to build, weakest visuals. Immediate-mode painting cannot do grain and stone specularity convincingly. |
| `bevy` | Physically based rendering out of the box, but a large dependency and an entity-component framework that is heavy for a two-player board game with a fixed camera. |

## Decision

Use `wgpu` 30 with `winit` 0.30 and hand-written WGSL for the board and the
stones. Use `egui` 0.36 with `egui-wgpu` and `egui-winit` for the menus,
panels, text, and markers, drawn as an overlay pass on the same device.

Board and stone textures are generated in the shader from board-space
coordinates. No image asset is loaded.

## Consequences

- The visual requirements are met without a compromise in the material model.
- The framework version coupling becomes a real constraint: `egui-wgpu` must
  share the `wgpu` major version with our own passes. This was verified as
  `wgpu ^30.0` and `winit ^0.30.13` for `egui-wgpu` 0.36.2. Every dependency
  bump is checked with `cargo tree -d`.
- We own the pipeline, the bind groups, and the window handling, which is more
  code than a toolkit would need.
- Procedural textures mean no aliasing at the maximum zoom and no texture
  memory, but the material is judged by eye and needs an iteration cycle. The
  tuning panel covers that.
