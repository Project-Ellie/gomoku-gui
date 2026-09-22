# Gomoku Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `gomoku-core`, the pure Rust crate that owns game state, the move history with its undo and rewind semantics, tournament notation, the versioned JSON record format, and the settings schema.

**Architecture:** One workspace with one crate in this plan (`crates/core`); `crates/gui` is added by the follow-up GUI plan. `gomoku-core` depends on the existing `engine` crate (path dependency) and calls it from `game.rs` only. The crate has no window, GPU, audio, or dialog dependency, so everything in it is testable with `cargo test` alone.

**Tech Stack:** Rust edition 2024, `engine` (path `../rust-ml/gomoku/crates/engine`), `serde` + `serde_json`, `time`, `toml`, `thiserror`, `proptest` (dev), `tempfile` (dev).

Scope note: this plan covers the core crate only. The specification is `docs/specs/2026-09-22-gomoku-gui-design.md`; the detail that these tasks implement is `docs/architecture/01_DOMAIN_MODEL.md`, `02_CORE_LOGIC.md`, and `03_PERSISTENCE.md`.

## Global Constraints

- Rust edition 2024. `rust-version = "1.85"`.
- Licence Apache-2.0. Every new `.rs` file starts with a module doc comment.
- `gomoku-core` never depends on a window, GPU, audio, or file-dialog crate. It receives paths and reads or writes them.
- The `engine` crate is a path dependency and is **not modified**. If a task appears to need an engine change, stop and report BLOCKED.
- No `unwrap`, `expect`, or `panic!` in a production path. The only exception is an invariant proven in a comment directly above the call.
- No `let _ =` on a `Result`. Handle it or propagate it with `?`.
- `#![deny(missing_docs)]` in `lib.rs`. Every public item has a doc comment of one line or more.
- No `unsafe`.
- Exact values, copied from the spec: format marker `gomoku-gui/record`; version `1`; board size `15`; rule set `freestyle`; moves stored as `[column, row]` with 0-based indices, column 0 left, row 0 top.
- Notation: columns `A` to `O` with the letter I included, rows numbered `15` at the top down to `1` at the bottom, so the centre is `H8`. Do not skip the letter I: 15 columns need 15 labels, and published renju notation uses `I8`.
- Gates before every commit, all three green: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --all`.
- Comments and doc comments are written in Simplified Technical English: short sentences, active voice, one instruction per sentence.
- Add no dependency that is not listed in this plan.
- Model policy for every subagent dispatch in this project: pass
  `model: "deepseek/deepseek-v4-flash"` explicitly, for implementers and for
  reviewers. The `reviewer` agent's configured default is a more expensive
  model and must not be inherited. An omitted model parameter silently
  inherits the session default, so always state it.

---

### Task 1: Workspace, core crate, errors, and notation

**Files:**
- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Create: `crates/core/Cargo.toml`
- Create: `crates/core/src/lib.rs`
- Create: `crates/core/src/error.rs`
- Create: `crates/core/src/notation.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: the workspace, the crate `gomoku-core`, `gomoku_core::GameError`, `gomoku_core::RecordError`, `gomoku_core::ConfigError`, `gomoku_core::label(point: engine::Move) -> String`.

- [ ] **Step 1: Create the workspace manifest**

`Cargo.toml`:

```toml
[workspace]
resolver = "3"
members = ["crates/core"]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "Apache-2.0"
rust-version = "1.85"

[workspace.dependencies]
engine = { path = "../rust-ml/gomoku/crates/engine" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
time = { version = "0.3", features = ["serde", "serde-human-readable", "macros"] }
toml = "1"
thiserror = "2"
proptest = "1"
tempfile = "3"
```

`rust-toolchain.toml`:

```toml
[toolchain]
channel = "stable"
components = ["clippy", "rustfmt"]
```

Note: the path `../rust-ml/gomoku/crates/engine` is relative to this manifest, so the checkout layout must be `workspace/gomoku-gui` beside `workspace/rust-ml`. If the path does not resolve, stop and report BLOCKED.

- [ ] **Step 2: Create the crate manifest**

`crates/core/Cargo.toml`:

```toml
[package]
name = "gomoku-core"
description = "Game state, history, notation, record format, and settings for the Gomoku GUI"
version.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[dependencies]
engine.workspace = true
serde.workspace = true
serde_json.workspace = true
time.workspace = true
toml.workspace = true
thiserror.workspace = true

[dev-dependencies]
proptest.workspace = true
tempfile.workspace = true
```

- [ ] **Step 3: Write the failing test for notation**

`crates/core/src/notation.rs`:

```rust
//! Display labels for board points, in tournament notation.

use engine::Move;

const COLUMN_LETTERS: [u8; 15] = *b"ABCDEFGHIJKLMNO";

/// The display label of a point: columns A to O, rows 1 to 15 counted from
/// the bottom, so the centre of the board is `H8`.
pub fn label(point: Move) -> String {
    let letter = COLUMN_LETTERS[point.col() as usize] as char;
    let number = 15 - point.row();
    format!("{letter}{number}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn point(row: u8, col: u8) -> Move {
        Move::new(row, col).expect("row and col are inside the board")
    }

    #[test]
    fn centre_of_the_board_is_h8() {
        assert_eq!(label(point(7, 7)), "H8");
    }

    #[test]
    fn every_column_has_its_own_letter() {
        assert_eq!(label(point(0, 0)), "A15");
        assert_eq!(label(point(0, 8)), "I15");
        assert_eq!(label(point(0, 14)), "O15");
    }

    #[test]
    fn rows_count_from_the_bottom() {
        assert_eq!(label(point(0, 0)), "A15");
        assert_eq!(label(point(14, 0)), "A1");
        assert_eq!(label(point(14, 14)), "O1");
    }

    #[test]
    fn all_labels_are_unique() {
        let mut seen = HashSet::new();
        for row in 0..15u8 {
            for col in 0..15u8 {
                assert!(seen.insert(label(point(row, col))));
            }
        }
        assert_eq!(seen.len(), 225);
    }
}
```

- [ ] **Step 4: Run the test to verify it fails**

Run: `cargo test -p gomoku-core notation`
Expected: cargo fails, because the crate root `crates/core/src/lib.rs` does not exist yet. A test that cannot compile is the red step of the first cycle.

- [ ] **Step 5: Write the error types**

`crates/core/src/error.rs`:

```rust
//! The error types of the core crate.

use std::path::PathBuf;

use engine::PlayError;
use thiserror::Error;

/// Errors returned by the game operations.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum GameError {
    /// The intersection already holds a stone.
    #[error("That intersection already holds a stone.")]
    Occupied,
    /// The game is over.
    #[error("The game is over. Take the last move back or start a new game.")]
    GameOver,
    /// The rules engine rejected the placement.
    #[error("The rules engine rejected the move: {0}")]
    Rejected(PlayError),
}

impl From<PlayError> for GameError {
    fn from(err: PlayError) -> GameError {
        match err {
            PlayError::Occupied => GameError::Occupied,
            PlayError::GameOver => GameError::GameOver,
            other => GameError::Rejected(other),
        }
    }
}

/// Errors returned when a record is read, validated, or written.
#[derive(Debug, Error)]
pub enum RecordError {
    /// The file could not be read or written.
    #[error("{}: {source}", path.display())]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The error from the file system.
        #[source]
        source: std::io::Error,
    },
    /// The text is not a valid record.
    #[error("This file is not readable as a game record: {0}")]
    Parse(String),
    /// The file is a record of another kind.
    #[error("This file is not a Gomoku game record.")]
    WrongFormat {
        /// The marker found in the file.
        found: String,
    },
    /// The record is newer than this build can read.
    #[error("This file is a version {0} record. This version of Gomoku opens version 1.")]
    UnsupportedVersion(u32),
    /// The record is for a board size this build does not play.
    #[error("This file is for a {0}x{0} board. This version of Gomoku plays 15x15.")]
    UnsupportedSize(u32),
    /// The record uses a rule set this build does not play.
    #[error("This file uses the rule set \"{0}\". This version of Gomoku plays freestyle.")]
    UnsupportedRuleset(String),
    /// The record holds more moves than the board has cells.
    #[error("This file holds {count} moves, but a 15x15 board has 225 cells.")]
    TooManyMoves {
        /// The number of moves in the file.
        count: usize,
    },
    /// A move is outside the board.
    #[error("Move {index} in this file is outside the board.")]
    PointOutOfRange {
        /// The 0-based index of the move in the file.
        index: usize,
    },
    /// The same cell appears twice.
    #[error("Move {index} in this file repeats a cell that already holds a stone.")]
    RepeatedPoint {
        /// The 0-based index of the move in the file.
        index: usize,
    },
    /// A move is not legal in sequence.
    #[error("Move {index} in this file is not legal. The file may be damaged.")]
    IllegalMove {
        /// The 0-based index of the move in the file.
        index: usize,
    },
    /// The stored result disagrees with the stored moves.
    #[error("The result stored in this file does not match the moves it holds.")]
    ResultMismatch,
}

/// Errors returned when the settings are written.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// The settings file could not be written.
    #[error("{}: {source}", path.display())]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The error from the file system.
        #[source]
        source: std::io::Error,
    },
    /// The settings could not be encoded.
    #[error("The settings could not be encoded: {0}")]
    Encode(String),
}
```

- [ ] **Step 6: Write the crate root**

`crates/core/src/lib.rs`:

```rust
//! Game state, history, notation, record format, and settings for the
//! Gomoku GUI.
//!
//! This crate is pure. It has no window, no GPU, no audio, and no file
//! dialog. The rules engine is called from the `game` module only.
#![deny(missing_docs)]

mod error;
mod notation;

pub use error::{ConfigError, GameError, RecordError};
pub use notation::label;
```

