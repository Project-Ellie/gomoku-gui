# gomoku-gui

A native desktop application for two players at one screen to play freestyle
Gomoku on a 15x15 board. The board and the stones are rendered as real
materials: a wooden board with grain, pores, and a sheen, with slate and shell
stones that cast contact shadows.

No server. No network. One window.

## Status

The application opens a window with a wooden board and stone pieces. You can
place stones, hear them land, pan, and zoom.

- `gomoku-core` — game state, undo and rewind history, notation, the versioned
  record format, and the settings schema. Complete.
- `gomoku-gui` — the window, the board and stone shaders, and the stone knock.
  Under construction: the sitting now has placing, panning, zooming, and undo.
  Saving, loading, the move list, and the overlays arrive next.

- Approved design: [docs/specs/2026-09-22-gomoku-gui-design.md](docs/specs/2026-09-22-gomoku-gui-design.md)
- Implementation plan for the core crate: [docs/plans/2026-09-22-gomoku-core.md](docs/plans/2026-09-22-gomoku-core.md)

## Requirements

- Rust 1.85 or later (edition 2024). Verified with 1.98.1.
- macOS with Metal. Verified on an M1 Max, macOS 26.6.2.
- The sibling rules engine checkout:

```
workspace/
├── gomoku-gui/                       this repository
└── rust-ml/gomoku/crates/engine      rules engine, path dependency
```

Without that directory the build fails. See
[ADR-002](docs/decisions/ADR-002-engine-reuse.md).

## Build and run

```bash
# Open the board. Start from a short opening with --demo.
cargo run --release -p gomoku-gui -- --demo

# Build everything, and run the test suite.
cargo build
cargo test
```

| Input | Action |
|---|---|
| Left click on an intersection | Place a stone |
| Left drag | Pan the view |
| Scroll, or pinch | Zoom about the pointer |
| `u`, or Backspace | Take the last stone back |
| `f`, or `0` | Fit the whole board |
| `v` | Turn the board around |
| Escape | Quit |

### Check the look without a window

`--preview` renders one frame to a file, which is how the shaders are checked on
a machine with no display:

```bash
cargo run -p gomoku-gui -- --demo --preview artifacts/board.bmp --size 1200
```

`--frames N` quits after N frames, which is a smoke test of the whole pipeline.

## Gates

Run these before every commit. All three must pass.

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

## Controls

| Input | Action |
|---|---|
| Left click | Place a stone |
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
`"format": "gomoku-gui/record"`. The schema is in
[docs/architecture/03_PERSISTENCE.md](docs/architecture/03_PERSISTENCE.md).

SGF FF[4] with `GM[4]` is the official Gomoku and Renju record format. It was
evaluated and rejected in favour of this format, which means files do not
interchange with RenLib, Sabaki, or other renju tools. The reasoning and the
cost are recorded in [ADR-003](docs/decisions/ADR-003-record-format.md).

## Licence

Apache-2.0, the same as `rust-ml` and `DeepGomoku`.
