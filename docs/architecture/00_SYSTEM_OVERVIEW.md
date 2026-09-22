# 00 — System Overview

## What the program is

A native desktop program for two players at one screen. It plays freestyle
Gomoku on a 15x15 board, records the game in a file, and shows a wooden board
with stone pieces. It has no server and no network use.

## Component map

```
        ┌──────────────────────────────────────────────┐
        │                 gomoku-gui                   │
        │          (binary, edition 2024)              │
        │                                              │
        │  winit 0.30 ── input ──┐                     │
        │                        ▼                     │
        │  camera ──► render (wgpu 30) ──► surface     │
        │                        │                     │
        │  egui 0.36 overlay ────┘                     │
        │                                              │
        │  audio (cpal 0.18)      dialogs (rfd 0.17)   │
        │  config (TOML)          autosave             │
        └───────────────────┬──────────────────────────┘
                            │ calls
                            ▼
        ┌──────────────────────────────────────────────┐
        │               gomoku-core                    │
        │  Game   — move list, cursor, board           │
        │  record — versioned JSON, validation         │
        │  config — settings schema                    │
        │  notation — A-O / 1-15 display labels        │
        └───────────────────┬──────────────────────────┘
                            │ calls
                            ▼
        ┌──────────────────────────────────────────────┐
        │  ../rust-ml/gomoku/crates/engine (external)  │
        │  Board, Move, Color, Status — rules only     │
        └──────────────────────────────────────────────┘
```

The arrow direction is the dependency direction. Nothing in a lower box knows
about a box above it.

## Dependency policy

| Crate | Purpose | Version |
|---|---|---|
| `wgpu` | Graphics device, pipelines, shaders | 30.0 |
| `winit` | Window, events, input | 0.30.13 |
| `egui` + `egui-wgpu` + `egui-winit` | Menus, panels, text, markers | 0.36.2 |
| `rfd` | Native open and save dialogs | 0.17 |
| `cpal` | Audio output | 0.18 |
| `serde` + `serde_json` | Record format | 1 |
| `time` | Creation timestamp, RFC 3339 | 0.3 |
| `toml` | Config file | 1.1 |
| `thiserror` | Typed errors in core | 2 |
| `anyhow` | Error context in the binary | 1 |
| `log` + `env_logger` | Diagnostics | 0.4 / 0.11 |
| `bytemuck` | Vertex and uniform casts | 1 |
| `glam` | Vector and matrix maths | 0.33 |
| `directories` | App data and config paths | 6 |
| `engine` | Gomoku rules | path `../rust-ml/gomoku/crates/engine` |
| `proptest` | Property tests (dev) | 1 |

Rule: a new dependency needs a reason in the pull request description.
Rule: check for duplicate versions with `cargo tree -d` after a bump, because
`egui-wgpu` and our renderer must share one `wgpu` type.

## Module map

### `crates/core`

| Module | Contents |
|---|---|
| `lib.rs` | Re-exports only |
| `game.rs` | `Game`, the operations, the invariants |
| `record.rs` | `Record`, `MetaData`, `Result`, serde form, validation |
| `storage.rs` | Atomic write, read, path handling |
| `config.rs` | `Settings`, serde form, defaults |
| `notation.rs` | Display labels and the inverse |
| `error.rs` | `GameError`, `RecordError`, `ConfigError` |

### `crates/gui`

| Module | Contents |
|---|---|
| `main.rs` | `main`, logging, `EventLoop`, error report to the user |
| `app.rs` | `App` — winit `ApplicationHandler`, frame scheduling, state |
| `camera.rs` | Board-space to screen-space transform, zoom and pan clamping |
| `input.rs` | Event to action mapping, hit testing |
| `config.rs` | Load and store `Settings` |
| `render/context.rs` | Instance, adapter, device, queue, surface, MSAA targets |
| `render/board.rs` | Background, slab, wood, grid pass |
| `render/stones.rs` | Instanced stone pass |
| `render/shadow.rs` | Instanced contact shadow pass |
| `render/mesh.rs` | Lens mesh generation for a stone |
| `render/pipeline.rs` | Pipeline and bind group construction |
| `render/wgsl/` | `board.wgsl`, `stone.wgsl`, `shadow.wgsl`, `noise.wgsl` |
| `ui/topbar.rs` | File, Edit, View, Go menus |
| `ui/panel.rs` | Move list, player names, status |
| `ui/overlays.rs` | Coordinates, move numbers, last-move ring, winning line |
| `ui/dialogs.rs` | Truncate confirm, resume prompt, quit confirm |
| `ui/materials.rs` | Material presets and the tuning panel |
| `audio.rs` | Click synthesis, lock-free queue, cpal stream |

## Frame flow

1. winit delivers an event. `App` updates the `Game`, the camera, or the UI
   state, then requests a redraw.
2. On redraw, the camera produces the view uniform.
3. The board pass writes a full-screen quad.
4. The shadow pass writes one instanced quad per stone.
5. The stone pass writes one instanced lens per stone.
6. The MSAA target resolves into the surface texture.
7. egui writes the chrome and the markers into the same surface texture.
8. The frame presents.

Redraw is on demand. `ControlFlow::Wait` is the idle state, and a state change
calls `request_redraw`. A hidden window and an idle game cost nothing.

## Data flow for one move

```
mouse click
  └─ input.rs: screen point -> board point -> nearest intersection
       └─ Game::pending_truncation()  (non-zero: ui/dialogs.rs asks first)
            └─ Game::play(point)
                 ├─ engine Board::play
                 └─ moves.push
       └─ audio.rs: enqueue one click voice (neighbour-aware variant)
       └─ config autosave (debounced)
       └─ app.rs: request_redraw
```

## Build and run

```bash
# Build both crates
cargo build

# Run the application
cargo run --release -p gomoku-gui

# Tests, lints, formatting
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

The path dependency needs the sibling checkout:

```
workspace/
├── gomoku-gui/          this repository
└── rust-ml/gomoku/crates/engine
```

## Position in the ecosystem

`rust-ml` holds the AlphaZero-style agent and its terminal user interface.
This repository holds the graphical user interface for the same rules engine.
The core crate keeps the rules behind a facade, so an agent player can be
added later as a second source of moves. The record format does not change
when that happens.
