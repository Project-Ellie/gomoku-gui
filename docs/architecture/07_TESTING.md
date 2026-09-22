# 07 — Testing

## Principle

Test behaviour through the public interface of each crate. Test the core
crate thoroughly without a window, a GPU, or a sound device. Test the pure
functions of the graphical crate thoroughly. Verify the visual result by eye,
because a pixel test cannot judge whether the wood looks like wood.

## What is tested where

| Layer | Mechanism | Runs in |
|---|---|---|
| `gomoku-core` behaviour | `#[cfg(test)]` modules next to the code | `cargo test` |
| `gomoku-core` invariants | `proptest` in `crates/core/tests/` | `cargo test` |
| Record format | Golden file plus malformed input cases | `cargo test` |
| Camera and geometry maths | Unit tests on pure functions | `cargo test` |
| Input mapping | Unit tests on the pure map function | `cargo test` |
| Shaders | `naga` parses and validates every WGSL module | `cargo test` |
| Audio synthesis | Unit tests on `render_click` and the queue | `cargo test` |
| The rendered image | Human review | manual |

## Core tests

### Game operations

| Test | Assertion |
|---|---|
| `play_places_a_stone` | The stone is readable at the point, the cursor advances, the side to move flips |
| `play_rejects_an_occupied_point` | `GameError::Occupied`, and the state is unchanged |
| `play_rejects_after_a_win` | `GameError::GameOver`, and the move list is unchanged |
| `undo_removes_the_last_move` | The move list loses one entry, the cursor follows, the cell is empty, `forward` cannot bring it back after a new move |
| `undo_on_an_empty_game` | Returns false, no panic, no state change |
| `rewind_does_not_change_the_game` | The move list is identical before and after any rewind and forward sequence |
| `forward_replays_the_same_position` | The board after rewind then forward equals the board before |
| `seek_clamps_out_of_range` | `seek(999)` lands on the last move, `seek(-1)` is rejected by the type |
| `play_while_rewound_truncates` | The moves after the cursor are gone, the new move is last, and the length is `cursor + 1` |
| `pending_truncation_reports_the_count` | The count equals `len - cursor`, and it is zero when live |
| `win_is_detected` | Five in a row yields `Won`, an overline of six also yields `Won` |
| `draw_is_detected` | A full board with no line yields `Draw` |
| `undo_reopens_a_won_game` | Undoing the winning move returns the status to `Ongoing` |
| `outcome_describes_the_game_not_the_view` | A rewind after a win keeps the win in the metadata |
| `stone_lookup_agrees_with_the_move_list` | `stone_at` and the `stones` iterator report the same colour for all 225 points at every cursor position |

### Invariants

`proptest` generates random legal and illegal action sequences from a small
alphabet (play a random point, undo, rewind, forward, seek, live, new game) and
asserts, after every step:

| Test | Assertion |
|---|---|
| `cursor_is_in_range` | `cursor <= moves.len()` |
| `board_matches_replay` | A fresh `Board` replaying `moves[..cursor]` equals the live board |
| `moves_are_unique_and_in_range` | No repeats, all inside the board |
| `side_to_move_is_derived` | `to_move()` agrees with the cursor parity |
| `record_round_trip` | `Game -> Record -> JSON -> Record -> Game` produces an equal move list, cursor, and metadata |

A second property test runs only valid games and asserts that a save followed
by a load produces an equal game at every step.

## Record tests

### Golden file

`crates/core/tests/golden/hotseat-v1.json` is a checked-in record. A test
writes the same game and asserts byte equality. A schema change therefore fails
the test loudly, and the failure shows the exact diff, which is the point. The
golden file is updated only when the schema changes on purpose, and that change
goes with a version decision.

### Malformed input

One test per rejection path from the persistence document:

| Input | Expected error |
|---|---|
| `format: "something-else"` | `WrongFormat` |
| `version: 2` | `UnsupportedVersion` |
| `size: 19` | `UnsupportedSize` |
| `ruleset: "renju"` | `UnsupportedRuleset` |
| 226 moves | `TooManyMoves` |
| `[15, 0]` | `PointOutOfRange` with the index |
| The same point twice | `RepeatedPoint` with the index |
| A move onto an occupied point | `IllegalMove` with the index |
| `result: won` with an ongoing board | `ResultMismatch` |
| `result: won` in the middle of the list | `PrematureResult` |
| Truncated JSON | `Parse` |
| Unknown extra field | Loads successfully, field ignored |

