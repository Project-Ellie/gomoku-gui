# Gomoku GUI — Design

- Date: 2026-09-22
- Status: accepted (owner-approved in dialogue, 2026-09-22)
- Owner: Wolfie
- Repository: `Project-Ellie/gomoku-gui`

## 1. Purpose

A native desktop application for two players at one screen to play freestyle
Gomoku on a 15x15 board. The board and the stones must look like a real wooden
board with real stones. The application records games in files. It runs without
a server and without a network connection.

The application is a standalone program. It is not a library. It has one user.

## 2. Verified constraints

These facts were checked before the design was written. They are not
assumptions.

| Item | Verified value |
|---|---|
| Rules engine | `../rust-ml/gomoku/crates/engine` builds with `cargo check -p engine` (1.5 s). Freestyle Gomoku, overlines win, 15x15. |
| Engine API | `Board::{new, play, undo, moves, stone_at, status, to_move, zobrist, empty_moves, from_position}`, `Color`, `Status`, `PlayError`, `Move::{new, row, col, index}`. |
| Engine storage | `Move` is one `u8` = `row * 15 + col`. `Move::new(row, col)` returns `None` off the board. |
| Engine dependency | `thiserror` and `serde` only. No Burn, no I/O, no `rand`. |
| Engine testing | Differential suite against a naive oracle (`reference.rs`, feature `testutil`). |
| Engine write access | The engine is the owner's tutorial material. This project does not modify it. |
| Graphics | M1 Max, Metal 4, macOS 26.6.2. |
| wgpu | 30.0.1 is the current stable version. |
| winit | 0.30.13 is the current stable version. |
| egui | 0.36.2. `egui-wgpu` 0.36.2 requires `wgpu ^30.0` and `winit ^0.30.13`, so the overlay shares the renderer device. |
| rfd | 0.17.2, native file dialogs. |
| cpal | 0.18.2, audio output. |
| Toolchain | `cargo` and `rustc` 1.98.1. Rust edition 2024 is available. |
| Official record format | SGF FF[4] at red-bean.com defines `GM[4]` as "Gomoku+Renju". RIF publishes rules only, no file format. The owner chose our own JSON format instead (ADR-003). |
| Display notation | Tournaments label columns A-O (the letter I is included) and rows 1-15 from the bottom, so the centre is H8. |

## 3. Requirements

### 3.1 Functional requirements

| ID | Requirement |
|---|---|
| R1 | Two players play on one screen, one mouse and one keyboard. Black moves first. Stones alternate. |
| R2 | The board is 15x15. Stones are placed on intersections. |
| R3 | The game ends when a player makes five or more stones in a line. Overlines count as a win. A full board with no line is a draw. |
| R4 | Undo destroys the last move. The move does not come back. |
| R5 | Rewind and forward move a review cursor through the played moves. The game does not change. The user can return to the latest position at any time. |
| R6 | A placement made while the cursor is behind the latest move deletes all moves after the cursor, after one confirmation. |
| R7 | A game is saved to a file and loaded from a file. Save, Save As, and Open use native macOS dialogs. |
| R8 | The view zooms in and out continuously about the pointer, and pans by dragging. The zoom limits are set by the window size. |
| R9 | The board shows coordinates, a last-move marker, optional move numbers, and the winning line. |
| R10 | A move list shows every move. A click on a move moves the cursor to that position. |
| R11 | The application remembers window size and position, zoom, pan, panel state, material preset, overlay toggles, volume, last folder, and recent files. |
| R12 | The application saves the game in progress and offers to resume it at the next start. |
| R13 | Placing a stone plays a short knock that sounds like a stone on wood. |
| R14 | The application does not lose an unsaved game without a warning. |

### 3.2 Quality requirements

| ID | Requirement |
|---|---|
| Q1 | The board reads as a real wooden board: grain, pores, sheen, and a bevel. |
| Q2 | The dark stone reads as slate, the white stone reads as shell or milky quartz. Wood and stone are never confusable. |
| Q3 | Text and lines stay crisp at every zoom level. |
| Q4 | No `unwrap` in a production path. Errors are typed and reported to the user. |
| Q5 | The core crate has no GPU, window, audio, or dialog dependency. |
| Q6 | `cargo test`, `cargo clippy --all-targets -- -D warnings`, and `cargo fmt --all` pass before every commit. |
| Q7 | The window redraws on demand, not in a busy loop. |

## 4. Decisions

