//! Game state, history, notation, record format, and settings for the
//! Gomoku GUI.
//!
//! This crate is pure. It has no window, no GPU, no audio, and no file
//! dialog. The rules engine is called from the `game` module only.
#![deny(missing_docs)]

mod error;
mod game;
mod notation;
mod record;
mod storage;

pub use error::{ConfigError, GameError, RecordError};
pub use game::{Game, MetaData, Outcome, WinMethod};
pub use notation::label;
pub use record::{
    BOARD_SIZE, FORMAT_MARKER, FORMAT_VERSION, Players, Record, StoredColor, StoredMethod,
    StoredOutcome,
};
pub use storage::{load, save, write_atomic};
