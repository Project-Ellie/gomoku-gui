# gomoku-gui

A native desktop application for humans and a baseline AI to play freestyle
Gomoku on a 15x15 board. The board and the stones are rendered as real
materials: a wooden board with grain, pores, and a sheen, with slate and shell
stones that cast contact shadows.

No server. No network. One window.

## Status

The application opens a wooden board with stone pieces, a menu bar, seat
controls, history, and file dialogs. Human and baseline AI turns share the same
game history. Games are saved and loaded, and the game in progress survives a crash.

- `gomoku-core` — game state, undo and rewind history, notation, the versioned
  record format, and the settings schema. Complete.
- `engine` — the rules engine from `Project-Ellie/rust-ml`, pinned to git
  revision `158b6114029d8708fe5d55394b2930f5e4fd1d8c`. Cargo fetches it;
  no sibling checkout is needed. See the
  [integration plan](docs/plans/2026-11-07-rust-ml-integration.md).
  `player`, `mcts`, and the Burn-backed `baseline` share this pin. A dedicated
  worker runs genuine baseline evaluation on Burn's Flex CPU backend.
- `gomoku-gui` — the window, board and stone shaders, stone knock, automatic AI
  controller/worker, and per-color ownership and search controls. Analysis
  overlays, trained networks, and Swap2 remain future milestones.
- Puzzle loading (File → Load puzzle...) for `vcf-material-v1` files with a
  `.parser` sidecar. The first move of the claimed solution is marked with a
  bright green cursor ring. Games loaded from a puzzle position cannot be saved
  as game records.

- Approved design: [docs/specs/2026-09-22-gomoku-gui-design.md](docs/specs/2026-09-22-gomoku-gui-design.md)
- Implementation plan for the core crate: [docs/plans/2026-09-22-gomoku-core.md](docs/plans/2026-09-22-gomoku-core.md)

## Requirements

- Rust 1.85 or later (edition 2024). Verified with 1.98.1.
- macOS with Metal. Verified on an M1 Max, macOS 26.6.2.
- Network access for Cargo's initial dependency fetch (including the pinned
  rules engine). A clone builds on its own; no local rust-ml checkout is used.

```
gomoku-gui/
├── crates/core        game state, history, records, settings
├── crates/gui         the window and the interface
└── tools/generate-glyph-atlas
```

## Build and run

```bash
# Open the board. Start from a short opening with --demo.
cargo run --release -p gomoku-gui -- --demo

# Build everything, and run the test suite.
cargo build --workspace --locked
cargo test --workspace --locked
```

### Check the look without a window

`--preview` renders one frame to a file, which is how the shaders are checked on
a machine with no display. `--preview-ui` draws the interface as well, and
`--scale N` renders at the number of pixels per interface point that a scaled
display has:

```bash
cargo run -p gomoku-gui -- --demo --preview artifacts/board.bmp --size 1200
cargo run -p gomoku-gui -- --demo --preview artifacts/ui.bmp --size 1100 \
    --preview-ui --scale 2
# Minimum side-panel width (points), independent of display scale:
cargo run -p gomoku-gui -- --demo --preview target/ui-narrow.bmp --size 800 \
    --preview-ui --panel-width 180 --scale 2
```

Previews use a fresh in-memory session, do not start an AI worker, and do not
read or write the normal application profile.

`--frames N` quits after N frames, which is a smoke test of the whole pipeline.

## Gates

Run these before every commit. All three must pass.

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all
cargo fmt --all -- --check
```

## Controls

| Input | Action |
|---|---|
| Left click | Place a stone for the current human-owned color |
| Release Black / Release White | Give that color to automatic baseline AI |
| Take Black / Take White | Take that color for human play, even during search |
| Resume AI | Resume continuous automatic turns after undo/review, at the live AI turn |
| Left / Right arrow | Rewind / forward through the played moves |
| Home / End | First position / latest position |
| Cmd-Z | Undo. The move is destroyed. |
| Cmd-N / Cmd-O / Cmd-S / Cmd-Shift-S | New / Open / Save / Save As |
| Scroll or pinch | Zoom about the pointer |
| Left drag | Pan |
| Double click, `0`, `F`, Cmd-F | Fit the board |
| `C` `M` `L` `W` | Toggle coordinates, move numbers, last-move marker, winning line |
| `V` | Flip the view |
| Space | Return to the latest position when rewound |

Black starts **Human**, White **AI · baseline**. Place a black stone and White
thinks automatically—there is no Step command. Both colors may be AI. Each
seat retains its own **Sims** (default 400, range 1–10,000) and **Temp** (default
0, range 0–2 in steps of 0.1), including while human-owned. Click a value to
type it, or drag it. Player names are editable saved metadata; Take/Release
never replaces them. Seat ownership and search settings are session-only.

**Resignation is ON by default.** It is the baseline's heuristic concession,
not a proof. A resignation records the result, not a stone, move number,
winning line, or stone sound. The live result stays visible during review.

Undo and history navigation pause AI. Return to the latest position, then
use **Resume AI** when the current color is AI. Releasing the current seat also
resumes automatic turns. Taking a seat invalidates pending decisions without
blocking on the worker. Dialogs disable background controls.

Undo destroys the last move permanently. Rewind moves the view and is
reversible. A placement made while rewound deletes the moves after the cursor,
after one confirmation. See
[ADR-004](docs/decisions/ADR-004-history-model.md).

## Documentation

| Path | Contents |
|---|---|
| [docs/specs/](docs/specs/) | The approved design, requirements, and build order |
| [docs/plans/](docs/plans/) | Implementation plans |
| [docs/architecture/](docs/architecture/) | System overview, domain model, core logic, persistence, rendering, interaction, audio, testing |
| [docs/decisions/](docs/decisions/) | Architecture decision records |

Start with the design document. It lists what was verified before the design
was written, and it points at the detail in each architecture document.

## Records

A game is saved as a versioned JSON document with the marker
`"format": "gomoku-gui/record"`. New saves write **version 2**, including
resignation results; the loader reads **versions 1 and 2**. The schema is in
[docs/architecture/03_PERSISTENCE.md](docs/architecture/03_PERSISTENCE.md).

SGF FF[4] with `GM[4]` is the official Gomoku and Renju record format. It was
evaluated and rejected in favour of this format, which means files do not
interchange with RenLib, Sabaki, or other renju tools. The reasoning and the
cost are recorded in [ADR-003](docs/decisions/ADR-003-record-format.md).

## Licence

Apache-2.0, the same as `rust-ml` and `DeepGomoku`.

The rules engine is a pinned git dependency from `Project-Ellie/rust-ml` and
keeps its licence and its authorship. [ADR-008](docs/decisions/ADR-008-engine-vendored.md)
records the superseded vendoring decision.