| # | Decision | ADR |
|---|---|---|
| D1 | wgpu 30 with winit 0.30 and hand-written WGSL for the board and the stones. | ADR-001 |
| D2 | Reuse the rust-ml engine behind a facade in our own core crate. | ADR-002 |
| D3 | Game records are versioned JSON. SGF FF[4] was evaluated and rejected by the owner. | ADR-003 |
| D4 | Undo is destructive. Rewind is a separate review cursor. A placement behind the cursor truncates the tail. | ADR-004 |
| D5 | A custom board and stone pass renders the game. egui renders the chrome, the text, and the markers. | ADR-005 |
| D6 | `thiserror` in core, `anyhow` in the binary. | ADR-006 |
| D7 | The stone knock is synthesised at start-up and played through cpal. No asset file. | ADR-007 |
| D8 | Two crates: `gomoku-core` and `gomoku-gui`. | §5 |
| D9 | Wood and stone textures are generated in the shader, not loaded from an image. | ADR-005 |
| D10 | Board and stone materials are presets, switchable from the menu and stored in the config. | ADR-005 |

## 5. Architecture

```
gomoku-gui/                     Cargo workspace, edition 2024, Apache-2.0
├── crates/core/                gomoku-core — pure logic, no I/O beyond files
│   └── src/{lib,game,record,storage,config,error,notation}.rs
└── crates/gui/                 gomoku-gui — binary
    └── src/{main,app,camera,input,config,
             render/{mod,context,board,stones,shadow,mesh,pipeline,wgsl},
             ui/{mod,topbar,panel,overlays,dialogs,materials},
             audio}.rs
```

Dependency direction: `gomoku-gui` → `gomoku-core` → `engine`.

The core crate owns the game, the history, the record format, the config
schema, and the notation. It has no dependency on a window, a GPU, sound, or
a file dialog. The caller gives it a path and it reads or writes the file.

The gui crate owns the window, the render pipeline, the input map, the user
interface, the dialogs, the audio output, and the application config file.

## 6. State model

`Game` holds one linear move list, one board, and one cursor.

```rust
pub struct Game {
    moves:  Vec<Move>,   // the record. The only truth about the game.
    board:  Board,       // the board at the review cursor
    cursor: usize,       // number of moves applied to `board`
    meta:   MetaData,    // players, creation time, result
}
```

- `cursor == moves.len()` means the view is at the latest position.
- `cursor < moves.len()` means the view is rewound.
- Every change goes through `play`, `undo`, `rewind`, `forward`, or `seek`.
  Therefore `cursor <= moves.len()` and `board == replay(moves[..cursor])`
  hold for all reachable states. Tests assert both.
- The board is moved one ply at a time with `Board::play` and `Board::undo`.
  No snapshots and no replay of the whole game are needed for a cursor move.

Operations:

| Operation | Effect on `moves` | Effect on `cursor` | Notes |
|---|---|---|---|
| `play(point)` | Truncate at the cursor, then append | `cursor += 1` | Caller confirms when `pending_truncation() > 0` |
| `undo()` | Drop the last move | `cursor = moves.len()` | Destructive. No redo stack. |
| `rewind()` | none | `cursor -= 1` | No effect at 0 |
| `forward()` | none | `cursor += 1` | No effect at the end |
| `seek(i)` | none | `cursor = i` | Move list click |
| `live()` | none | `cursor = moves.len()` | Return to the latest position |

## 7. Interaction model

| Input | Action |
|---|---|
| Left click on an empty intersection | Play, after the truncation confirm when needed |
| Left drag on the board | Pan |
| Scroll or pinch | Zoom about the pointer |
| Double click | Fit the board to the window |
| Cmd-N | New game, after a confirm when the game has moves |
| Cmd-O / Cmd-S / Cmd-Shift-S | Open, Save, Save As |
| Cmd-Z | Undo. Destroys the last move. |
| Left / Right arrow | Rewind / forward |
| Home / End | First position / latest position |
| Cmd-F or F | Fit the board |
| `+` / `-` / `0` | Zoom in / zoom out / fit |
| Cmd-Q | Quit, after a confirm when the game has unsaved changes |
| Space | Return to the latest position when the view is rewound |

The top bar has File, Edit, View, and Go menus. The right panel has the move
list, the player names, and the status. A rewound view shows a banner with a
"Return to latest" button. The window title shows the file name and a modified
marker.

A placement is refused on an occupied intersection and after the game ends.
The intersection under the pointer is highlighted when a placement is legal.

## 8. Persistence

Three files, three purposes.

| File | Location | Content |
|---|---|---|
| Game record | User choice, `.json` | The game: version, size, ruleset, players, creation time, result, moves |
| Config | `~/Library/Application Support/gomoku-gui/config.toml` | Window, view, material, overlay, audio, and file settings |
| Autosave | `~/Library/Application Support/gomoku-gui/autosave.json` plus `autosave-state.json` | The game in progress and the path it came from |

- Moves are stored as `[column, row]` pairs with 0-based indices. Column 0 is
  left, row 0 is the top. Numeric points do not depend on the display notation.
- Writes are atomic: write a temporary file in the same directory, then rename.
- A load rejects a version above 1, a size other than 15, an out-of-range
  point, an illegal move sequence, and a move list longer than 225. Unknown
  fields are ignored, so a later minor addition does not break an older file.
