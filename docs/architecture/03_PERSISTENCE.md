# 03 — Persistence

Three files with three jobs. A game record travels with the game. A config
file keeps the session. An autosave protects work in progress.

## 1. Game record — writes v2, reads v1 and v2

Extension: `.json`. One JSON object, pretty-printed, with a trailing newline.
New records use version 2, adding only a resignation result. The owner's explicit
requirement for default-on AI resignation supersedes the earlier integration
assumption of move-only endings and no format change (ADR-003/004). No AI
configuration, player-type metadata or search statistics are stored.

The frozen version 1 example remains readable:

```json
{
  "format": "gomoku-gui/record",
  "version": 1,
  "size": 15,
  "ruleset": "freestyle",
  "players": {
    "black": "Wolfie",
    "white": "Anna"
  },
  "created": "2026-09-22T18:04:11Z",
  "result": {
    "kind": "ongoing"
  },
  "moves": [
    [
      7,
      7
    ],
    [
      8,
      7
    ],
    [
      8,
      8
    ],
    [
      6,
      6
    ]
  ]
}
```

The example above is the exact content of the committed fixture
`crates/core/tests/golden/hotseat-v1.json`. It remains a frozen reader fixture.
The encoder is now pinned to `crates/core/tests/golden/resignation-v2.json`,
generated from the same moves, names and timestamp with White winning by
resignation. Both fixtures are loaded and validated in tests.

`serde_json::to_string_pretty` writes every array element on its own line, so each
move occupies five lines. The file is longer than a hand-written sample, and every
new move changes only its own lines, which keeps a version control diff clean. Do
not transcribe this example by hand: generate any fixture from the encoder.

The timestamp uses `time::serde::rfc3339`. The `time` crate does not write RFC 3339
by default, not even with its human-readable serde support, so the field carries the
`#[serde(with = "time::serde::rfc3339")]` attribute and a test pins the exact
wire text.

| Field | Type | Rule |
|---|---|---|
| `format` | string | Always `gomoku-gui/record`. Guard against opening a different JSON file. |
| `version` | integer | New writes: 2. Reads: 1 or 2; every other value is refused. |
| `size` | integer | 15. Refused if different. |
| `ruleset` | string | `freestyle`. |
| `players.black` | string or absent | Free text, may be empty. |
| `players.white` | string or absent | Free text, may be empty. |
| `created` | string | RFC 3339, for example `2026-09-22T18:04:11Z`. This application writes UTC. A reader accepts any valid offset. |
| `result` | object | See below. |
| `moves` | array of `[column, row]` | Column 0 is left. Row 0 is the top. Both in `0..=14`. Black moves first. |

`result` variants:

```json
{ "kind": "ongoing" }
{ "kind": "won", "winner": "black", "method": "five" }
{ "kind": "won", "winner": "white", "method": "resignation" }
{ "kind": "draw" }
```

`winner` is `black` or `white`. `method` is `five` in either version, or
`resignation` in version 2 only. The new meaning requires the version bump,
even though it fits the existing result shape. Methods are lowercase strings.

### Validation and game restoration

After JSON parsing, validation follows this order:

1. Check format marker, supported version, board size, ruleset and move count.
2. Replay every move from an empty board, checking bounds, repeated points and
   legality in sequence. No moves may follow a board win or draw.
3. Validate the result. Five/draw/ongoing must match the replayed board. A v2
   resignation requires an ongoing replayed board and winner opposite its side
   to move; v1 resignation, wrong winner or a terminal-board resignation returns
   `ResultMismatch`. Apply valid resignation through `Game::resign`.

Saving from a reviewed resignation writes **all** moves and the whole-game
result, not the visible prefix. Loading restores that game finished at its live
tip. Names and creation time are preserved. Resignation is not a move and
changes neither the board nor turn. Games imported from puzzle positions remain
rejected with `FromPosition`: neither version can encode their starting stones.

### Numeric points, not letters

`[7, 7]` is the centre. Letters are a view concern, so the file does not depend
on the notation. The display labels A-O and 1-15, and the `H8` name of the
centre, come from the same numeric pair.

### Forward compatibility

Unknown fields are ignored on load. New result meanings require a version bump,
not a guess. This build reads v1 and v2; an older v1 build refuses v2 records.

## 2. Config — TOML

Location: `~/Library/Application Support/gomoku-gui/config.toml`.
Written on exit and after a change, debounced to at most one write every two
seconds. A missing file is normal: the defaults apply.

