# 02 — Core Logic

## The facade over the engine

`game.rs` is the only module that calls the engine. All other modules speak in
`Game` operations. If the rules source ever changes, this one file changes.

Engine calls used:

| Our operation | Engine call |
|---|---|
| Create an empty game | `Board::new()` |
| Apply a move | `Board::play(mv)` |
| Remove the last move | `Board::undo()` |
| Read a cell | `Board::stone_at(mv) -> Option<Color>`, an O(1) bitboard probe |
| List the stones at the cursor | `Board::moves()`, in play order |
| Game state | `Board::status()` |
| Side to move | `Board::to_move()` |

`Game` keeps no duplicate of the position. A per-cell lookup delegates to
`Board::stone_at`, and the render instance list comes from `Board::moves()`,
which is already the position at the review cursor. Colours follow the play
order, because freestyle hotseat play alternates strictly and Black moves
first:

```rust
pub fn stones(&self) -> impl Iterator<Item = (Move, Color)> + '_ {
    self.board.moves().iter().enumerate().map(|(i, &mv)| {
        (mv, if i % 2 == 0 { Color::Black } else { Color::White })
    })
}
```

Duplicating the board in a second array would need an invariant to keep the two
in step. Delegating needs none.

## Operations

### `play(point) -> Result<(), GameError>`

```
1. Reject if stone_at(point).is_some().     -> GameError::Occupied
2. Reject if self.board.status() != Ongoing. -> GameError::GameOver
3. If cursor < moves.len():
     discard moves[cursor..]              (the caller confirmed first)
4. self.board.play(point)?                (engine validates again)
5. self.moves.push(point); self.cursor += 1
6. Store `Board::status()` in the metadata as the outcome. The caller reads it
     with `outcome()`.
```

Step 3 is the truncation rule. Core does not show a dialog. The caller asks
`pending_truncation()` first and confirms, then calls `play`. This keeps the
core crate free of user-interface concerns.

### `undo() -> bool`

```
1. If moves.is_empty() -> false.
2. self.cursor = self.moves.len()          (leave a rewound view first)
3. self.moves.pop()
4. self.board.undo(); self.cursor -= 1
5. Clear an outcome that the removal invalidated.  (I6)
6. -> true
```

The move is gone. There is no redo stack. This is the owner's rule: undo
destroys the move.

Step 5 exists because a win can be undone. If the last move was the winning
move, the game is ongoing again. The outcome is recomputed from
`board.status()` in one place after every change.

### `rewind() -> bool`, `forward() -> bool`, `seek(index) -> bool`, `live()`

```
rewind:  if cursor == 0 -> false; else cursor -= 1; board.undo(); true
forward: if cursor == moves.len() -> false; else
           let mv = moves[cursor]; board.play(mv)?; cursor += 1; true
seek(i): clamp i to 0..=moves.len(); step the cursor with the same
         incremental play and undo calls, so the board is never rebuilt
live():  seek(moves.len())
```

Incremental stepping is used instead of a replay, so a cursor move is O(1)
regardless of the game length. The board therefore always matches
`moves[..cursor]`, and invariant I2 holds by construction.

### `set_players(black, white)`

There is no `new_game`: a new game is a new `Game`. The caller drops the old one
and sets the player names again, because the same two people usually play the
next game.

## Game-end rules

The engine decides. Our code only reads `Status`:

| Engine status | Our outcome |
|---|---|
| `Status::Won(color)` | `Outcome::Won { winner: color, method: WinMethod::Five }` |
| `Status::Draw` | `Outcome::Draw` |
| `Status::Ongoing` | `Outcome::Ongoing` |

Freestyle rules apply: five or more stones in a line win, so an overline wins.
This matches the engine and the rest of the project.

A finished game rejects further placements. Undo reopens it, because the rule
is recomputed from the board.

## Validation on load

A record can come from disk, so every field is checked. The checks run in the
order below, and the first failure wins.

| # | Check | Error |
|---|---|---|
| 1 | `format` is `gomoku-gui/record` | `RecordError::WrongFormat` |
| 2 | `version` is at most 1, so a newer file is refused | `RecordError::UnsupportedVersion(u32)` |
| 3 | `size` is 15 | `RecordError::UnsupportedSize(u32)` |
| 4 | `ruleset` is `freestyle` | `RecordError::UnsupportedRuleset` |
| 5 | `moves.len() <= 225` | `RecordError::TooManyMoves` |
| 6 | each point is inside the board | `RecordError::PointOutOfRange { index }` |
| 7 | each point is new | `RecordError::RepeatedPoint { index }` |
| 8 | `Board::play` accepts each point in order | `RecordError::IllegalMove { index }` |
| 9 | the stored `result` agrees with the board after the last move | `RecordError::ResultMismatch` |

Check 8 catches a file whose moves are already illegal at move 20, and it also
catches a game that continues after a win. Check 9 is not a repeat of check 8:
a file can hold only legal moves and still store a result that the board does not
support, which is what check 9 catches. Check 4 leaves room for a future
ruleset field without a version bump.

On any failure the caller keeps the current game and shows the message. A load
never leaves a half-loaded game.

## Record creation

`Game` turns itself into a `Record` in one place. The record holds no cursor
and no view state, because those are not part of the game.

## Time

The creation time is read once, when a game starts, and stored in UTC with a
`Z` suffix. The type is `time::OffsetDateTime` from the `time` crate, with the `serde` and
`serde-human-readable` features and the per-field attribute
`#[serde(with = "time::serde::rfc3339")]`. The attribute is required: without
it the crate writes its own space-separated form, not RFC 3339.
`time` is small, maintained, and has no transitive dependency surprise. The
alternative was a plain `String`, which would push the validity question onto
the record validator for no gain.

## Why no separate `Rules` trait

A trait with one implementation adds a layer and a generic parameter without a
second consumer. The engine is the only rules source. The facade is a plain
module boundary, which gives the same isolation with none of the machinery.
When an agent opponent arrives, it will need a *player*, not a *rules* source,
so a `Player` trait is the seam that will actually be used. Adding it now
would be premature.