Later tasks add their own modules to this file, and each task states the exact
lines to add. Do not add a module before its task.

`ConfigError` is defined in this task but is first used in Task 6. The
`missing_docs` deny covers it from the start.

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test -p gomoku-core`
Expected: 4 tests pass, in `notation::tests`.

- [ ] **Step 8: Run the gates**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --all && cargo test`
Expected: no warnings, no diff, all tests pass.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml rust-toolchain.toml crates/core
git commit -m "feat(core): add the workspace, core crate, error types, and notation"
```

---

### Task 2: The `Game` aggregate

**Files:**
- Create: `crates/core/src/game.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Consumes: `gomoku_core::GameError` from Task 1.
- Produces: `gomoku_core::Game` with `new`, `started_at`, `meta`, `set_players`, `stone_at`, `stones`, `status`, `outcome`, `to_move`, `moves`, `len`, `is_empty`, `cursor`, `is_rewound`, `pending_truncation`, `last_move`, `play`, `undo`, `rewind`, `forward`, `seek`, `live`; and the types `MetaData`, `Outcome`, `WinMethod`.

**Review amendments from Task 1 — apply these two small edits first.** The Task 1
review recommended them, and they belong to files that Task 1 already committed,
so they land here rather than by rewriting that commit.

1. In `crates/core/src/notation.rs`, put the invariant above the indexing line,
   because that index can panic if the function signature ever changes:

```rust
    // Move::col() is at most 14, so the index is always inside the array.
    let letter = COLUMN_LETTERS[point.col() as usize] as char;
```

2. In `crates/core/src/error.rs`, replace the catch-all arm of
   `From<PlayError> for GameError` with an explicit arm for the third engine
   variant. A catch-all silently reclassifies any variant that the engine adds
   later; an explicit arm turns that into a compile error.

Replace:

```rust
            other => GameError::Rejected(other),
```

with:

```rust
            // Explicit, so that a new engine variant fails to compile here
            // instead of being silently reclassified.
            PlayError::BadOpeningCounts => GameError::Rejected(PlayError::BadOpeningCounts),
```

Engine facts this task relies on, all verified:
`Board::new()`, `Board::play(Move) -> Result<(), PlayError>`, `Board::undo()` (panics on an empty history), `Board::moves() -> &[Move]`, `Board::stone_at(Move) -> Option<Color>`, `Board::status() -> Status`, `Board::to_move() -> Color`, `Status::{Ongoing, Won(Color), Draw}`, `Color::{Black, White}`, `PlayError::{Occupied, GameOver, BadOpeningCounts}`, `Move::new(row, col) -> Option<Move>`, `Move::row()`, `Move::col()`.

- [ ] **Step 1: Write the failing tests**

Add to the end of `crates/core/src/game.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn point(row: u8, col: u8) -> Move {
        Move::new(row, col).expect("row and col are inside the board")
    }

    fn game() -> Game {
        Game::started_at(datetime!(2026-09-22 18:04:11 UTC))
    }

    /// Five black stones on row 7 at columns 3 to 7. White replies on the
    /// diagonal (3,0), (4,1), (5,2), (6,3), which is only four stones.
    fn near_win() -> Game {
        let mut game = game();
        for index in 0..4 {
            game.play(point(7, 3 + index)).expect("empty cell");
            game.play(point(3 + index, index)).expect("empty cell");
        }
        game.play(point(7, 7)).expect("empty cell");
        game
    }

    #[test]
    fn a_new_game_is_empty() {
        let game = game();
        assert_eq!(game.len(), 0);
        assert!(game.is_empty());
        assert_eq!(game.cursor(), 0);
        assert!(!game.is_rewound());
        assert_eq!(game.to_move(), Color::Black);
    }

    #[test]
    fn play_places_a_stone_and_flips_the_side() {
        let mut game = game();
        game.play(point(7, 7)).expect("empty cell");
        assert_eq!(game.stone_at(point(7, 7)), Some(Color::Black));
        assert_eq!(game.to_move(), Color::White);
        assert_eq!(game.len(), 1);
        assert_eq!(game.last_move(), Some(point(7, 7)));
    }

    #[test]
    fn play_rejects_an_occupied_cell_and_changes_nothing() {
        let mut game = game();
        game.play(point(7, 7)).expect("empty cell");
        assert_eq!(game.play(point(7, 7)), Err(GameError::Occupied));
        assert_eq!(game.len(), 1);
        assert_eq!(game.to_move(), Color::White);
    }

    #[test]
    fn play_is_rejected_after_a_win() {
        let mut game = near_win();
        assert_eq!(game.outcome(), Outcome::Won { winner: Color::Black, method: WinMethod::Five });
        assert_eq!(game.play(point(10, 10)), Err(GameError::GameOver));
        assert_eq!(game.len(), 9);
    }

    #[test]
    fn stones_are_listed_in_play_order_with_alternating_colors() {
        let mut game = game();
        game.play(point(7, 7)).expect("empty cell");
        game.play(point(7, 8)).expect("empty cell");
        let stones: Vec<_> = game.stones().collect();
        assert_eq!(stones, vec![(point(7, 7), Color::Black), (point(7, 8), Color::White)]);
    }

    #[test]
    fn undo_removes_the_last_move() {
        let mut game = game();
        game.play(point(7, 7)).expect("empty cell");
        game.play(point(7, 8)).expect("empty cell");
        assert!(game.undo());
        assert_eq!(game.len(), 1);
        assert_eq!(game.cursor(), 1);
        assert_eq!(game.stone_at(point(7, 8)), None);
        assert_eq!(game.to_move(), Color::White);
    }

    #[test]
    fn undo_on_an_empty_game_does_nothing() {
        let mut game = game();
        assert!(!game.undo());
        assert!(game.is_empty());
    }

    #[test]
    fn undo_reopens_a_won_game() {
        let mut game = near_win();
        assert!(game.undo());
        assert_eq!(game.outcome(), Outcome::Ongoing);
        assert_eq!(game.status(), Status::Ongoing);
        game.play(point(7, 2)).expect("empty cell");
        assert_eq!(game.len(), 9);
    }

    #[test]
    fn undo_leaves_a_rewound_view_first() {
        let mut game = game();
        game.play(point(7, 7)).expect("empty cell");
        game.play(point(7, 8)).expect("empty cell");
        game.rewind();
        assert!(game.undo());
        assert_eq!(game.len(), 1);
        assert_eq!(game.cursor(), 1);
    }

    #[test]
    fn rewind_and_forward_move_the_view_only() {
        let mut game = game();
        game.play(point(7, 7)).expect("empty cell");
        game.play(point(7, 8)).expect("empty cell");

        assert!(game.rewind());
        assert!(game.is_rewound());
        assert_eq!(game.cursor(), 1);
        assert_eq!(game.len(), 2);
        assert_eq!(game.stone_at(point(7, 8)), None);
        assert_eq!(game.to_move(), Color::White);

        assert!(game.forward());
        assert!(!game.is_rewound());
        assert_eq!(game.stone_at(point(7, 8)), Some(Color::White));
        assert_eq!(game.to_move(), Color::Black);
    }

    #[test]
    fn rewind_and_forward_stop_at_the_ends() {
        let mut game = game();
        assert!(!game.rewind());
        assert!(!game.forward());
        game.play(point(7, 7)).expect("empty cell");
        assert!(!game.forward());
        assert!(game.rewind());
        assert!(!game.rewind());
    }

    #[test]
    fn seek_clamps_to_the_game_length() {
        let mut game = game();
        game.play(point(7, 7)).expect("empty cell");
        game.play(point(7, 8)).expect("empty cell");
        // Leave the live position first, so that the clamp is observable.
        game.seek(0);
        assert!(game.seek(999));
        assert_eq!(game.cursor(), 2);
        assert!(game.seek(0));
        assert_eq!(game.cursor(), 0);
        assert_eq!(game.stone_at(point(7, 7)), None);
        assert!(!game.seek(0));
    }

    #[test]
    fn live_returns_to_the_latest_move() {
        let mut game = game();
        game.play(point(7, 7)).expect("empty cell");
        game.play(point(7, 8)).expect("empty cell");
        game.seek(0);
        assert!(game.live());
        assert_eq!(game.cursor(), 2);
        assert!(!game.live());
    }

    #[test]
    fn pending_truncation_reports_the_moves_after_the_cursor() {
        let mut game = game();
        for col in 0..4 {
            game.play(point(7, col)).expect("empty cell");
        }
        assert_eq!(game.pending_truncation(), 0);
        game.seek(1);
        assert_eq!(game.pending_truncation(), 3);
    }

    #[test]
    fn play_while_rewound_deletes_the_moves_after_the_cursor() {
        let mut game = game();
        game.play(point(7, 7)).expect("empty cell");
        game.play(point(7, 8)).expect("empty cell");
        game.play(point(9, 9)).expect("empty cell");
        game.seek(1);
        game.play(point(3, 3)).expect("empty cell");

        assert_eq!(game.len(), 2);
        assert_eq!(game.cursor(), 2);
        assert!(!game.is_rewound());
        assert_eq!(game.stone_at(point(7, 8)), None);
        assert_eq!(game.stone_at(point(9, 9)), None);
        assert_eq!(game.stone_at(point(3, 3)), Some(Color::White));
    }

    #[test]
    fn meta_keeps_the_player_names_and_the_start_time() {
        let mut game = game();
        assert_eq!(game.meta().created, datetime!(2026-09-22 18:04:11 UTC));
        game.set_players(Some("Wolfie".to_string()), None);
        assert_eq!(game.meta().black.as_deref(), Some("Wolfie"));
        assert_eq!(game.meta().white, None);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p gomoku-core game`
