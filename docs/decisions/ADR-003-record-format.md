# ADR-003: Game records use a versioned JSON format, not SGF

## Status

Accepted (owner-approved, 2026-09-22). Supersedes the SGF recommendation that
was made during design.

## Context

The owner asked whether an official game-record format exists, and asked that
the design use it or justify an alternative.

The research result:

- **SGF FF[4]** (red-bean.com) is the official standard for board-game records.
  Its game type list defines `GM[4]` as "Gomoku+Renju". Result is `RE[B+R]`,
  board size is `SZ[15]`, moves are `B[aa]` nodes with two-letter points, the
  rule set is `RU`, and the specification requires a reader to preserve unknown
  properties.
- Renju and Gomoku software exchanges SGF: RenLib, ORC Game Center exports, and
  Sabaki-based renju editors all read it. A parser exists for exactly this use
  case.
- The Renju International Federation publishes the international rules and
  tournament rules. It does not publish an electronic file format.
- The crate `sgf-parse` 4.2.8 is MIT licensed, has zero dependencies, reads and
  writes, and preserves non-standard properties. Games other than Go take a
  generic fallback path where moves are strings, which is the right behaviour
  for `GM[4]`.

SGF was recommended for the interoperation it provides. The owner chose our own
JSON format instead.

## Decision

Game records are a versioned JSON document with the marker
`"format": "gomoku-gui/record"`.

- Moves are stored as numeric `[column, row]` pairs with 0-based indices, so the
  file does not depend on the display notation.
- The document carries `version`, and a loader refuses a version above 1 and
  ignores unknown fields, so a later minor addition stays compatible.
- Writes are atomic: temporary file, flush, rename.
- The format lives in one module, `crates/core/src/record.rs`.

## Consequences

- No interoperation with RenLib, Sabaki, ORC exports, or any other renju tool.
  This is the cost the owner accepted, and it is recorded here so the trade is
  not rediscovered later.
- The format is exact and simple for our own needs, and it is readable and
  diffable by hand.
- Adding an SGF exporter later is a new module, a new entry in the File menu,
  and a round-trip test against real renju SGF files. It does not touch the
  game, the renderer, or the input path, so the exit stays cheap.
