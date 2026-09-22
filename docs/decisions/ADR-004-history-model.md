# ADR-004: Destructive undo, a separate review cursor, and truncation on placement

## Status

Accepted (owner-approved, 2026-09-22)

## Context

The owner asked for forward, back, and undo controls. Three models were
considered:

| Model | Behaviour |
|---|---|
| One cursor | Back and undo are the same action. The move stays available forward. |
| Destructive undo plus a review cursor | Undo destroys the move. Arrows move a view cursor that does not touch the game. |
| Read-only review mode | Stepping back is a separate mode. Play resumes only at the tip. |

The owner's rule, in his words: "Undo — and the move is gone forever. Rewind and
we can return to the latest position. If anywhere on a rewound history I choose
to place a stone, that move will delete whatever has been after the current
move."

## Decision

`Game` holds one linear move list, one board, and one cursor.

- **Undo** removes the last move from the list and destroys it. There is no
  redo stack and no redo shortcut.
- **Rewind** and **forward** move the cursor and step the board with
  `Board::undo` and `Board::play`. The move list does not change. `live()`
  returns to the latest position at any time.
- **Placement while rewound** deletes every move after the cursor and then plays
  the new move, after one confirmation that names the count of deleted moves.
- Core does not show dialogs. It exposes `pending_truncation()`, and the caller
  confirms. This keeps the core crate free of user-interface concerns.

## Consequences

- One truth, one cursor. The board, the move list, the save file, and the
  autosave cannot disagree, because every mutation goes through five methods.
- A take-back is two different intents, and each has its own control. Undo is a
  decision about the game, rewind is a decision about the view. The move list
  and the review banner show which one the user is in.
- The confirm dialog protects an accidental click from silently deleting the
  rest of a game.
- Undo cannot be reversed. A player who undoes a move by accident has lost it.
  This is the owner's explicit choice.
- The outcome describes the game, not the view, so a rewind after a win still
  reports the win. The banner carries the review state, so the two are never
  confused.
- An undo of a winning move reopens the game, because the outcome is recomputed
  from the board after every change.