Expected: a compile error, because `Game` does not exist. That is the failure for this step.

- [ ] **Step 3: Write the implementation**

Replace the whole of `crates/core/src/game.rs` with:

```rust
//! The game aggregate: one move list, one board, one review cursor.
//!
//! `moves` is the only truth about the game. `board` is always the position
//! after `moves[..cursor]`, and `cursor <= moves.len()` always holds.
//! Every change goes through the methods of `Game`, so both facts hold by
//! construction.

use engine::{Board, Color, Move, Status};
use time::OffsetDateTime;

use crate::error::GameError;

/// How a game was won.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinMethod {
    /// Five or more stones in a line. Overlines win.
    Five,
}

/// The result of a game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The game is still open.
    Ongoing,
    /// A player has won.
    Won {
        /// The winner.
        winner: Color,
        /// How the game was won.
        method: WinMethod,
    },
    /// The board is full with no line.
    Draw,
}

/// Game data that is not a move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaData {
    /// The name of the Black player, if given.
    pub black: Option<String>,
    /// The name of the White player, if given.
    pub white: Option<String>,
    /// When the game started, in UTC.
    pub created: OffsetDateTime,
    /// The result of the game. `Game` keeps this in step with the board.
    pub outcome: Outcome,
}

/// A game of freestyle Gomoku: the move list, the board at the review
/// cursor, and the metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Game {
    moves: Vec<Move>,
    board: Board,
    cursor: usize,
    meta: MetaData,
}

impl Game {
    /// A new empty game, started now.
    pub fn new() -> Game {
        Game::started_at(OffsetDateTime::now_utc())
    }

    /// A new empty game with a given start time.
    ///
    /// The record loader uses this to keep the original start time.
    pub fn started_at(created: OffsetDateTime) -> Game {
        Game {
            moves: Vec::new(),
            board: Board::new(),
            cursor: 0,
            meta: MetaData {
                black: None,
                white: None,
                created,
                outcome: Outcome::Ongoing,
            },
        }
    }

    /// The metadata of the game.
    pub fn meta(&self) -> &MetaData {
        &self.meta
    }

    /// Set the player names.
    pub fn set_players(&mut self, black: Option<String>, white: Option<String>) {
        self.meta.black = black;
        self.meta.white = white;
    }

    /// The stone at a point, if the point holds one.
    pub fn stone_at(&self, point: Move) -> Option<Color> {
        self.board.stone_at(point)
    }

    /// The stones on the visible board, in play order.
    ///
    /// Freestyle play alternates from Black, so the color follows the
    /// position in the list.
    pub fn stones(&self) -> impl Iterator<Item = (Move, Color)> + '_ {
        self.board.moves().iter().enumerate().map(|(index, &mv)| {
            let color = if index % 2 == 0 {
                Color::Black
            } else {
                Color::White
            };
            (mv, color)
        })
    }

    /// The status of the board at the review cursor.
    pub fn status(&self) -> Status {
        self.board.status()
    }

    /// The result of the game.
    ///
    /// The result describes the game, not the view. A rewound view keeps
    /// the result of the game.
    pub fn outcome(&self) -> Outcome {
        self.meta.outcome
    }

    /// The side to move at the review cursor.
    pub fn to_move(&self) -> Color {
        self.board.to_move()
    }

    /// The moves of the game.
    pub fn moves(&self) -> &[Move] {
        &self.moves
    }

    /// The number of moves in the game.
    pub fn len(&self) -> usize {
        self.moves.len()
    }

    /// True when the game has no move.
    pub fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }

    /// The number of moves applied to the visible board.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// True when the view is behind the latest move.
    pub fn is_rewound(&self) -> bool {
        self.cursor < self.moves.len()
    }

    /// The number of moves that a placement would delete.
    pub fn pending_truncation(&self) -> usize {
        self.moves.len() - self.cursor
    }

    /// The last move of the visible board.
    pub fn last_move(&self) -> Option<Move> {
        self.board.moves().last().copied()
    }

    /// Play a stone at `point`.
    ///
    /// When the view is rewound, every move after the cursor is deleted
    /// first. Read `pending_truncation` before you call this, and confirm
    /// with the user when it is not zero.
    ///
    /// # Errors
    /// `GameError::Occupied` if the point holds a stone.
    /// `GameError::GameOver` if the board at the cursor is finished.
    pub fn play(&mut self, point: Move) -> Result<(), GameError> {
        if self.stone_at(point).is_some() {
            return Err(GameError::Occupied);
        }
        if self.status() != Status::Ongoing {
            return Err(GameError::GameOver);
        }
        self.moves.truncate(self.cursor);
        self.board.play(point).map_err(GameError::from)?;
        self.moves.push(point);
        self.cursor += 1;
        self.meta.outcome = outcome_of(self.board.status());
        Ok(())
    }

    /// Delete the last move of the game. The move is gone.
    ///
    /// Returns false when the game has no move.
    pub fn undo(&mut self) -> bool {
        if self.moves.is_empty() {
            return false;
        }
        // Undo acts on the last move of the game, so leave a rewound view.
        self.live();
        self.moves.pop();
        // The board holds exactly `cursor` moves, and that is at least one.
        self.board.undo();
        self.cursor -= 1;
        self.meta.outcome = outcome_of(self.board.status());
        true
    }

    /// Step the view back one move. Returns false at the first move.
    pub fn rewind(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.board.undo();
        self.cursor -= 1;
        true
    }

    /// Step the view forward one move. Returns false at the latest move.
    pub fn forward(&mut self) -> bool {
        if self.cursor == self.moves.len() {
            return false;
        }
        let mv = self.moves[self.cursor];
        // The move was legal when it was played, and the board is that same
        // position, so the engine cannot reject it here.
        self.board.play(mv).expect("replaying a recorded move is legal");
        self.cursor += 1;
        true
    }

    /// Move the view to a move number, clamped to the game length.
    ///
    /// Returns false when the view does not move.
    pub fn seek(&mut self, index: usize) -> bool {
        let target = index.min(self.moves.len());
        if target == self.cursor {
            return false;
        }
        while self.cursor < target {
            self.forward();
        }
        while self.cursor > target {
            self.rewind();
        }
        true
    }

    /// Return the view to the latest move.
    pub fn live(&mut self) -> bool {
        self.seek(self.moves.len())
    }
}

impl Default for Game {
    fn default() -> Game {
        Game::new()
    }
}

/// Read the outcome from an engine status.
fn outcome_of(status: Status) -> Outcome {
    match status {
        Status::Ongoing => Outcome::Ongoing,
        Status::Won(winner) => Outcome::Won {
            winner,
            method: WinMethod::Five,
        },
        Status::Draw => Outcome::Draw,
    }
}
```

- [ ] **Step 4: Add the module to the crate root**

In `crates/core/src/lib.rs`, add `mod game;` after `mod error;`, and add:

```rust
pub use game::{Game, MetaData, Outcome, WinMethod};
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p gomoku-core`
Expected: 20 tests pass: 4 in `notation::tests` and 16 in `game::tests`.

- [ ] **Step 6: Run the gates**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --all && cargo test`
Expected: no warnings, no diff, all tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/core Cargo.lock
git commit -m "feat(core): add the Game aggregate with undo, rewind, and truncation"
```

`Cargo.lock` belongs in version control for an application, and Task 1's add
list missed it. It is added here, with the first task that builds against it.

---

### Task 3: Property tests for the game invariants

**Files:**
- Create: `crates/core/tests/invariants.rs`

**Interfaces:**
- Consumes: the whole public surface of `gomoku_core::Game` from Task 2.
- Produces: nothing that later tasks call. This task is the safety net for the invariants I1 to I6 in `docs/architecture/01_DOMAIN_MODEL.md`.

**Amendments from the Task 2 review — apply these two small edits first.** The
review approved Task 2 and rated both of these Minor. They are folded in here
because both concern the code that this task is about to exercise, and each is
a few lines.

1. In `crates/core/src/game.rs`, `play` truncates the move list before the
   fallible engine call. A rejected placement would therefore delete the moves
after the cursor and then report an error, which loses game data. Put the
engine call first:

Replace:

```rust
        self.moves.truncate(self.cursor);
        self.board.play(point).map_err(GameError::from)?;
        self.moves.push(point);
```

with:

```rust
        self.board.play(point).map_err(GameError::from)?;
        self.moves.truncate(self.cursor);
        self.moves.push(point);
```

The board is at the cursor, so playing on it first is correct, and a rejected
placement now changes nothing at all.

2. In `crates/core/src/game.rs`, in `mod tests`: invariant I6, that the outcome
describes the game and not the view, had no test. Every test that reads
`outcome` used a board that was live. Add:

```rust
    #[test]
    fn the_outcome_describes_the_game_not_the_view() {
        let mut game = near_win();
        assert!(game.rewind());
        assert_eq!(game.status(), Status::Ongoing);
        assert_eq!(
            game.outcome(),
            Outcome::Won {
                winner: Color::Black,
                method: WinMethod::Five
            }
        );
    }
```

The rewind leaves four black stones in row 7, so the board is ongoing while the
game is already won.

- [ ] **Step 1: Write the failing test**

`crates/core/tests/invariants.rs`:

