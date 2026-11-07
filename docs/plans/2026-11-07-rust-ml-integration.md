# Plan: gomoku-gui on the live rust-ml implementation

Date: 2026-11-07. Status: M0 implemented, pending independent review;
AI milestones remain planned. API notes below checked against live owner code
at `158b6114029d8708fe5d55394b2930f5e4fd1d8c`, not tutorial solutions.

## 1. Goal and constraint

**Goal.** gomoku-gui consumes the Gomoku crates in
`Project-Ellie/rust-ml/gomoku` as libraries, replacing the vendored
engine snapshot and unlocking what that workspace has grown since the
snapshot was taken: MCTS search, a reusable AI player, threat tactics,
and the TSS prover.

**Hard constraint.** The dependency runs one way only:
gomoku-gui → rust-ml. Nothing in rust-ml may depend on, import, or be
reshaped for gomoku-gui. The rust-ml side of the seam is its existing
public API, taken as it stands. If the interface needs an engine
change, that need is reported to the owner, not implemented here
(unchanged since ADR-002).

## 2. Where the two repositories stand today

### gomoku-gui

- M0 removes the former `crates/engine` copy (ADR-008, now superseded)
  in favor of the pinned upstream git crate; a clone needs no sibling checkout.
- `crates/core/src/game.rs` owns the engine `Board` operations behind the
  `Game` facade (ADR-002). Core modules also use engine value types and map
  upstream errors; GUI callers use the core re-exports `Color`, `Move`, and
  `Status`. `PositionError` is now core-owned; `Game::board_snapshot` returns
  a detached upstream `Board` for future analysis.
- The facade consumes `Board::{new, from_position, play, undo, moves, status,
  to_move, stone_at, clone}`, `Color`, `Move`, and `Status`; coordinate creation
  uses `Move::new`. Upstream position errors are mapped at the core boundary.

### rust-ml/gomoku (the "current implementation")

| Crate | Pure? | What it offers the GUI |
|---|---|---|
| `engine` | yes — no Burn, no I/O, no `rand` | rules, `Board::from_position`, tactics (`immediate_wins`, `forced_blocks`, `double_threats`), TSS prover (`prove_forced_win`, `verify_line`), Swap2 typestate machine, serde on `Move`/`Color` |
| `mcts` | yes — engine + `rand` only | single-threaded PUCT search, `Evaluator` seam, `SearchConfig`/`SearchOutcome`, visit-count policy |
| `player` | yes — engine + mcts + `rand` | `AiPlayer::decide`: one fresh search per move, certified own-side TSS win shortcut, guard diagnostics |
| `baseline` | **no — Burn** | hand-written threat-map + gravity evaluator behind the `mcts::Evaluator` seam |
| `patterns`, `train` | **no — Burn** | threat network, training pipeline (future trained-net opponent) |

The former vendored snapshot predates the pinned engine.
Facade adaptations are listed in §5.

## 3. The experience this unlocks

1. **Play against the machine.** `player::AiPlayer` decides a move
   from a `Board`. Certified own forced wins return their first move without
   search statistics; `Decision` does not carry the proof line. Otherwise PUCT
   search uses a configurable simulation budget = difficulty knob.
   At the pin, `DecisionAction` also supports opt-in heuristic resignation;
   applying or persisting resignation is not part of M0.
2. **Live analysis overlays.** The engine's tactics functions mark
   immediate wins, forced blocks, and double threats on the wooden
   board; `prove_forced_win` + `verify_line` draw certified winning
   lines. The rust-ml CLI already ships a TSS proof overlay — the
   precedent for presentation and for keeping proof data out of the
   rules path.
3. **Search insight.** `SearchOutcome` carries visit counts per root
   move: a heatmap or move-ranking panel over the 3D board, turning
   the GUI into an analysis tool for the owner's own agent work.
4. **Swap2 opening, properly.** The engine's typestate `Swap2`
   machine (`Placing3 → FirstChoice → Placing2 → FinalChoice`) maps
   one-to-one onto a guided opening dialog; illegal opening sequences
   become unrepresentable in the UI.
5. **One rules truth, current.** Win detection, Zobrist, position
   validation track the owner's implementation as it develops — no
   snapshot drift, no re-vendoring ritual.

## 4. Architecture

### 4.1 Dependency shape

```
        gomoku-gui workspace                     rust-ml workspace
 ┌───────────────────────────────┐      ┌──────────────────────────┐
 │ gomoku-gui (wgpu/egui/cpal)   │      │ train   patterns         │
 │   render · audio · session    │      │    │       │             │
 │        │                      │      │    └───┬───┘             │
 │        ▼                      │      │      baseline ── Burn ───┤ (tier 1)
 │ gomoku-core                   │      │        │                 │
 │   Game facade (unchanged      │ dep  │        ▼                 │
 │   shape, §5 adaptations) ─────┼─────►│ player  mcts  engine     │ (tiers 0–1)
 │   record · puzzle · settings  │      │        └──────┘          │
 │        ▲                      │      │        ▲                 │
 │        │                      │      │        │                 │
 │ ai-worker thread (new, §4.3) ─┼──────┼────────┘                 │
 └───────────────────────────────┘      └──────────────────────────┘
              direction of every dependency: ◄─── never the reverse
```

