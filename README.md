# gomoku-gui

A native desktop application for two players at one screen to play freestyle
Gomoku on a 15x15 board. The board and the stones are rendered as real
materials: a wooden board with grain, pores, and a sheen, with slate and shell
stones that cast contact shadows.

No server. No network. One window.

## Status

The `gomoku-core` crate is implemented: game state, the undo and rewind
history, tournament notation, the versioned record format, and the settings
schema. The graphical crate is not started.

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
# Build the workspace
cargo build

# Run the application. This works when the graphical crate lands, which the
# Status section describes as not started.
cargo run --release -p gomoku-gui
```

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
