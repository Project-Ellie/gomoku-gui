# 00 — System Overview

## What the program is

A native desktop program for human and baseline AI players at one screen. It plays freestyle
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
        │  rust-ml engine (pinned git dependency)      │
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
| `engine`, `mcts`, `player`, `baseline` | Rules, search, decisions, Burn-backed evaluation | git `Project-Ellie/rust-ml`, rev `158b6114029d8708fe5d55394b2930f5e4fd1d8c` |
| `proptest` | Property tests (dev) | 1 |

Rule: a new dependency needs a reason in the pull request description.
Rule: check for duplicate versions with `cargo tree -d` after a bump, because
`egui-wgpu` and our renderer must share one `wgpu` type. Multiple `wgpu`
versions can coexist in a binary; types from different versions are not
interchangeable. The baseline evaluator uses genuine Burn computation
on the Flex CPU backend (`Device::flex()`), not a fake evaluator or a required
shared graphics device.

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
| `error.rs` | `GameError`, core-owned `PositionError`, `RecordError`, `ConfigError` |

### `crates/gui`

| Module | Contents |
|---|---|
| `main.rs` | `main`, logging, `EventLoop`, error report to the user |
| `app.rs` | `App` — winit `ApplicationHandler`, frame scheduling, state |
| `camera.rs` | Board-space to screen-space transform, zoom and pan clamping |
| `session.rs` | Game commands, metadata/save state, ownership, validated AI settings, generation/eligibility |
| `ai.rs` | Seat options, detached request/reply protocol, flight bookkeeping |
| `ai_worker.rs` | One thread owning the genuine baseline player, panic/error containment |
| `render.rs` | Board, stone and shadow pipelines, textures and instance data |
| `ui.rs` | Menus, per-color controls, history, markers and dialogs |
| `preview.rs` | Native offscreen GPU previews, including the real egui interface |
| `audio.rs` | Click synthesis, lock-free queue, cpal stream |

## AI worker and controls

Dependencies point **gomoku-gui → rust-ml**, never back. `gomoku-core` uses
only the pinned engine; the GUI worker consumes `player`/`mcts`/`baseline` at
the same revision. Burn CPU evaluation is real, not a mock or a Burn-free
mode. Graphics and AI do not share GPU resources.

`ui.rs` renders retained intent; Take/Release, validated numeric edits, Resume,
and player-name edits call `Session` methods only. Repainting is not a config
commit and cannot bump the generation. Names are saved metadata independent
of ownership. Default seats are Human Black / AI White; either or both can be
AI. Undo/review pauses automation until a live current AI turn is resumed.

`App` reconciles eligible turns before waiting. `Session::next_ai_request`
reserves one flight containing a cloned board, per-seat options, generation,
side, and identity. The worker constructs and retains the non-Send `AiPlayer`
inside its thread, using `BaselineEvaluator` on Burn `Device::flex()`.
Completion wakes the winit loop through a user event; it is not frame polling.
The session retires only the matching flight, then checks generation, turn,
ownership, live position and dialog eligibility before committing Play/Resign.
Taking a seat during search is immediate; stale work occupies the one slot
until it replies, but cannot alter the game. Worker failures pause automation.

Only committed stones enqueue a knock. Resignation sets a whole-game result
(v2 writes, v1/v2 reads), with no phantom move or winning line; review shows
the live result separately from the historical board. Renderer design is
unchanged. Analysis/proof overlays, search heatmaps, Swap2 and trained networks
are not part of M1.

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
  └─ app.rs / Session: screen point -> board point -> nearest intersection
       └─ Game::pending_truncation()  (non-zero: ui.rs asks first)
            └─ Session::place -> Game::play(point)
                 ├─ engine Board::play
                 └─ moves.push
       └─ audio.rs: enqueue one click voice (neighbour-aware variant)
       └─ config autosave (debounced)
       └─ app.rs: request_redraw
```

## Build and run

```bash
# Build the standalone workspace (Cargo fetches the pinned engine)
cargo build --workspace --locked

# Run the application
cargo run --release -p gomoku-gui

# Tests, lints, formatting
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all
cargo fmt --all -- --check
```

No sibling checkout or vendored engine is required. `Cargo.lock` records the
full git source revision. `mcts`, `player`, and `baseline` share the engine pin
in `[workspace.dependencies]` and are consumed by the GUI's M1 worker.
The dependency direction is only gomoku-gui → rust-ml. See the
[integration plan](../plans/2026-11-07-rust-ml-integration.md); ADR-008's
vendoring decision is superseded.

## Position in the ecosystem

`rust-ml` holds the AlphaZero-style agent and its terminal user interface.
This repository holds the graphical user interface for the same rules engine.
The core crate keeps the rules behind a facade; human clicks and validated AI
replies are two sources of moves. Records now write v2 to preserve resignation
without inventing a move; old v1 games remain readable.