The facade stays a module, not a trait (ADR-002 reasoning unchanged:
one consumer, no second implementation). `Game` remains the only
truth about a game; AI and analysis live beside it, not inside it.

### 4.2 Tiers — each independently shippable

The owner has decided (§7): Burn is welcome in the GUI build, and
play-vs-AI is the priority experience. The original Burn-free
tiering collapses — the baseline evaluator is available from the
first AI milestone, no feature gates needed.

**Tier 0 — swap the engine source.**
Remove `crates/engine`; depend on the rust-ml `engine` crate
(§4.4). Adapt the facade to drift (§5), preserving duplicate rejection and
fixing the imported-root draw regression through upstream behavior. No AI is
added. Gate: locked workspace tests, clippy, formatting and standalone build
without the vendored crate or any sibling path dependency.

**Tier 1 — play vs AI (the dream milestone).**
Depend on `mcts` + `player` + `baseline`. The AI worker (§4.3) owns
an `AiPlayer` backed by `BaselineEvaluator` — the full threat-map +
gravity policy from the start, no stub phase. Difficulty knob:
simulation count in `PlayerOptions` (mapped internally to `SearchConfig`).
Delivers: play-vs-AI on the wooden board, certified-win shortcut via
`OwnTssConfig`, guard diagnostics in the log. Start with Burn's genuine Flex
CPU backend, `Device::flex()`; GPU selection or graphics-resource sharing is
not required. See §7 for version coexistence.
Later: a net trained by `train` drops in at the same `Evaluator`
seam without touching the GUI.

**Tier 2 — analysis overlays.**
Engine-only additions: tactics markers and TSS proof lines as
optional render layers; search heatmap from `Decision::search` (`SearchStats`) shown
after a decision completes. The pinned player has no streaming search API;
live progress would require a separate design. Proof overlays need a separate
engine prover call because `Decision` carries no proof line. Pure presentation
work in `render/`, fed by engine calls and completed decision statistics.

### 4.3 Concurrency: one AI worker, message-passing only

`mcts` is deliberately single-threaded — the integration preserves
that and puts the one search thread *outside* the render loop:

```
winit event loop (render + egui)          AI worker thread
────────────────────────────────          ─────────────────────────────
user clicks "let AI move"        ─────►   Request { board, gen, cfg }
(render never blocks)                     AiPlayer::decide(&board, &options, &mut rng)
                                          …search runs (ms–s)…
poll channel each frame        ◄─────   Reply { gen, decision }
if gen == current_gen: handle DecisionAction (Play / Resign)
```

- Construct `AiPlayer` and its evaluator **inside** the worker thread.
  `AiPlayer` holds `Box<dyn Evaluator>` without a `Send` bound and is non-Send;
  moving a player constructed on the UI thread into the worker will not work.
  The worker reuses its evaluator and receives a **cloned `Board`** via
  `Game::board_snapshot()`; no shared mutable game state, no locks.
- **Generation counter**: new game, undo, rewind, or seek increments
  `gen`; stale replies are dropped. That is the entire cancellation
  story — no thread kills mid-search.
- Board render reads only `Game`; analysis overlays are computed
  on-demand per frame from the engine's pure functions (cheap:
  tactics are bitboard scans) or from the last `Reply`'s stats.
- Audio (stone knock on AI move) triggers off `Game::play` as it does
  for human moves — the worker never touches audio or GPU.

### 4.4 Dependency mechanism: pinned git revision (owner decision)

ADR-008 vendored the engine so the repository stands alone, justified
by "the owner does not intend to develop the interface further." That
premise is now reversed: tracking the live implementation *is* the
point. The owner has chosen a **pinned git dependency**:

```toml
engine   = { git = "https://github.com/Project-Ellie/rust-ml", rev = "158b6114029d8708fe5d55394b2930f5e4fd1d8c" }
mcts     = { git = "https://github.com/Project-Ellie/rust-ml", rev = "158b6114029d8708fe5d55394b2930f5e4fd1d8c" }
player   = { git = "https://github.com/Project-Ellie/rust-ml", rev = "158b6114029d8708fe5d55394b2930f5e4fd1d8c" }
baseline = { git = "https://github.com/Project-Ellie/rust-ml", rev = "158b6114029d8708fe5d55394b2930f5e4fd1d8c" }
```

