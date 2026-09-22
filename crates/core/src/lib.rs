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
