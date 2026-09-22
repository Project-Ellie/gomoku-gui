# 05 — Interaction

## Event flow

winit delivers an event. The order is fixed:

```
1. winit event
2. if the event is a redraw or a resize -> rebuild targets, request redraw
3. egui-winit consumes the event and updates the egui context
4. if egui wants the pointer or the keyboard, we are done
5. otherwise the event goes to the board input map
6. a state change requests a redraw
7. an idle state returns ControlFlow::Wait
```

Step 4 is the important one. The board must not pan when a drag starts on the
move list or on a menu. egui reports this with `wants_pointer_input()` and
`wants_keyboard_input()`, and the check costs nothing.

## Actions

Input is mapped to an action, not to an effect. The action list is the seam
between the window system and the game.

```rust
pub enum Action {
    Place(Point),
    Undo,
    Rewind,
    Forward,
    Seek(usize),
    Live,
    Fit,
    ZoomBy(f32),
    PanBy(Vec2),
    ToggleFlip,
    ToggleOverlay(Overlay),
    NewGame,
    Open,
    Save,
    SaveAs,
    Quit,
}
```

The input map is a pure function from an event and a modifier state to
`Option<Action>`, so the whole key map is testable without a window.

## Hit testing

```
board_p  = screen_to_board(cursor_position)
nearest  = round(board_p)
offset   = board_p - nearest
if offset.length() > 0.45                     -> no target
if nearest is outside 0..=14 in either axis   -> no target
if Game::stone_at(nearest) is occupied        -> target, but not placeable
otherwise                                     -> placeable, highlight
```

The 0.45 radius matches the stone radius, so the clickable area is the stone
itself. A click that misses every stone does nothing, which is what a user
expects: no accidental placements in the gaps.

The highlight is drawn as a faint ring on the hovered intersection when a
placement is legal, and as nothing when it is not.

## Zoom and pan

| Gesture | Effect |
|---|---|
| Scroll up, or pinch out | Zoom in about the pointer |
| Scroll down, or pinch in | Zoom out about the pointer |
| Left drag | Pan |
| Double click | Fit and centre |
| `+` / `-` | Zoom in or out about the window centre by a factor of 1.15 |
| `0`, `F`, or Cmd-F | Fit and centre |

Details:

- The trackpad delivers both a scroll delta and a magnification factor.
  Magnification is used when present, otherwise the scroll delta is used with
  a factor of `exp(delta * 0.0015)`. Both paths keep the pointer anchored.
- Panning uses the pointer delta divided by `pixels_per_cell`, so a drag moves
  the board exactly with the pointer. Panning is clamped as described in the
  rendering document.
- `pixels_per_cell` is clamped to `[fit, fit * 40]` after every change,
  including a window resize.
- Flip mirrors both axes, which is equivalent to a 180-degree rotation of the
  board. A player on the far side of the table sees their own perspective, and
  the coordinate labels follow the physical board, as they would on a real
  board that is turned around.

## Overlay projection

Overlays are egui shapes and text, positioned through the camera transform:

| Overlay | Projection |
|---|---|
| Coordinates | Board points `(-0.62, y)` and `(14.62, y)` for the row labels, and `(x, -0.62)` and `(x, 14.62)` for the column labels |
| Move numbers | The intersection centre, with a `1.6` cell height circle of contrast behind the glyph |
| Last-move ring | The intersection centre at radius `0.40` cell |
| Winning line | The two end stones of the line, drawn as a line at 55 percent alpha |
| Hover ring | The intersection centre at radius `0.44` cell |

Because the labels are projected rather than computed in screen space, they
stay attached to the board under zoom, pan, and flip.

## Layout

```
┌──────────────────────────────────────────────────────────┐
│ File  Edit  View  Go                          egui top bar│
├──────────────────────────────────────────────┬───────────┤
│                                              │ Move list │
│                board viewport                │ 1  ● H8   │
│                                              │ 2  ○ I9   │
│                                              │ ...       │
│       [ Reviewing move 12 of 40 — Return ]    │ Players   │
│                                              │ Status    │
└──────────────────────────────────────────────┴───────────┘
```

| Area | Height or width | Content |
|---|---|---|
| Top bar | 26 px | File, Edit, View, Go |
| Right panel | 240 px, resizable, collapsible | Move list, player names, status |
| Viewport | The rest | The board passes, plus the egui overlays |
| Review banner | Floating, bottom centre of the viewport | Shown only when rewound |
| Material panel | Floating, collapsible | Shown only when opened from View |

The status line shows the side to move, the move number, and the outcome.
When the view is rewound, it shows the outcome of the game and the banner
carries the review state, so the two are never confused.

The move list shows one row per move: number, a stone glyph, and the
coordinate label. The current cursor row is highlighted, and a click seeks to
that position. A double click seeks and returns to live.

Player names are two text fields. They are stored in the record, default to
empty, and are remembered for the next game.

## Dialogs

Only four modal dialogs exist:

| Dialog | Trigger | Buttons |
|---|---|---|
| Truncate confirm | A placement while the cursor is behind the latest move | Place and delete N moves / Cancel |
| Unsaved changes | New game, Open, or Quit with changes since the last save | Save / Discard / Cancel |
| Resume | An autosave newer than the last save, at start-up | Resume / Discard |
| Error | A failed load or save | OK, with the full message |

The truncate confirm names the count, for example "This deletes 12 moves". The
count comes from `Game::pending_truncation()`, so the dialog and the action can
never disagree.

## Hi-DPI

All camera arithmetic uses physical pixels from
`Window::scale_factor() * logical_size`. egui receives the scale factor, so its
chrome is crisp on a Retina display. Overlay text sizes are physical pixels,
which keeps labels readable at any zoom on any display.

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| Left click | Place a stone |
| Left / Right arrow | Rewind / forward one move |
| Home / End | First position / latest position |
| Cmd-Z | Undo. Destroys the last move. |
| Cmd-N | New game |
| Cmd-O | Open |
| Cmd-S | Save |
| Cmd-Shift-S | Save As |
| Cmd-Q | Quit |
| `+` / `-` | Zoom in / out |
| `0` / `F` / Cmd-F | Fit |
| `C` | Toggle coordinates |
| `M` | Toggle move numbers |
| `L` | Toggle the last-move marker |
| `W` | Toggle the winning line |
| `V` | Flip the view |
| Space | Return to the latest position when rewound |

There is no redo shortcut, because undo destroys the move. `Space` is the
"return to live" gesture, and it does nothing when the view is already live.

## Window and quit behaviour

- The window title is `Gomoku — <file name>` with a trailing `•` when there
  are unsaved changes, or `Gomoku — Untitled •` for a new game.
- Closing the window with unsaved changes asks first.
- The last window size, position, and maximised state are stored.
- A saved position on a display that is no longer connected is ignored, and the
  window opens centred on the main display.