ADR-008 verified that Cargo resolves crates inside the nested
`gomoku/` workspace from a git dependency, so one `rev` pin covers
all four. M0 declares all four at workspace level but consumes only `engine`;
the others do not enter the lockfile until used. The repository builds
standalone again (Cargo fetches the dependency). Tracking the live
implementation is an explicit, deliberate act: bump the rev, adapt the facade if the API moved,
run the gates. That bump ritual replaces the old re-vendoring
ritual — same cost, but now the update is *visible* in one diff
line instead of a 4,000-line snapshot copy.

ADR-008 gets a "Superseded" header pointing here.

Crate names coincide (`engine` in both workspaces) — no collision
once the vendored copy is deleted; the dependency simply *is* the
rust-ml crate.

## 5. Reconciling the drift (Tier 0 work list)

Concrete deltas between the vendored snapshot and current
`rust-ml/gomoku/crates/engine`, and what the facade does about each:

1. **`PositionError::DuplicateStone` no longer exists upstream.**
   `Game::from_position` retains its same-color duplicate pre-check and returns
   core-owned `PositionError::DuplicateStone`. Upstream overlap and count
   errors map exhaustively to `PositionError::{OverlappingColors, InvalidCounts}`.
   Each color is checked separately so overlap is not mislabeled a duplicate.
   `GameError` remains the placement error type.
2. **Draw detection on play** now uses occupied cells via `empty_moves()`
   instead of the played-history length. This fixes the imported 224-stone
   puzzle root: the final placement draws even though history has one move.
   Regression coverage includes rewind/forward and destructive undo/replay.
3. **serde on `Move`/`Color`** is now available upstream. The record
   format stays GUI-owned and versioned (ADR-003 unchanged) — but
   `StoredColor`/`StoredMethod` mapping code may optionally simplify.
   Optional, not part of Tier 0's gate.
4. **Swap2 was refactored into a stricter typestate.** Irrelevant to
   the current facade (the GUI doesn't run Swap2 yet); it *defines*
   the shape of the future opening dialog (§3.4).
5. **`utils` feature** exists upstream, off by default. The GUI does
   not enable it.
6. **`Board::from_position`** takes `&[Move]`, `&[Move]`, `Color` and returns
   `Result<Board, engine::PositionError>`. It computes root status and Zobrist
   with empty move history. `Game` continues to preserve the imported root.
7. **Immutable snapshot access** is `Game::board_snapshot(&self) -> Board`,
   an owned clone at the review cursor, not mutable access to the live board.

## 6. What rust-ml must not become

- `engine` remains a dependency island: no Burn, no I/O, no `rand` —
  the GUI changes nothing about that; it merely consumes.
- `mcts` stays Burn-free and single-threaded; the GUI's worker-thread
  placement is a GUI-side choice, not an mcts feature.
- No GUI types (`egui`, `winit`, config, record format) leak into any
  rust-ml crate. No "for the GUI" features, cfgs, or re-exports.
- If the facade genuinely needs an engine change, it is reported to
  the owner (engine is tutorial material) — ADR-002's rule, carried
  forward verbatim.

## 7. Owner decisions (2026-11-07)

1. **Mechanism — pinned git revision.** No path dependency, no
   sibling checkout required; the repository stands alone again and
   engine updates arrive as deliberate rev bumps. See §4.4.
2. **Burn — welcome.** No Burn-free constraint and no feature
   gates; `baseline` is a direct dependency from Tier 1 on. One
   implementation choice for Tier 1 is the genuine Burn Flex CPU backend,
   already used by the upstream baseline. Multiple wgpu versions CAN coexist
   in one binary; their types are distinct. Only sharing GPU resources between
   Burn and the GUI would require compatible APIs. No GPU alignment or fake
   evaluator is necessary to begin baseline play.
3. **First experience — play vs AI.** Tier 1 is the dream
   milestone: a worthy opponent on the wooden board. Analysis
   overlays (Tier 2) follow after.
4. **Records of AI games — move-only for now.** The record format
   (ADR-003) is untouched by this integration: an AI game is saved
   like a human game, with the AI named as the player. Search
   diagnostics go to the log. An optional, versioned format
   extension carrying player-type/search-stats metadata remains
   possible later, but play-vs-AI must not wait for a format
   migration.

## 8. Milestones and gates

| # | Milestone | Gate |
|---|---|---|
| 0 | ADR-008 superseded; vendored engine removed; pinned git declarations wired (only engine consumed); facade adapted (§5) | Locked workspace tests/clippy, fmt check, standalone build; exactly one git engine and no sibling path dependency |
| 1 | AI worker + `AiPlayer`/`BaselineEvaluator`; egui "AI move" affordance; Burn/wgpu coexistence verified | play a full game vs the machine on the wooden board; undo/seek during search drops stale replies |
| 2 | Tactics + separately requested TSS proof overlays; completed-search heatmap | overlays render from engine calls/completed statistics; no streaming or proof line in `Decision` |
| 3 | Swap2 opening flow on the typestate machine | illegal opening sequences unrepresentable in UI |
| 4 | (optional) AI-aware record metadata, versioned format extension | old records still load; new fields optional |
