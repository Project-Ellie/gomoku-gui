//! Game state, history, notation, record format, and settings for the
//! Gomoku GUI.
//!
//! This crate is pure. It has no window, no GPU, no audio, and no file
//! dialog. The rules engine is called from the `game` module only.
#![deny(missing_docs)]

mod config;
mod error;
mod format;
mod game;
mod history;
mod index;
mod notation;
mod psq;
mod puzzle;
mod record;
mod storage;

pub use config::{
    AudioSettings, BoardSettings, ConfigLoad, ConfigNotice, FileSettings, MAX_PANEL_WIDTH,
    MAX_RECENT, MIN_PANEL_WIDTH, OverlaySettings, PanelSettings, PlayersSettings, SeatSettings,
    Settings, StoneSettings, ViewSettings, WindowSettings, load_settings, save_settings,
};
pub use engine::{Color, Move, Status};
pub use error::{ConfigError, GameError, PositionError, RecordError};
pub use format::{
    CORPUS_PROBE_LIMIT, Confidence, Format, LoadError, OpenedThing, RecordKind, RecordMeta,
    RecordUnit, Registry,
};
pub use game::{Game, MetaData, Outcome, WinMethod};
pub use index::{IndexEntry, index_directory};
pub use notation::label;
pub use psq::PsqFormat;
pub use puzzle::{Puzzle, PuzzleError, load_puzzles};
pub use record::{
    BOARD_SIZE, FORMAT_MARKER, FORMAT_VERSION, NodeRecord, Players, Record, StoredColor,
    StoredMethod, StoredOutcome,
};
pub use storage::{load, save, write_atomic};

/// The point at a row and a column, or `None` when it is off the board.
///
/// This is the only way to build a [`Move`] from coordinates, so a point
/// outside the board cannot be constructed.
pub fn point(row: u8, col: u8) -> Option<Move> {
    Move::new(row, col)
}
