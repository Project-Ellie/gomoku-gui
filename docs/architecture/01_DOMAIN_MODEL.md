# 01 — Domain Model

## Boundary types from the engine

The engine owns the rules. These types cross into our code.

| Type | Meaning | Notes |
|---|---|---|
| `Move` | One board cell | One `u8` = `row * 15 + col`. `Move::new(row, col) -> Option<Move>`. `row()`, `col()`, `index()`. |
| `Color` | `Black` or `White` | `Color::other()` |
| `Status` | `Ongoing`, `Won(Color)`, `Draw` | Result of `Board::status()` |
| `Board` | The position and the move list | `play(Move)`, `undo()`, `moves()`, `stone_at(Move)`, `status()`, `to_move()` |
| `PlayError` | Rejected placement | Occupied cell, game already over |

`Move` is `Copy`, `PartialEq`, `Eq`, and `Hash`. It is small, so it is passed
by value everywhere.

## Core types

```rust
/// A board point or a cell, in our own vocabulary: 0-based, row 0 at the top.
pub type Point = Move;              // re-exported, not re-implemented

/// The result of a finished game, as stored in a record.
pub enum Outcome {
    Ongoing,
    Won { winner: Color, method: WinMethod },
    Draw,
}

pub enum WinMethod {
    Five,      // five or more in a line
}

/// Game-level data that is not a move.
pub struct MetaData {
    pub black: Option<String>,
    pub white: Option<String>,
    pub created: time::OffsetDateTime,
    pub outcome: Outcome,
}
```

Rationale: we reuse `engine::Move` instead of defining a parallel `Point`
type. A second type would need conversions at every boundary and would gain
nothing.

## The `Game` aggregate

```rust
pub struct Game {
    moves:  Vec<Move>,
    board:  Board,
    cursor: usize,
    meta:   MetaData,
}
```

The aggregate is the only owner of the board. Callers never see `Board`.
Callers ask `Game` for what they need: `stone_at`, `stones`, `last_move`,
`status`, `move_number`, `to_move`, `cursor`, `len`, `is_rewound`,
`pending_truncation`.

## Invariants

These hold for every reachable state. Tests assert them after every operation.

| # | Invariant |
|---|---|
| I1 | `cursor <= moves.len()` |
| I2 | `board` equals a fresh replay of `moves[..cursor]` on a new board |
| I3 | `moves` contains no repeated point and no point off the board |
| I4 | `board.moves().len() == cursor` |
| I5 | Colors alternate from `Color::Black`, so `to_move() == Black` when `cursor` is even |
| I6 | `meta.outcome` is `Won` or `Draw` only when the board at the latest position says so; a rewound view does not change the outcome |

I6 is a deliberate choice. The outcome describes the game, not the view. A
rewound view still shows "Black wins" in the status line, together with the
review banner.

## State transitions

```
                 play (cursor == len)
      ┌───────────────────────────────────────┐
      │                                       ▼
   ┌──────┐   rewind    ┌───────────┐   play (cursor < len)   ┌──────────┐
   │ LIVE │────────────►│  REWOUND  │────────────────────────►│  LIVE    │
   │cursor│◄────────────│           │   truncate tail first   │          │
   │= len │  forward    └───────────┘                         └──────────┘
   └──────┘      or live()
```

Only a placement changes `moves`. Rewind, forward, seek, and live change the
view alone.

A finished game is a plain state, not a typestate. The board answers
`status()`, and the placement path rejects a move when the status is not
`Ongoing`. A typestate would need `Game<Ongoing>` and `Game<Finished>` and
would force every caller through a recovery path. The gain is small for one
boolean condition, so the status stays a value. The reason is recorded here
because the choice is deliberate.

## Errors

```rust
pub enum GameError {
    /// The cell already holds a stone.
    Occupied,
    /// The game is over. Undo or start a new game.
    GameOver,
    /// The cursor index is beyond the move list.
    CursorOutOfRange,
}
```

`GameError` uses `thiserror` and implements `Display` and `Error`. It is
`PartialEq` and `Eq`, so tests can compare errors directly.

## Display notation

Tournament notation, as adopted by the owner:

| Board index | Label |
|---|---|
| Column 0..14 | `A B C D E F G H I J K L M N O` |
| Row 0 (top) .. row 14 (bottom) | `15 14 13 12 11 10 9 8 7 6 5 4 3 2 1` |
| Centre, column 7 row 7 | `H8` |

Mapping:

```rust
const COLUMN_LETTERS: &[u8; 15] = b"ABCDEFGHIJKLMNO";
letter = COLUMN_LETTERS[col];
number = 15 - row;
```

The board is 15 columns wide, so all 15 letters A to O are used. The centre
of the board is `H8`: H is the eighth letter, and row 7 is the eighth row
counted from the bottom.

This convention was verified against published renju notation. The opening
`H8 H9 I8` uses the letter I, which shows that I is part of the label set.
An earlier version of this document claimed that the letter I is skipped, as
in Go. That is wrong for a 15-wide board: skipping I gives only 14 labels from
A to O, which cannot label 15 columns.

The record format stores numeric `[column, row]` pairs. The notation is a view
concern only, so a change of notation never invalidates a saved file.

Note: the terminal user interface in `rust-ml/gomoku/crates/cli` labels rows
`0..14` from the top. Those labels are debug labels. This application uses
tournament notation. The difference is intentional and is recorded here.