```rust
//! Property tests for the invariants of `Game`.
//!
//! I1 cursor <= len
//! I2 the board equals a replay of moves[..cursor]
//! I3 the move list has no repeat and no point off the board
//! I4 the board holds exactly `cursor` moves
//! I5 the side to move follows the cursor parity
//! I6 the outcome follows the board at the latest position

use engine::{Board, Move, Status};
use gomoku_core::{Game, GameError, Outcome};
use proptest::prelude::*;

/// One step of a random walk through the public interface.
#[derive(Debug, Clone, Copy)]
enum Step {
    Play(Move),
    Undo,
    Rewind,
    Forward,
    Seek(u8),
    Live,
}

fn any_move() -> impl Strategy<Value = Move> {
    (0u8..15, 0u8..15).prop_map(|(row, col)| Move::new(row, col).expect("inside the board"))
}

fn any_step() -> impl Strategy<Value = Step> {
    prop_oneof![
        6 => any_move().prop_map(Step::Play),
        1 => Just(Step::Undo),
        1 => Just(Step::Rewind),
        1 => Just(Step::Forward),
        1 => (0u8..230).prop_map(Step::Seek),
        1 => Just(Step::Live),
    ]
}

fn apply(game: &mut Game, step: Step) {
    match step {
        Step::Play(point) => match game.play(point) {
            // A random placement can be refused: the cell may hold a stone,
            // or the board at the cursor may be finished. Both are valid.
            Ok(())
            | Err(GameError::Occupied)
            | Err(GameError::GameOver)
            | Err(GameError::Rejected(_)) => {}
        },
        Step::Undo => {
            game.undo();
        }
        Step::Rewind => {
            game.rewind();
        }
        Step::Forward => {
            game.forward();
        }
        Step::Seek(index) => {
            game.seek(usize::from(index));
        }
        Step::Live => {
            game.live();
        }
    }
}

/// A fresh board with the first `count` moves of `game` replayed.
fn replay(game: &Game, count: usize) -> Board {
    let mut board = Board::new();
    for mv in &game.moves()[..count] {
        board.play(*mv).expect("a recorded move replays");
    }
    board
}

/// The outcome that the board at the latest position implies.
fn expected_outcome(game: &Game) -> Outcome {
    match replay(game, game.len()).status() {
        Status::Ongoing => Outcome::Ongoing,
        Status::Won(winner) => Outcome::Won {
            winner,
            method: gomoku_core::WinMethod::Five,
        },
        Status::Draw => Outcome::Draw,
    }
}

fn check(game: &Game) {
    assert!(
        game.cursor() <= game.len(),
        "I1: the cursor is inside the move list"
    );
    assert_eq!(
        game.pending_truncation(),
        game.len() - game.cursor(),
        "the truncation count is the length of the tail"
    );

    // I2: the visible board is the replay of the first `cursor` moves.
    let visible = replay(game, game.cursor());
    for row in 0..15u8 {
        for col in 0..15u8 {
            let point = Move::new(row, col).expect("row and col are inside the board");
            assert_eq!(
                game.stone_at(point),
                visible.stone_at(point),
                "I2: the visible board differs at row {row} column {col}"
            );
        }
    }
    assert_eq!(game.status(), visible.status(), "I2: the status of the replay");
    assert_eq!(
        game.to_move(),
        visible.to_move(),
        "I5: the side to move of the replay"
    );

    // I6: the outcome describes the game, not the view.
    assert_eq!(
        game.outcome(),
        expected_outcome(game),
        "I6: the outcome of the game"
    );

    // I3: the move list has no repeat.
    let mut seen = Vec::with_capacity(game.len());
    for mv in game.moves() {
        assert!(!seen.contains(mv), "I3: no repeated point");
        seen.push(*mv);
    }
}

proptest! {
    #[test]
    fn every_reachable_state_holds_the_invariants(steps in prop::collection::vec(any_step(), 0..60)) {
        let mut game = Game::new();
        check(&game);
        for step in steps {
            apply(&mut game, step);
            check(&game);
        }
    }

    #[test]
    fn rewind_then_forward_restores_the_same_position(steps in prop::collection::vec(any_step(), 0..40)) {
        let mut game = Game::new();
        for step in steps {
            apply(&mut game, step);
        }
        // Forward stops at the latest move, so the cycle restores the live
        // position. The walk can leave a rewound view, so compare against the
        // live state.
        let mut live_before = game.clone();
        live_before.live();
        while game.rewind() {}
        while game.forward() {}
        prop_assert_eq!(game, live_before);
    }

    #[test]
    fn play_while_rewound_truncates_the_tail(steps in prop::collection::vec(any_step(), 0..40), point in any_move()) {
        let mut game = Game::new();
        for step in steps {
            apply(&mut game, step);
        }
        let cursor = game.cursor();
        if game.stone_at(point).is_none() && game.status() == Status::Ongoing && game.play(point).is_ok() {
            prop_assert_eq!(game.len(), cursor + 1);
            prop_assert_eq!(game.cursor(), cursor + 1);
            prop_assert!(!game.is_rewound());
            prop_assert_eq!(game.last_move(), Some(point));
            prop_assert_eq!(game.outcome(), expected_outcome(&game));
        }
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p gomoku-core --test invariants`
Expected: a compile error on `gomoku_core::WinMethod` only if the re-export is missing; otherwise the tests run. If the file compiles, the first run is the baseline, and any assertion failure is a real defect in Task 2. Report a failure in Task 2 as a finding, not by editing `game.rs` here.

Note for the implementer: `Game` must derive `Clone` and `PartialEq` for the second property. Task 2 already derives both. `prop_assert_eq!` needs `Debug`, which `Game` also derives.

- [ ] **Step 3: Run the tests to verify they pass**

Run: `cargo test -p gomoku-core --test invariants`
Expected: 3 property tests pass. The default is 256 cases per property.

- [ ] **Step 4: Strengthen the run once**

Run: `PROPTEST_CASES=2000 cargo test -p gomoku-core --test invariants`
Expected: 3 property tests pass with 2000 cases each. Report the result in the report file.

- [ ] **Step 5: Run the gates**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --all && cargo test`
Expected: no warnings, no diff, all tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/core/tests/invariants.rs
git commit -m "test(core): add property tests for the game invariants"
```

---

### Task 4: The record schema and its validation

**Files:**
- Create: `crates/core/src/record.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Consumes: `Game`, `Outcome`, `WinMethod` from Task 2; `RecordError` from Task 1.
- Produces: `gomoku_core::{Record, Players, StoredOutcome, StoredColor, StoredMethod, FORMAT_MARKER, FORMAT_VERSION, BOARD_SIZE}` with `Record::{from_game, to_json, from_json, into_game}`.

- [ ] **Step 1: Write the failing tests**

Add to the end of `crates/core/src/record.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn point(row: u8, col: u8) -> Move {
        Move::new(row, col).expect("row and col are inside the board")
    }

    fn played_game() -> Game {
        let mut game = Game::started_at(datetime!(2026-09-22 18:04:11 UTC));
        game.set_players(Some("Wolfie".to_string()), Some("Anna".to_string()));
        for (row, col) in [(7, 7), (7, 8), (8, 8), (6, 6)] {
            game.play(point(row, col)).expect("the cell is empty");
        }
        game
    }

    fn won_game() -> Game {
        let mut game = Game::started_at(datetime!(2026-09-22 18:04:11 UTC));
        for index in 0..4 {
            game.play(point(7, 3 + index)).expect("the cell is empty");
            game.play(point(3 + index, index)).expect("the cell is empty");
        }
        game.play(point(7, 7)).expect("the cell is empty");
        game
    }

    fn record() -> Record {
        Record::from_game(&played_game())
    }

    #[test]
    fn a_record_round_trips_through_json() {
        let original = record();
        let text = original.to_json().expect("the record encodes");
        let parsed = Record::from_json(&text).expect("the text parses");
        assert_eq!(parsed, original);

        let game = parsed.into_game().expect("the record is valid");
        assert_eq!(game.moves(), played_game().moves());
        assert_eq!(game.cursor(), 4);
        assert_eq!(game.meta().black.as_deref(), Some("Wolfie"));
        assert_eq!(game.meta().white.as_deref(), Some("Anna"));
        assert_eq!(game.meta().created, datetime!(2026-09-22 18:04:11 UTC));
        assert_eq!(game.outcome(), Outcome::Ongoing);
    }

    #[test]
    fn a_won_game_round_trips_with_its_result() {
        let record = Record::from_game(&won_game());
        assert_eq!(
            record.result,
            StoredOutcome::Won {
                winner: StoredColor::Black,
                method: StoredMethod::Five
            }
        );
        let game = Record::from_json(&record.to_json().expect("encodes"))
            .expect("parses")
            .into_game()
            .expect("valid");
        assert_eq!(
            game.outcome(),
            Outcome::Won {
                winner: Color::Black,
                method: WinMethod::Five
            }
        );
    }

    #[test]
    fn a_draw_maps_both_ways() {
        assert_eq!(Outcome::from(StoredOutcome::Draw), Outcome::Draw);
        assert_eq!(StoredOutcome::from(Outcome::Draw), StoredOutcome::Draw);
    }

    #[test]
    fn the_moves_are_stored_column_first() {
        let list = record().moves;
        assert_eq!(list, vec![[7, 7], [8, 7], [8, 8], [6, 6]]);
    }

    #[test]
    fn an_unknown_field_is_ignored() {
        let original = record();
        let text = original.to_json().expect("encodes");
        let mut value: serde_json::Value = serde_json::from_str(&text).expect("parses");
        value["a_field_from_a_later_version"] = serde_json::json!(42);
        let text = serde_json::to_string(&value).expect("encodes");
        let parsed = Record::from_json(&text).expect("parses");
        assert_eq!(parsed, original);
    }

    #[test]
    fn a_record_refuses_another_format_marker() {
        let mut record = record();
        record.format = "something-else".to_string();
        assert!(matches!(
            record.into_game(),
            Err(RecordError::WrongFormat { .. })
        ));
    }

    #[test]
    fn a_record_refuses_a_newer_version() {
        let mut record = record();
        record.version = 2;
        assert!(matches!(
            record.into_game(),
            Err(RecordError::UnsupportedVersion(2))
        ));
    }

    #[test]
    fn a_record_refuses_another_board_size() {
        let mut record = record();
        record.size = 19;
        assert!(matches!(
            record.into_game(),
            Err(RecordError::UnsupportedSize(19))
        ));
    }

    #[test]
    fn a_record_refuses_another_rule_set() {
        let mut record = record();
        record.ruleset = "renju".to_string();
        assert!(matches!(
            record.into_game(),
            Err(RecordError::UnsupportedRuleset(_))
        ));
    }

    #[test]
    fn a_record_refuses_more_moves_than_the_board_has_cells() {
        let mut record = record();
        record.moves = vec![[0, 0]; 226];
        assert!(matches!(
            record.into_game(),
            Err(RecordError::TooManyMoves { count: 226 })
        ));
    }

    #[test]
    fn a_record_refuses_a_point_outside_the_board() {
        let mut record = record();
        record.moves.push([15, 0]);
        assert!(matches!(
            record.into_game(),
            Err(RecordError::PointOutOfRange { index: 4 })
        ));
    }

    #[test]
    fn a_record_refuses_a_repeated_point() {
        let mut record = record();
        record.moves.push([7, 7]);
        assert!(matches!(
            record.into_game(),
            Err(RecordError::RepeatedPoint { index: 4 })
        ));
    }

    #[test]
    fn a_record_refuses_a_move_after_the_game_ended() {
        let mut record = Record::from_game(&won_game());
        record.moves.push([10, 10]);
        assert!(matches!(
            record.into_game(),
            Err(RecordError::IllegalMove { index: 9 })
        ));
    }

    #[test]
    fn a_record_refuses_a_result_that_disagrees_with_the_moves() {
        let mut record = record();
        record.result = StoredOutcome::Won {
            winner: StoredColor::Black,
            method: StoredMethod::Five,
        };
        assert!(matches!(record.into_game(), Err(RecordError::ResultMismatch)));
    }

    #[test]
    fn damaged_text_is_a_parse_error() {
        assert!(matches!(
            Record::from_json("{ not json"),
            Err(RecordError::Parse(_))
        ));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p gomoku-core record`
Expected: a compile error, because `Record` does not exist. That is the failure for this step.

- [ ] **Step 3: Write the implementation**

Replace the whole of `crates/core/src/record.rs` with:

```rust
//! The game record: schema version 1, stored as JSON.

use engine::{Color, Move};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::error::RecordError;
use crate::game::{Game, Outcome, WinMethod};

/// The value of the `format` field of a record.
pub const FORMAT_MARKER: &str = "gomoku-gui/record";
/// The record schema version this build writes and reads.
pub const FORMAT_VERSION: u32 = 1;
/// The board size this application plays.
pub const BOARD_SIZE: u32 = 15;
/// The rule set this application plays.
pub const RULESET: &str = "freestyle";
/// The number of cells on a 15x15 board.
const CELLS: usize = 225;

/// The player names of a record.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Players {
    /// The name of the Black player, if given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub black: Option<String>,
    /// The name of the White player, if given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub white: Option<String>,
}

