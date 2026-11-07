//! The error types of the core crate.

use std::path::PathBuf;

use engine::PlayError;
use thiserror::Error;

/// Errors returned when importing a game position.
///
/// Duplicate entries within a color are rejected before the engine validates
/// overlap and counts. The record format has its own validation errors.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum PositionError {
    /// The same cell appears more than once in one color's stone list.
    #[error("The same cell appears twice for one color.")]
    DuplicateStone,
    /// A cell appears in both the black and white lists.
    #[error("Black and white stones overlap on the same cell.")]
    OverlappingColors,
    /// Stone counts are inconsistent with the side to move.
    #[error("Stone counts are inconsistent with the side to move.")]
    InvalidCounts,
}

impl From<engine::PositionError> for PositionError {
    fn from(err: engine::PositionError) -> Self {
        match err {
            engine::PositionError::OverlappingColors => Self::OverlappingColors,
            engine::PositionError::InvalidCounts => Self::InvalidCounts,
        }
    }
}

/// Errors returned by the game operations.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum GameError {
    /// The intersection already holds a stone.
    #[error("That intersection already holds a stone.")]
    Occupied,
    /// The game is over.
    #[error("The game is over. Take the last move back or start a new game.")]
    GameOver,
    /// Resignation requires the live position, not a reviewed move.
    #[error("Return to the live position before resigning.")]
    NotLive,
    /// The rules engine rejected the placement.
    #[error("The rules engine rejected the move: {0}")]
    Rejected(PlayError),
}

impl From<PlayError> for GameError {
    fn from(err: PlayError) -> GameError {
        match err {
            PlayError::Occupied => GameError::Occupied,
            PlayError::GameOver => GameError::GameOver,
            // Explicit, so that a new engine variant fails to compile here
            // instead of being silently reclassified.
            PlayError::BadOpeningCounts => GameError::Rejected(PlayError::BadOpeningCounts),
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
    /// The record uses a schema version this build cannot read.
    #[error("This file is a version {0} record. This version of Gomoku opens versions 1 and 2.")]
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
    /// The stored result disagrees with the moves or is invalid for its version.
    #[error("The result stored in this file is not valid for its moves or record version.")]
    ResultMismatch,
    /// The game was built from a puzzle position and cannot be saved as a
    /// moves-only record.
    #[error("Games loaded from a puzzle position cannot be saved as game records.")]
    FromPosition,
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
