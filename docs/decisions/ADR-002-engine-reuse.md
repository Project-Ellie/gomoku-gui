# ADR-002: Reuse the rust-ml Gomoku engine behind a facade

## Status

Accepted (owner-approved, 2026-09-22)

## Context

The repository `Project-Ellie/rust-ml` holds a completed freestyle Gomoku
rules engine at `gomoku/crates/engine`. It was verified to build with
`cargo check -p engine`, it has `thiserror` and `serde` as its only
dependencies, it stores a move as one `u8` with `row * 15 + col`, and it
exposes `Board::{new, play, undo, moves, status, to_move}`. Its win detection
is covered by a property-test differential suite against a naive oracle.

The alternative is a second rules implementation inside this repository.

## Decision

Depend on the engine as a path dependency
(`../rust-ml/gomoku/crates/engine`) and call it from one module,
`crates/core/src/game.rs`, behind a small operation facade.

The engine is not modified. If the user interface needs an engine change, the
need is reported to the owner instead of being implemented here, because the
engine is the owner's tutorial material.

## Consequences

- Gomoku rules have one source of truth across the ecosystem, and the win
  detection is not rewritten or re-tested here.
- This repository needs the sibling checkout to build. The README documents
  the path. If the repository is cloned on another machine, the dependency
  changes to a pinned git revision, which is a one-line change in the
  workspace manifest.
- The facade is a module, not a trait. A trait with one implementation adds a
  layer with no second consumer. When an agent opponent arrives, the seam that
  will actually be used is a player, not a rules source.
- The engine has no `Serialize`. The record format is therefore ours, which is
  what the owner chose anyway (ADR-003).
- `Board::stone_at` gives an O(1) cell lookup, and `Board::moves()` gives the
  stones at the review cursor. `Game` delegates to both and keeps no copy of
  the position, so there is no second source of truth to keep in step.
