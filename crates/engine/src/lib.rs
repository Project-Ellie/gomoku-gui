//! Gomoku rules engine — milestone 1 of the AlphaZero-style agent.
//!
//! Design: `docs/13-engine-design.md` in the rust-ml repository.
//! Pure Rust: no Burn, no I/O, no GPU. Runs on every self-play worker
//! thread simultaneously, inside the hottest loop in the system.
//!
//! Public surface is deliberately small (visibility as enforcement).
//! Rules: freestyle Gomoku on 15×15 — overlines count as a win, draw at
//! 225 moves, Swap2 opening protocol supported.

#![deny(missing_docs)]

mod bitboard;
mod board;
mod encode;
mod moveset;
mod opening;
mod symmetry;
mod tactics;
mod tss;
mod win;
mod zobrist;

#[cfg(any(test, feature = "testutil"))]
#[allow(missing_docs)]
pub mod reference;

// Public surface (ch. 13, "Crate layout" — re-exports ONLY what belongs
// to the API). Grows slice by slice.
pub use board::{Board, Color, PlayError, PositionError, Status};
pub use encode::{EXT, Planes, encode};
pub use moveset::{Move, MoveSet};
pub use opening::{FinalChoice, FirstChoice, Placing2, Placing3, Swap2};
pub use symmetry::Transform;
pub use tactics::{double_threats, forced_blocks, immediate_wins};
pub use tss::{Proof, SearchBudget, prove_forced_win, verify_line};