- The truncation confirm, the resume prompt, and the quit confirm are the only
  modal dialogs.

The record schema, the config schema, and the validation rules are in
`docs/architecture/03_PERSISTENCE.md`.

## 9. Rendering

Two passes of our own, then one egui pass.

1. Background, slab, wood, and grid. One full-screen pass. The shader computes
   a rounded-rectangle distance field for the slab, so the bevel, the drop
   shadow, and the anti-aliased edge come from the same expression.
2. Contact shadows. One instanced pass of soft elliptical quads, one per stone,
   offset along the light direction, with a darkening blend.
3. Stones. One instanced pass. One lens mesh, one instance per stone, one draw
   call for the whole board.
4. MSAA resolve.
5. egui overlay: top bar, panel, coordinates, move numbers, last-move ring,
   winning line, banners, dialogs.

A depth buffer is not needed. Stones are 0.94 cell apart at the closest, so
their screen footprints never overlap.

Wood and stone textures are generated in the shader from board-space
coordinates, so the grain belongs to the board and does not slide when the view
moves. Grain detail is faded with `fwidth`, so zooming in reveals more detail
and zooming out does not alias. Grid lines are computed in the shader and
anti-aliased, so they stay one pixel wide at every zoom.

The camera is a plain orthographic view. `pixels_per_cell` is the only scale
factor. The zoom limits are `[fit, fit * 40]`, where `fit` fits the whole board
in the window. Zoom keeps the board point under the pointer fixed. Panning is
clamped, and the centre snaps to the board centre when the whole board fits.

Shader detail, the material parameters, and the camera arithmetic are in
`docs/architecture/04_RENDERING.md`.

## 10. Audio

`cpal` writes to the default output device. The click is four damped
resonators excited by a short noise burst. Six variations are rendered at
start-up, so the audio callback only mixes from a buffer. The callback does
not allocate, does not lock, and does not log. A slightly duller, shorter
variation plays when the neighbouring intersection is occupied.

A missing audio device disables sound, logs a warning, and does not stop the
application. See `docs/architecture/06_AUDIO.md`.

## 11. Testing and gates

| Layer | Test |
|---|---|
| core | Behaviour tests per operation, the truncation rule, the game-end rule, notation mapping |
| core | `proptest` invariants: cursor and board stay consistent, save and load round-trip, undo and forward restore equal state |
| core | Golden JSON test, so schema drift fails loudly |
| core | Malformed record cases: bad version, bad size, bad point, duplicate point, illegal sequence, oversized list |
| gui | Pure-function tests: camera round-trip, zoom clamp, pan clamp, hit testing, notation labels, grain fade |
| gui | `naga` validates every WGSL file, so a shader error fails `cargo test` without a GPU |
| manual | Run the application, play a game, undo, rewind, save, load, zoom, and listen to the sound |

Gates before every commit: `cargo test`,
`cargo clippy --all-targets -- -D warnings`, `cargo fmt --all`.

## 12. Build order

Each slice ends with green gates and one commit.

1. Repo skeleton: workspace, two crate manifests, LICENSE, README, docs.
2. core: `Game` with play, undo, rewind, forward, seek and the invariant tests.
3. core: record save and load, validation, golden test.
4. core: config schema.
5. gui: window, wgpu context, camera, flat-colour board. Proves the pipeline.
6. gui: wood material, slab, grid lines, background.
7. gui: stone mesh, instancing, slate and shell materials, contact shadows.
8. gui: egui overlay, top bar, panel, move list, status, banners.
9. gui: input map, hit testing, zoom, pan, shortcuts, dialogs.
10. gui: autosave, config persistence, file dialogs, recent files.
11. gui: audio.
12. gui: material presets and the tuning panel. Visual polish.
13. Docs update and final review.

## 13. Risks

| Risk | Handling |
|---|---|
| The engine is a path dependency to a sibling repository. A different machine needs that checkout. | Document it in the README. Switch to a pinned git revision if the repository is cloned elsewhere. |
| The GUI may need an engine change. | The engine belongs to the owner's tutorial material. Report the need instead of editing it. |
| The procedural wood look needs iteration by eye. | A tuning panel in the View menu exposes the material parameters. The chosen values become the preset. |
| wgpu 30 is a recent release. | Verify each API against the pinned version at implementation time. The design avoids API names. |
| `egui-wgpu` and our renderer must share one device. | The required versions match. A version bump is checked with `cargo tree -d`. |
| Screen capture on macOS needs permission for visual verification. | The owner can look at the window. |

## 14. Out of scope

- A computer opponent. The core crate is built so an engine player can be
  added later without a change to the record format.
- Network play.
- Clocks, resign, and byo-yomi.
- Board sizes other than 15x15.
- Renju forbidden moves. The ruleset is freestyle.
- SGF import and export. The record module is the only place that knows the
  format, so an exporter stays a contained change.