/// A stone color in a record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StoredColor {
    /// Black, the first player.
    Black,
    /// White, the second player.
    White,
}

/// A way to win a game in a record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StoredMethod {
    /// Five or more stones in a line.
    Five,
}

/// The result of a game in a record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum StoredOutcome {
    /// The game is still open.
    Ongoing,
    /// A player has won.
    Won {
        /// The winner.
        winner: StoredColor,
        /// How the game was won.
        method: StoredMethod,
    },
    /// The board is full with no line.
    Draw,
}

impl From<Color> for StoredColor {
    fn from(color: Color) -> StoredColor {
        match color {
            Color::Black => StoredColor::Black,
            Color::White => StoredColor::White,
        }
    }
}

impl From<StoredColor> for Color {
    fn from(color: StoredColor) -> Color {
        match color {
            StoredColor::Black => Color::Black,
            StoredColor::White => Color::White,
        }
    }
}

impl From<Outcome> for StoredOutcome {
    fn from(outcome: Outcome) -> StoredOutcome {
        match outcome {
            Outcome::Ongoing => StoredOutcome::Ongoing,
            Outcome::Draw => StoredOutcome::Draw,
            Outcome::Won { winner, .. } => StoredOutcome::Won {
                winner: winner.into(),
                method: StoredMethod::Five,
            },
        }
    }
}

impl From<StoredOutcome> for Outcome {
    fn from(outcome: StoredOutcome) -> Outcome {
        match outcome {
            StoredOutcome::Ongoing => Outcome::Ongoing,
            StoredOutcome::Draw => Outcome::Draw,
            StoredOutcome::Won { winner, .. } => Outcome::Won {
                winner: winner.into(),
                method: WinMethod::Five,
            },
        }
    }
}

/// A game record as it is stored on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// The format marker. Always [`FORMAT_MARKER`].
    pub format: String,
    /// The schema version. Always [`FORMAT_VERSION`].
    pub version: u32,
    /// The board size. Always [`BOARD_SIZE`].
    pub size: u32,
    /// The rule set. Always `freestyle`.
    pub ruleset: String,
    /// The player names.
    #[serde(default)]
    pub players: Players,
    /// When the game started, in UTC.
    pub created: OffsetDateTime,
    /// The result of the game.
    pub result: StoredOutcome,
    /// The moves as `[column, row]` pairs. Column 0 is left, row 0 is the top.
    pub moves: Vec<[u8; 2]>,
}

impl Record {
    /// Build a record from a game.
    pub fn from_game(game: &Game) -> Record {
        let meta = game.meta();
        Record {
            format: FORMAT_MARKER.to_string(),
            version: FORMAT_VERSION,
            size: BOARD_SIZE,
            ruleset: RULESET.to_string(),
            players: Players {
                black: meta.black.clone(),
                white: meta.white.clone(),
            },
            created: meta.created,
            result: meta.outcome.into(),
            moves: game.moves().iter().map(|mv| [mv.col(), mv.row()]).collect(),
        }
    }

    /// Encode the record as pretty JSON with a trailing newline.
    ///
    /// # Errors
    /// `RecordError::Parse` if the record cannot be encoded.
    pub fn to_json(&self) -> Result<String, RecordError> {
        let mut text = serde_json::to_string_pretty(self)
            .map_err(|err| RecordError::Parse(err.to_string()))?;
        text.push('\n');
        Ok(text)
    }

    /// Parse a record from JSON text.
    ///
    /// This checks the syntax only. `into_game` checks the contents.
    ///
    /// # Errors
    /// `RecordError::Parse` if the text is not a record.
    pub fn from_json(text: &str) -> Result<Record, RecordError> {
        serde_json::from_str(text).map_err(|err| RecordError::Parse(err.to_string()))
    }

    /// Turn the record into a game, after checking every field.
    ///
    /// # Errors
    /// The `RecordError` of the first check that fails, in the order
    /// documented in `docs/architecture/03_PERSISTENCE.md`.
    pub fn into_game(self) -> Result<Game, RecordError> {
        self.check_header()?;
        let mut game = Game::started_at(self.created);
        game.set_players(self.players.black, self.players.white);
        for (index, &[col, row]) in self.moves.iter().enumerate() {
            let point = Move::new(row, col).ok_or(RecordError::PointOutOfRange { index })?;
            if game.stone_at(point).is_some() {
                return Err(RecordError::RepeatedPoint { index });
            }
            game.play(point)
                .map_err(|_| RecordError::IllegalMove { index })?;
        }
        if Outcome::from(self.result) != game.outcome() {
            return Err(RecordError::ResultMismatch);
        }
        Ok(game)
    }

    /// Check the header fields, which do not depend on the moves.
    fn check_header(&self) -> Result<(), RecordError> {
        if self.format != FORMAT_MARKER {
            return Err(RecordError::WrongFormat {
                found: self.format.clone(),
            });
        }
        if self.version > FORMAT_VERSION {
            return Err(RecordError::UnsupportedVersion(self.version));
        }
        if self.size != BOARD_SIZE {
            return Err(RecordError::UnsupportedSize(self.size));
        }
        if self.ruleset != RULESET {
            return Err(RecordError::UnsupportedRuleset(self.ruleset.clone()));
        }
        if self.moves.len() > CELLS {
            return Err(RecordError::TooManyMoves {
                count: self.moves.len(),
            });
        }
        Ok(())
    }
}
```

- [ ] **Step 4: Add the module to the crate root**

In `crates/core/src/lib.rs`, add `mod record;` after `mod notation;`, and add:

```rust
pub use record::{
    BOARD_SIZE, FORMAT_MARKER, FORMAT_VERSION, Players, Record, StoredColor, StoredMethod,
    StoredOutcome,
};
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p gomoku-core`
Expected: all tests pass, including 13 tests in `record::tests`.

If the timestamp in the JSON is not the form the plan expects, that is acceptable: RFC 3339 allows both `Z` and `+00:00`. Do not change the code for it. Record what the encoder produced in your report file, because Task 5 freezes it in the golden file.

- [ ] **Step 6: Run the gates**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --all && cargo test`
Expected: no warnings, no diff, all tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/core
git commit -m "feat(core): add the versioned record schema and its validation"
```

---

### Task 5: File storage, the golden file, and the damaged-file cases

**Files:**
- Create: `crates/core/src/storage.rs`
- Create: `crates/core/tests/record_format.rs`
- Create: `crates/core/tests/golden/hotseat-v1.json`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Consumes: `Record` from Task 4; `Game` from Task 2.
- Produces: `gomoku_core::{save, load}` with the signatures `save(path: &Path, game: &Game) -> Result<(), RecordError>` and `load(path: &Path) -> Result<Game, RecordError>`, and `gomoku_core::write_atomic(path: &Path, text: &str) -> std::io::Result<()>`, which Task 6 uses for the settings file.

- [ ] **Step 1: Write the failing test**

`crates/core/tests/record_format.rs`:

```rust
//! File-level tests for the record format: the frozen golden file, a round
//! trip through the file system, and the damaged-file cases.