## Graphical crate tests

Pure functions only. No GPU and no window.

| Test | Assertion |
|---|---|
| `screen_board_round_trip` | For random points, zoom levels, centres, and flips, `screen_to_board(board_to_screen(p))` equals `p` within `1e-4` |
| `zoom_keeps_the_pointer_anchored` | The board point under the pointer is unchanged by a zoom |
| `zoom_is_clamped` | Ten zoom-ins land on `fit * 40`, ten zoom-outs land on `fit` |
| `pan_is_clamped` | A huge drag cannot push the board out of the window |
| `fit_centres_the_board` | After fit, the centre is `(7.0, 7.0)` |
| `resize_keeps_the_zoom_scale` | A resize changes the clamp but not `pixels_per_cell` unless the clamp demands it |
| `hit_test_finds_the_nearest_intersection` | A point 0.44 cell from the centre hits, 0.46 does not |
| `hit_test_rejects_off_board` | Points outside `0..=14` return no target |
| `notation_centre_is_h8` | `label(7, 7) == "H8"` |
| `notation_skips_i` | Column 8 is `J`, and the label set has no `I` |
| `notation_is_a_bijection` | Every one of the 225 points maps to a unique label and back |
| `octave_fade_range` | The fade factor is 1 for a small footprint and 0 for a large one |
| `shortcut_map_is_stable` | Each shortcut maps to the documented action, and Cmd-Z maps to `Undo` |
| `egui_input_has_priority` | A click reported by egui as wanted is not converted to a board action |

## Shader test

```rust
#[test]
fn shaders_are_valid_wgsl() {
    for (name, src) in [("board", BOARD_WGSL), ("stone", STONE_WGSL), ("shadow", SHADOW_WGSL)] {
        let module = naga::front::wgsl::parse_str(src)
            .unwrap_or_else(|e| panic!("{name} is not valid WGSL:\n{}", e.emit_to_string(src)));
        naga::valid::Validator::new(Default::default(), naga::valid::Capabilities::all())
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name} fails validation:\n{e:?}"));
    }
}
```

This catches a shader typo in `cargo test` instead of at run time on a GPU. It
is the cheapest high-value test in the graphical crate.

## Audio tests

As listed in the audio document: buffer length, sample bounds, envelope shape,
silent edges, determinism per seed, difference between seeds, damping, and the
queue behaviour under a full buffer and two threads.

## Manual verification

Run the application and check each row. This is the acceptance test for the
visual requirements.

| Check | Expected |
|---|---|
| Start-up | A wooden board, fitted to the window, with coordinates |
| Place a stone | The stone lands on the intersection under the pointer, with a contact shadow, and a knock plays |
| Zoom to the maximum | Grain, pores, and stone texture stay sharp, and nothing shimmers |
| Zoom to the minimum | No aliasing on the grain, the grid, or the stone edges |
| Rewind 20 moves | The board follows, the move list highlights the cursor, and the banner appears |
| Return to latest | The board returns exactly to the last position |
| Place while rewound | The confirm appears with the correct count, and the tail is deleted |
| Undo | The last stone disappears, and the board is playable again |
| Win | The winning line is marked and the board locks |
| Save, then open | The game returns identically |
| Quit with changes | The unsaved-changes dialog appears |
| Quit and restart | The window, zoom, panel, and material state are restored, and the resume prompt appears |
| Switch material | Each preset changes the board and persists across a restart |
| Tuning panel | Changing a slider changes the render immediately |

## Gates

Run before every commit:

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

A commit with a red gate is not written. There is no continuous integration
service in this repository; the gates are the developer's responsibility and
are run locally.

## Deliberate gaps

| Gap | Reason |
|---|---|
| No pixel comparison of the rendered board | A pixel test breaks on every driver update and cannot judge "looks like real wood". The material tuning panel and human review cover this. |
| No GPU test in `cargo test` | The test suite must run on a machine without a GPU. The `naga` test covers shader correctness. |
| No test of window creation | A headless window test is not worth its platform-specific fragility for one window. |
| No test of the file dialogs | The dialogs belong to macOS. The code around them is a thin call. |