```toml
[window]
width = 1180
height = 900
x = 140
y = 90
maximized = false

[view]
pixels_per_cell = 0.0        # 0.0 means "fit at start-up"
center = [7.0, 7.0]          # board space, in cells
flipped = false

[overlays]
coordinates = true
move_numbers = false
last_move = true
win_line = true

[panel]
width = 240.0
collapsed = false

[board]
material = "aged_wood"       # aged_wood | walnut | dark_marble | green_marble

[stones]
set = "slate_shell"

[audio]
enabled = true
volume = 0.7

[files]
last_directory = "/Users/wgiersche/Games"
recent = [
  "/Users/wgiersche/Games/2026-09-22-hotseat.json"
]
```

| Section | Purpose |
|---|---|
| `window` | Size, position, and maximised state. Restored when the display layout still fits. |
| `view` | Zoom and pan. `0.0` means the board is fitted and centred at start-up. |
| `overlays` | The four toggles from the View menu. |
| `panel` | Right panel width in logical pixels, and whether it is collapsed. The width is clamped to `120..=600`. |
| `board` | Material preset name. |
| `stones` | Stone set name. |
| `audio` | Sound on or off, and the volume in `0.0..=1.0`. |
| `files` | The last directory and up to ten recent records. |

Rules:

- A missing file, an unreadable file, or an unknown value falls back to the
  default for that field. The application never fails to start because of a
  bad config. A corrupt file is renamed to `config.toml.corrupt` and a warning
  is logged, so the next start is clean.
- Window geometry is clamped to a visible display. A saved position from a
  disconnected monitor does not hide the window.
- The config is written atomically.

## 3. Autosave — the game in progress

Location: `~/Library/Application Support/gomoku-gui/`.

| File | Content |
|---|---|
| `autosave.json` | A complete valid game record, exactly as Save would write it |
| `autosave-state.json` | `{ "source_path": "/…/game.json" \| null, "cursor": 12 }` |

Rules:

- The autosave is written after a placement, an undo, a truncation, or a change
  of player names, debounced to at most one write every two seconds, and once
  more on exit.
- A rewind does not change the record. Only the cursor in `autosave-state.json`
  is updated, so the view is restored too.
- The autosave file is a valid record on purpose. A user can open it by hand
  if the application ever fails to offer a resume.

### Resume decision at start-up

```
if autosave.json is absent                      -> normal start
if the autosave has no moves                    -> normal start, no prompt
if source_path is null                          -> prompt: "Resume unsaved game?"
if source_path exists and autosave mtime > source mtime -> prompt: "Resume?"
otherwise                                       -> normal start
```

The prompt offers Resume and Discard. Discard deletes both autosave files. The
prompt is never shown twice for the same state.

## 4. Atomic writes

Every file is written with the same three steps:

```
1. write to <name>.tmp in the same directory
2. flush and sync the file
3. rename <name>.tmp over <name>
```

`rename` inside one filesystem is atomic, so a crash leaves either the old
file or the new file, never a half-written file. A `.tmp` file left by a crash
is ignored and overwritten.

## 5. Errors

```rust
pub enum RecordError {
    Io { path: PathBuf, source: std::io::Error },
    Parse(String),
    WrongFormat { found: String },
    UnsupportedVersion(u32),
    UnsupportedSize(u32),
    UnsupportedRuleset(String),
    TooManyMoves { count: usize },
    PointOutOfRange { index: usize },
    RepeatedPoint { index: usize },
    IllegalMove { index: usize },
    ResultMismatch,
    FromPosition,
}
```

`Display` messages are written for the user, not for a log file. Examples:

- "This file is a version 3 record. This version of Gomoku opens versions 1 and 2."
- "Move 34 in this file is not legal. The file may be damaged."

The binary adds the file path with `anyhow::Context` and shows a dialog. A
failed save leaves the previous file untouched.

## 6. Migration policy

- Version 1 is frozen and remains readable. Its result must still follow the
  board, with no resignation claim.
- Version 2 adds resignation only. New saves, including games loaded from v1,
  use v2; player metadata, timestamp and moves retain their existing shape.
- Only versions 1 and 2 are accepted. Refuse unsupported versions and invalid
  results rather than guessing.
- The format lives in one module, `record.rs`. A second format (SGF export, for
  example) is a new module plus a new entry in the File menu, not a rewrite.