use std::path::PathBuf;

use engine::Move;
use gomoku_core::{Game, Record, RecordError, load, save};
use time::macros::datetime;

/// The golden file is frozen. A change to the schema changes these bytes,
/// and that must be a deliberate decision with a version bump.
const GOLDEN: &str = include_str!("golden/hotseat-v1.json");

fn golden_game() -> Game {
    let mut game = Game::started_at(datetime!(2026-09-22 18:04:11 UTC));
    game.set_players(Some("Wolfie".to_string()), Some("Anna".to_string()));
    for (row, col) in [(7, 7), (7, 8), (8, 8), (6, 6)] {
        game.play(Move::new(row, col).expect("inside the board"))
            .expect("the cell is empty");
    }
    game
}

#[test]
fn the_encoder_still_writes_the_golden_file() {
    let text = Record::from_game(&golden_game())
        .to_json()
        .expect("the record encodes");
    assert_eq!(text, GOLDEN, "the record schema changed");
}

#[test]
fn the_golden_file_loads_and_validates() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/hotseat-v1.json");
    let game = load(&path).expect("the golden file is a valid record");
    assert_eq!(game.len(), 4);
    assert_eq!(game.moves(), golden_game().moves());
    assert_eq!(game.meta().black.as_deref(), Some("Wolfie"));
}

#[test]
fn a_saved_game_loads_back_unchanged() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path().join("game.json");
    let game = golden_game();

    save(&path, &game).expect("the game saves");
    let loaded = load(&path).expect("the game loads");

    assert_eq!(loaded.moves(), game.moves());
    assert_eq!(loaded.cursor(), game.cursor());
    assert_eq!(loaded.meta(), game.meta());
}

#[test]
fn a_save_leaves_no_temporary_file_behind() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path().join("game.json");
    save(&path, &golden_game()).expect("the game saves");

    assert!(path.exists(), "the record file exists");
    let leftovers: Vec<_> = std::fs::read_dir(directory.path())
        .expect("the directory reads")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "temporary files left behind: {leftovers:?}");
}

#[test]
fn a_save_replaces_an_older_file() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path().join("game.json");
    let mut game = golden_game();
    save(&path, &game).expect("the first save");

    game.play(Move::new(10, 10).expect("inside the board"))
        .expect("the cell is empty");
    save(&path, &game).expect("the second save");

    let loaded = load(&path).expect("the game loads");
    assert_eq!(loaded.len(), 5);
}

#[test]
fn a_missing_file_is_an_io_error() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path().join("absent.json");
    let error = load(&path).expect_err("the file is absent");
    assert!(matches!(error, RecordError::Io { .. }));
}

#[test]
fn a_damaged_file_is_a_parse_error() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path().join("damaged.json");
    std::fs::write(&path, "{\"format\": \"gomoku-gui/record\",").expect("the file writes");
    let error = load(&path).expect_err("the text is not a record");
    assert!(matches!(error, RecordError::Parse(_)));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p gomoku-core --test record_format`
Expected: a compile error, because `golden/hotseat-v1.json` is absent and `storage` does not exist. That is the failure for this step.

- [ ] **Step 3: Write the storage implementation**

`crates/core/src/storage.rs`:

```rust
//! Reading and writing files.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::error::RecordError;
use crate::game::Game;
use crate::record::Record;

/// Write text to `path` through a temporary file and a rename.
///
/// A rename inside one file system is atomic, so a crash leaves either the
/// old file or the new file, never a half-written file.
///
/// # Errors
/// The error from the file system.
pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let temporary = temporary_path(path);
    let mut file = fs::File::create(&temporary)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary, path)
}

/// Read a whole file as text.
///
/// # Errors
/// The error from the file system.
pub fn read_to_string(path: &Path) -> std::io::Result<String> {
    fs::read_to_string(path)
}

