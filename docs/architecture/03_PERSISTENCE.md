# 03 — Persistence

Three files with three jobs. A game record travels with the game. A config
file keeps the session. An autosave protects work in progress.

## 1. Game record — version 1

Extension: `.json`. One JSON object, pretty-printed, with a trailing newline.

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
`crates/core/tests/golden/hotseat-v1.json`. A test compares the encoder's output
against those bytes, so the example cannot drift from the code.

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
| `version` | integer | 1. A higher value is refused with a clear message. |
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
{ "kind": "draw" }
```

`winner` is `black` or `white`. `method` is `five`. `method` is present because
a later ruleset or an agent opponent may add a second way to win, and an enum
absorbs that without a schema change.

`win_method` is serialised as a lowercase string, so the file stays readable
and a rename in Rust cannot silently change the file format.

### Numeric points, not letters

`[7, 7]` is the centre. Letters are a view concern, so the file does not depend
on the notation. The display labels A-O and 1-15, and the `H8` name of the
centre, come from the same numeric pair.

### Forward compatibility

Unknown fields are ignored on load. Therefore a version 1 file written by a
later minor revision still opens in this revision. A new *meaning* needs
`version: 2`, and the loader refuses it instead of guessing.

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
}
```

`Display` messages are written for the user, not for a log file. Examples:

- "This file is a version 2 record. This version of Gomoku opens version 1."
- "Move 34 in this file is not legal. The file may be damaged."

The binary adds the file path with `anyhow::Context` and shows a dialog. A
failed save leaves the previous file untouched.

## 6. Migration policy

- Version 1 is the first release. There is nothing to migrate.
- A version 2 loader must be able to read a version 1 file. The rule is: refuse
  a newer version, upgrade an older version, never guess a field.
- The format lives in one module, `record.rs`. A second format (SGF export, for
  example) is a new module plus a new entry in the File menu, not a rewrite.