/// Save a game to `path`.
///
/// # Errors
/// `RecordError::Io` if the file cannot be written.
/// `RecordError::Parse` if the record cannot be encoded.
pub fn save(path: &Path, game: &Game) -> Result<(), RecordError> {
    let text = Record::from_game(game).to_json()?;
    write_atomic(path, &text).map_err(|source| RecordError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Load a game from `path` and check every field of the record.
///
/// # Errors
/// `RecordError::Io` if the file cannot be read.
/// `RecordError::Parse` if the text is not a record.
/// A validation error if the contents are wrong.
pub fn load(path: &Path) -> Result<Game, RecordError> {
    let text = read_to_string(path).map_err(|source| RecordError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Record::from_json(&text)?.into_game()
}

/// The path of the temporary file that `write_atomic` uses.
fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}
```

- [ ] **Step 4: Create the golden file and verify it**

Create `crates/core/tests/golden/hotseat-v1.json` by hand from the schema in
`docs/architecture/03_PERSISTENCE.md`: the format marker, version 1, size 15,
the rule set, both player names, the start time of `golden_game`, a result of
`{"kind": "ongoing"}`, and four moves as `[column, row]` pairs, namely
`[7, 7]`, `[8, 7]`, `[8, 8]`, and `[6, 6]`.

Run: `cargo test -p gomoku-core --test record_format the_encoder_still_writes_the_golden_file`

If the test fails, `assert_eq!` prints both texts. The four-space indent of
`to_string_pretty` matters, and so does the exact rendering of the timestamp.
Copy the actual text from the failure output into the golden file, then run
the test again.

Expected: the test passes. The file is now the frozen bytes of the schema,
and every later change to the encoder fails this test on purpose.

Write the full content of the golden file into your report file. The reviewer
checks it against the schema, because a golden file that the code generated
proves drift and not correctness.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p gomoku-core`
Expected: all tests pass, including 7 tests in `record_format` and the 13 tests in `record::tests`.

- [ ] **Step 6: Add the module to the crate root**

In `crates/core/src/lib.rs`, add `mod storage;` after `mod record;`, and add:

```rust
pub use storage::{load, save, write_atomic};
```

- [ ] **Step 7: Run the gates**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --all && cargo test`
Expected: no warnings, no diff, all tests pass.

- [ ] **Step 8: Commit**

```bash
git add crates/core
git commit -m "feat(core): add atomic file storage and freeze the record golden file"
```

---

### Task 6: The settings schema

**Files:**
- Create: `crates/core/src/config.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Consumes: `ConfigError` from Task 1, `write_atomic` and `read_to_string` from Task 5.
- Produces: `gomoku_core::{Settings, WindowSettings, ViewSettings, OverlaySettings, BoardSettings, StoneSettings, AudioSettings, FileSettings, ConfigLoad, ConfigNotice, load_settings, save_settings}`.

- [ ] **Step 1: Write the failing tests**

Add to the end of `crates/core/src/config.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn settings_file() -> (tempfile::TempDir, std::path::PathBuf) {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("config.toml");
        (directory, path)
    }

    #[test]
    fn the_defaults_match_the_specification() {
        let settings = Settings::default();
        assert_eq!(settings.window.width, 1180.0);
        assert_eq!(settings.window.height, 900.0);
        assert_eq!(settings.view.pixels_per_cell, 0.0);
        assert_eq!(settings.view.center, [7.0, 7.0]);
        assert!(!settings.view.flipped);
        assert!(settings.overlays.coordinates);
        assert!(!settings.overlays.move_numbers);
        assert!(settings.overlays.last_move);
        assert!(settings.overlays.win_line);
        assert_eq!(settings.board.material, "aged_wood");
        assert_eq!(settings.stones.set, "slate_shell");
        assert!(settings.audio.enabled);
        assert_eq!(settings.audio.volume, 0.7);
        assert!(settings.files.recent.is_empty());
    }

    #[test]
    fn a_missing_file_gives_the_defaults_and_a_notice() {
        let (_directory, path) = settings_file();
        let loaded = load_settings(&path);
        assert_eq!(loaded.settings, Settings::default());
        assert_eq!(loaded.notice, Some(ConfigNotice::Missing));
    }

    #[test]
    fn the_settings_round_trip_through_a_file() {
        let (_directory, path) = settings_file();
        let settings = Settings {
            view: ViewSettings {
                pixels_per_cell: 42.5,
                flipped: true,
                ..ViewSettings::default()
            },
            overlays: OverlaySettings {
                move_numbers: true,
                ..OverlaySettings::default()
            },
            board: BoardSettings {
                material: "walnut".to_string(),
            },
            audio: AudioSettings {
                volume: 0.25,
                ..AudioSettings::default()
            },
            ..Settings::default()
        };

        save_settings(&path, &settings).expect("the settings save");
        let loaded = load_settings(&path);
        assert_eq!(loaded.settings, settings);
        assert_eq!(loaded.notice, None);
    }

    #[test]
    fn a_partial_file_keeps_the_defaults_for_the_missing_fields() {
        let (_directory, path) = settings_file();
        std::fs::write(&path, "[audio]\nvolume = 0.5\n").expect("the file writes");

        let loaded = load_settings(&path);
        assert_eq!(loaded.notice, None);
        assert_eq!(loaded.settings.audio.volume, 0.5);
        assert!(loaded.settings.audio.enabled, "the missing field keeps its default");
        assert_eq!(loaded.settings.board.material, "aged_wood");
    }

    #[test]
    fn a_damaged_file_is_quarantined_and_the_defaults_apply() {
        let (_directory, path) = settings_file();
        std::fs::write(&path, "this is not toml === ").expect("the file writes");

        let loaded = load_settings(&path);
        match loaded.notice {
            Some(ConfigNotice::Corrupt { backup }) => assert!(backup.exists()),
            other => panic!("expected a corrupt notice, got {other:?}"),
        }
        assert_eq!(loaded.settings, Settings::default());
        assert!(!path.exists(), "the damaged file was moved away");
    }

    #[test]
    fn sanitize_clamps_the_volume() {
        let mut settings = Settings {
            audio: AudioSettings {
                volume: 4.2,
                ..AudioSettings::default()
            },
            ..Settings::default()
        };
        settings.sanitize();
        assert_eq!(settings.audio.volume, 1.0);

        settings.audio.volume = -1.0;
        settings.sanitize();
        assert_eq!(settings.audio.volume, 0.0);
    }

    #[test]
    fn sanitize_caps_and_deduplicates_the_recent_list() {
        let mut recent: Vec<std::path::PathBuf> = (0..12)
            .map(|index| std::path::PathBuf::from(format!("/games/{index}.json")))
            .collect();
        recent.push(std::path::PathBuf::from("/games/0.json"));
        let mut settings = Settings {
            files: FileSettings {
                recent,
                ..FileSettings::default()
            },
            ..Settings::default()
        };

        settings.sanitize();

        assert_eq!(settings.files.recent.len(), MAX_RECENT);
        assert_eq!(
            settings.files.recent[0],
            std::path::PathBuf::from("/games/0.json")
        );
        assert_eq!(
            settings.files.recent.last().expect("not empty"),
            &std::path::PathBuf::from("/games/9.json")
        );
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p gomoku-core config`
Expected: a compile error, because `Settings` does not exist. That is the failure for this step.

- [ ] **Step 3: Write the implementation**

Replace the whole of `crates/core/src/config.rs` with:

```rust
//! The settings file. A missing file is normal; the defaults apply.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::ConfigError;
use crate::storage::{read_to_string, write_atomic};

/// The most recent record paths that are kept.
pub const MAX_RECENT: usize = 10;

/// Window geometry and state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowSettings {
    /// Window width in logical pixels.
    pub width: f32,
    /// Window height in logical pixels.
    pub height: f32,
    /// Window x position in logical pixels, if it is known.
    pub x: Option<f32>,
    /// Window y position in logical pixels, if it is known.
    pub y: Option<f32>,
    /// True when the window was maximised.
    pub maximized: bool,
}

impl Default for WindowSettings {
    fn default() -> WindowSettings {
        WindowSettings {
            width: 1180.0,
            height: 900.0,
            x: None,
            y: None,
            maximized: false,
        }
    }
}

/// The zoom and the position of the view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewSettings {
    /// Pixels per board cell. Zero means "fit the board at start-up".
    pub pixels_per_cell: f32,
    /// The board point at the centre of the view, in cells.
    pub center: [f32; 2],
    /// True when the board is shown from the other side.
    pub flipped: bool,
}

impl Default for ViewSettings {
    fn default() -> ViewSettings {
        ViewSettings {
            pixels_per_cell: 0.0,
            center: [7.0, 7.0],
            flipped: false,
        }
    }
}

/// The four board overlays.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlaySettings {
    /// Show the coordinate labels.
    pub coordinates: bool,
    /// Show the move numbers on the stones.
    pub move_numbers: bool,
    /// Mark the last move.
    pub last_move: bool,
    /// Mark the winning line.
    pub win_line: bool,
}

impl Default for OverlaySettings {
    fn default() -> OverlaySettings {
        OverlaySettings {
            coordinates: true,
            move_numbers: false,
            last_move: true,
            win_line: true,
        }
    }
}

/// The board material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BoardSettings {
    /// The name of the material preset.
    pub material: String,
}

impl Default for BoardSettings {
    fn default() -> BoardSettings {
        BoardSettings {
            material: "aged_wood".to_string(),
        }
    }
}

/// The stone set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StoneSettings {
    /// The name of the stone set.
    pub set: String,
}

impl Default for StoneSettings {
    fn default() -> StoneSettings {
        StoneSettings {
            set: "slate_shell".to_string(),
        }
    }
}

/// The sound settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    /// True when the stone knock plays.
    pub enabled: bool,
    /// The volume, from 0.0 to 1.0.
    pub volume: f32,
}

impl Default for AudioSettings {
    fn default() -> AudioSettings {
        AudioSettings {
            enabled: true,
            volume: 0.7,
        }
    }
}

/// The file settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileSettings {
    /// The directory of the last save or open.
    pub last_directory: Option<PathBuf>,
    /// The most recently used record paths, newest first.
    pub recent: Vec<PathBuf>,
}

impl Default for FileSettings {
    fn default() -> FileSettings {
        FileSettings {
            last_directory: None,
            recent: Vec::new(),
        }
    }
}

/// The settings of the application.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Settings {
    /// Window geometry and state.
    pub window: WindowSettings,
    /// Zoom and position of the view.
    pub view: ViewSettings,
    /// The board overlays.
    pub overlays: OverlaySettings,
    /// The board material.
    pub board: BoardSettings,
    /// The stone set.
    pub stones: StoneSettings,
    /// The sound settings.
    pub audio: AudioSettings,
    /// The file settings.
    pub files: FileSettings,
}

impl Settings {
    /// Clamp the values that a hand-edited file can put out of range.
    pub fn sanitize(&mut self) {
        self.audio.volume = self.audio.volume.clamp(0.0, 1.0);

        let mut recent: Vec<PathBuf> = Vec::with_capacity(self.files.recent.len());
        for path in &self.files.recent {
            if recent.len() < MAX_RECENT && !recent.contains(path) {
                recent.push(path.clone());
            }
        }
        self.files.recent = recent;
    }
}

/// What the caller should tell the user about a load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigNotice {
    /// There was no settings file.
    Missing,
    /// The settings file could not be read. It was moved to `backup`.
    Corrupt {
        /// The path of the quarantined file.
        backup: PathBuf,
    },
}

/// The result of a load.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigLoad {
    /// The settings, either read from the file or the defaults.
    pub settings: Settings,
    /// Anything the caller should report.
    pub notice: Option<ConfigNotice>,
}

/// Read the settings from `path`.
///
/// A missing file gives the defaults. A damaged file is moved aside so that
/// the next start is clean. This function never fails.
pub fn load_settings(path: &Path) -> ConfigLoad {
    let Ok(text) = fs::read_to_string(path) else {
        return ConfigLoad {
            settings: Settings::default(),
            notice: Some(ConfigNotice::Missing),
        };
    };

    match toml::from_str::<Settings>(&text) {
        Ok(mut settings) => {
            settings.sanitize();
            ConfigLoad {
                settings,
                notice: None,
            }
        }
        Err(_) => {
            let backup = path.with_extension("toml.corrupt");
            let notice = match fs::rename(path, &backup) {
                Ok(()) => ConfigNotice::Corrupt { backup },
                Err(_) => ConfigNotice::Missing,
            };
            ConfigLoad {
                settings: Settings::default(),
                notice: Some(notice),
            }
        }
    }
}

/// Write the settings to `path`.
///
/// # Errors
/// `ConfigError::Io` if the file cannot be written.
/// `ConfigError::Encode` if the settings cannot be encoded.
pub fn save_settings(path: &Path, settings: &Settings) -> Result<(), ConfigError> {
    let text = toml::to_string(settings).map_err(|err| ConfigError::Encode(err.to_string()))?;
    write_atomic(path, &text).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })
}
```

Note: `toml::to_string` and `toml::from_str` are the documented entry points of the `toml` crate at version 1. If either name does not resolve in the resolved version, use the equivalent documented path (`toml::ser::to_string`, `toml::de::from_str`) and record the change in the report file. Do not add a dependency.

- [ ] **Step 4: Add the module to the crate root**

In `crates/core/src/lib.rs`, add `mod config;` after `mod storage;` (keep the list alphabetical: `config`, `error`, `game`, `notation`, `record`, `storage`), and add:

```rust
pub use config::{
    AudioSettings, BoardSettings, ConfigLoad, ConfigNotice, FileSettings, MAX_RECENT,
    OverlaySettings, Settings, StoneSettings, ViewSettings, WindowSettings, load_settings,
    save_settings,
};
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p gomoku-core`
Expected: all tests pass, including 7 tests in `config::tests`.

- [ ] **Step 6: Run the gates**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --all && cargo test`
Expected: no warnings, no diff, all tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/core
git commit -m "feat(core): add the settings schema with quarantine and sanitising"
```

---

### Task 7: Bring the documentation back in step

**Files:**
- Modify: `docs/architecture/02_CORE_LOGIC.md`
- Modify: `docs/architecture/03_PERSISTENCE.md`
- Modify: `docs/decisions/ADR-006-error-strategy.md`
- Modify: `README.md`

**Interfaces:**
- Consumes: the code and the tests from Tasks 1 to 6.
- Produces: documentation that matches the code. No later task depends on these edits.

Reason for this task: Task 4 dropped the redundant `PrematureResult` check, because a move after a win is already an `IllegalMove`. The design documents still describe it. A document that describes a check the code does not make is a defect.

- [ ] **Step 1: Remove the redundant check from the core logic document**

In `docs/architecture/02_CORE_LOGIC.md`, in the "Validation on load" table, delete this row:

```
| 10 | a result of `Won` or `Draw` appears only on the last move of the game | `RecordError::PrematureResult` |
```

And in the paragraph below the table, replace:

```
Check 8 catches a file whose moves are already illegal at move 20, and it also
catches a game that continues after a win. Check 4 leaves room for a future
ruleset field without a version bump.
```

with:

```
Check 8 catches a file whose moves are already illegal at move 20, and it also
catches a game that continues after a win. Check 4 leaves room for a future
ruleset field without a version bump. Check 9 makes a separate check for a
result that the moves do not produce unnecessary: a game that continues after
a win already fails check 8.
```

- [ ] **Step 2: Remove the same check from the persistence document**

In `docs/architecture/03_PERSISTENCE.md`, in the error list, delete these two lines:

```
    ResultMismatch,
    PrematureResult { index: usize },
```

and insert this line in their place:

```
    ResultMismatch,
```

- [ ] **Step 3: Remove the check row and the test row**

In `docs/architecture/03_PERSISTENCE.md`, in the numbered check table, delete the row:

```
| 10 | a result of `Won` or `Draw` appears only on the last move of the game | `PrematureResult` |
```

In `docs/architecture/07_TESTING.md`, in the malformed input table, delete the row:

```
| `result: won` in the middle of the list | `PrematureResult` |
```

- [ ] **Step 4: Correct the error strategy record**

In `docs/decisions/ADR-006-error-strategy.md`, replace:

```
- `gomoku-core` defines `GameError`, `RecordError`, and `ConfigError` with
  `thiserror`. Each implements `Display`, `Error`, `Debug`, `PartialEq`, and
  `Eq`, so tests compare errors directly.
```

with:

```
- `gomoku-core` defines `GameError`, `RecordError`, and `ConfigError` with
  `thiserror`. Each implements `Display`, `Error`, and `Debug`. `GameError`
  also implements `PartialEq` and `Eq`, so tests compare it directly.
  `RecordError` and `ConfigError` carry an `std::io::Error` source, which is
  not comparable, so their tests match on the variant with `matches!`.
```

- [ ] **Step 5: Update the status in the README**

In `README.md`, replace:

```
## Status

Design complete and approved. Implementation has not started. The approved
design is in [docs/specs/2026-09-22-gomoku-gui-design.md](docs/specs/2026-09-22-gomoku-gui-design.md).
```

with:

```
## Status

The `gomoku-core` crate is implemented: game state, the undo and rewind
history, tournament notation, the versioned record format, and the settings
schema. The graphical crate is not started.

- Approved design: [docs/specs/2026-09-22-gomoku-gui-design.md](docs/specs/2026-09-22-gomoku-gui-design.md)
- Implementation plan for the core crate: [docs/plans/2026-09-22-gomoku-core.md](docs/plans/2026-09-22-gomoku-core.md)
```

And in the same file, replace:

```
| [docs/specs/](docs/specs/) | The approved design, requirements, and build order |
```

with:

```
| [docs/specs/](docs/specs/) | The approved design, requirements, and build order |
| [docs/plans/](docs/plans/) | Implementation plans |
```

- [ ] **Step 6: Verify the documentation claims against the code**

Run: `grep -rn "PrematureResult" docs README.md`
Expected: no output.

Run: `cargo test -p gomoku-core`
Expected: every test passes. The count is the count from Task 6.

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --all`
Expected: no warnings, no diff.

- [ ] **Step 7: Report the final test count**

Run: `cargo test -p gomoku-core 2>&1 | tail -20`
Expected: the summary line names the number of passing tests. Put that number in the report file, with the list of test files.

- [ ] **Step 8: Commit**

```bash
git add docs README.md
git commit -m "docs: align the design documents with the core implementation"
```

---

### Task 8: Deferred findings sweep

**Files:**
- Modify: `crates/core/src/game.rs`
- Modify: `crates/core/tests/invariants.rs`

**Interfaces:**
- Consumes: everything from Tasks 1 to 7.
- Produces: nothing. This task changes two comments and strengthens three property
tests. It changes no behaviour.

Reason: the per-task reviews parked these Minor findings so that each one does
not widen an unrelated task's diff. Fix them together, in one reviewed commit,
after the functional work is complete.

- [ ] **Step 1: Correct the `play` doc comment**

The comment still describes the order before the Task 3 amendment. Replace:

```rust
    /// When the view is rewound, every move after the cursor is deleted
    /// first. Read `pending_truncation` before you call this, and confirm
    /// with the user when it is not zero.
```

with:

```rust
    /// When the view is rewound, the moves after the cursor are deleted as
    /// part of the placement, so a rejected placement changes nothing. Read
    /// `pending_truncation` before you call this, and confirm with the user
    /// when it is not zero.
```

- [ ] **Step 2: Correct the invariant header**

In `crates/core/tests/invariants.rs`, replace the six-line invariant list at the
very top of the file with:

```rust
//! I1 cursor <= len
//! I2 the board equals a replay of moves[..cursor]
//! I3 the move list has no repeated point. A point outside the board is
//!    unrepresentable, because `Move::new` rejects it.
//! I4 the board holds exactly `cursor` moves
//! I5 the side to move follows the cursor parity
//! I6 the outcome follows the board at the latest position
```

- [ ] **Step 3: Strengthen the rewind and forward property**

Replace the body of `rewind_then_forward_restores_the_same_position` with:

```rust
        let mut game = Game::new();
        for step in steps {
            apply(&mut game, step);
        }
        // `forward` stops at the latest move, so the cycle restores the live
        // position. The walk can leave a rewound view, so check the live state
        // and then step back to the view the walk left.
        let mut live_before = game.clone();
        live_before.live();
        let view_before = game.clone();
        let cursor = game.cursor();
        while game.rewind() {}
        while game.forward() {}
        prop_assert_eq!(&game, &live_before);

        let moved = game.seek(cursor);
        prop_assert_eq!(moved, cursor != live_before.len());
        prop_assert_eq!(&game, &view_before);
```

- [ ] **Step 4: Close the silent skip in the truncation property**

Replace the body of `play_while_rewound_truncates_the_tail` with:

```rust
        let mut game = Game::new();
        for step in steps {
            apply(&mut game, step);
        }
        let cursor = game.cursor();
        let legal = game.stone_at(point).is_none() && game.status() == Status::Ongoing;
        match game.play(point) {
            Ok(()) => {
                prop_assert_eq!(game.len(), cursor + 1);
                prop_assert_eq!(game.cursor(), cursor + 1);
                prop_assert!(!game.is_rewound());
                prop_assert_eq!(game.last_move(), Some(point));
                prop_assert_eq!(game.outcome(), expected_outcome(&game));
            }
            // A refused placement is only allowed when the guard above agrees.
            Err(_) => prop_assert!(!legal, "a legal placement was refused"),
        }
```

- [ ] **Step 5: Add the property that reaches a won game**

Random play almost never completes five in a row, so the won-game branch of
`check` was covered only by two unit tests. Add this property at the end of the
file, and change the import line to `use engine::{Board, Color, Move, Status};`:

```rust
    /// A won game keeps its outcome while the view moves, and the board at a
    /// rewound view is ongoing. This is invariant I6 in the case that random
    /// play almost never reaches.
    #[test]
    fn a_won_game_keeps_its_outcome_while_the_view_moves(
        steps in prop::collection::vec(any_step(), 0..30),
    ) {
        let mut game = Game::new();
        for index in 0..4u8 {
            game.play(Move::new(7, 3 + index).expect("inside the board"))
                .expect("the cell is empty");
            game.play(Move::new(3 + index, index).expect("inside the board"))
                .expect("the cell is empty");
        }
        game.play(Move::new(7, 7).expect("inside the board"))
            .expect("the cell is empty");

        let won = Outcome::Won {
            winner: Color::Black,
            method: gomoku_core::WinMethod::Five,
        };
        prop_assert_eq!(game.status(), Status::Won(Color::Black));
        prop_assert_eq!(game.outcome(), won);

        for step in steps {
            // A placement is skipped: the move list must stay won and fixed.
            if let Step::Play(_) = step {
                continue;
            }
            apply(&mut game, step);
            prop_assert_eq!(game.outcome(), won, "I6: the outcome describes the game");
            check(&game);
        }
    }
```

- [ ] **Step 6: Prove that the strengthened assertion can fail**

A strengthened test that cannot fail is worthless. Prove that the new `Err(_)`
arm in Step 4 works:

1. Temporarily change `play` so that it refuses a legal placement, for example
   by moving the `GameError::GameOver` check to the top of the function.
2. Run `cargo test -p gomoku-core --test invariants play_while_rewound` and save
   the output to `.superpowers/sdd/2026-09-22-gomoku-core/task-8-mutation.txt`.
   Expected: the property FAILS with "a legal placement was refused".
3. Restore `play` exactly.
4. Run the same command again and confirm it passes.

- [ ] **Step 7: Run the gates and the strengthened run**

Run: `cargo test`
Expected: 21 unit tests and 4 property tests pass.

Run: `PROPTEST_CASES=2000 cargo test -p gomoku-core --test invariants`
Expected: 4 property tests pass. Save the output to
`.superpowers/sdd/2026-09-22-gomoku-core/task-8-proptest-2000.txt`.

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --all`
Expected: no warnings, no diff.

- [ ] **Step 8: Commit**

```bash
git add crates/core
git commit -m "test(core): strengthen the property tests and correct two comments"
```

---

## Completion checklist

The core crate is complete when every item below is true.

- [ ] `cargo test` passes in the workspace root.
- [ ] `cargo clippy --all-targets -- -D warnings` reports nothing.
- [ ] `cargo fmt --all --check` reports nothing.
- [ ] No `unwrap`, `expect`, or `panic!` in `crates/core/src` outside a proven invariant with a comment above it.
- [ ] `crates/core/tests/golden/hotseat-v1.json` is committed and the encoder still produces it byte for byte.
- [ ] The `engine` crate has no modification by this project: the output of `git -C ../rust-ml status --short gomoku` still equals the baseline recorded in the ledger at the start of the plan. That checkout already carried unrelated staged work from its owner, so an empty status is not the test.
- [ ] No document claims a check that the code does not make.
