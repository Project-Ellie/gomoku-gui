# Gomoku GUI — the training laboratory

- Date: 2026-11-08
- Status: design proposal for owner review. **Not locked.** This document
  supersedes the TUI-based predecessor in
  `Project-Ellie/rust-ml/docs/specs/2026-11-08-gomoku-lab-design.md`.
  Where documents disagree, the authority order is: chapters 12/13/14 locked
  decisions → the mentor-datagen spec → this spec.
- Owner: Wolfie
- Repository: `Project-Ellie/gomoku-gui`

## Abstract

This spec turns the native Gomoku GUI into the project's training laboratory
("the lab"). The lab does not train; rust-ml's `nettrain` remains the only
authority that writes model records and run journals. The lab is the
observatory, control room, and model workbench: it browses corpora, compares
runs, promotes candidates, assigns models to seats, launches `nettrain`, watches
live loss curves, and resumes from snapshots — all through native macOS
affordances (menu bar, file dialogs, forms, rendered charts) and always on the
real wooden board with stones.

The terminal study table in rust-ml keeps its role as the developer's minimal
environment. The lab is the GUI.

## Glossary

- **Lab** — the set of training-laboratory capabilities added to gomoku-gui.
- **Lab mode / lab window** — a top-level screen family reachable from the GUI
  menu bar: corpora, runs, candidates, launcher/watch, resume manager
  [invention].
- **Corpus** — a directory of length-delimited bincode shards plus a JSON
  `manifest.json`, per the datagen tutorial's storage discipline.
- **Run** — a `nettrain` output directory holding `model.mpk` (full-precision
  model record), `run.json` (run journal), and optionally `metrics.jsonl`
  (per-step/per-eval metrics) [invention].
- **Run journal** — `run.json`: seed, `ModelConfig`, `TrainConfig`, final
  step/eval metrics. The unit of comparison between runs.
- **metrics.jsonl** — proposed run-metrics record: one JSON line per training
  step and per holdout evaluation [invention].
- **Candidate** — a run directory promoted to a named, tagged model that can be
  sent to a seat [invention].
- **Snapshot** — a full training-state checkpoint at an intermediate step:
  model record + optimizer state + LR scheduler state + RNG/step position +
  corpus identity + lineage metadata, enabling resume [invention].
- **Seat** — one of the two sides (Black/White) in the GUI; a lab-assigned seat
  uses a `NetEvaluator` behind the `mcts::Evaluator` seam.
- **Overlay renderer** — the GUI's existing board overlay machinery (coordinates,
  move numbers, last-move ring, winning line), extended to render policy/value
  heatmaps on the real board.
- **Generation counter** — the GUI session's existing invalidation counter that
  retires stale AI work; reused by lab views to drop stale derived results.
- **Claim labels** — [paper] / [derived] / [experiment] / [invention] per
  chapter 12; this spec adds [measured] for values directly observed on this
  project.

## 1. Why the lab lives in gomoku-gui

The GUI already owns the project's best visualization surface: a rendered
wooden board, real stones, and a per-color seat model. The rust-ml study table
is intentionally minimal — it is the developer's fallback. Moving the lab to the
GUI exploits four native affordances the terminal cannot match:

1. **The board as the universal renderer.** A corpus sample's policy target, a
candidate's raw network policy, or a run's recorded prior is most informative
when painted as a heatmap over the stones. The GUI already projects overlays
onto the board; lab views feed them a new data source.
2. **Native file dialogs and forms.** Selecting a corpus or run directory uses
`rfd` instead of typed paths. Parameter editing uses real egui forms with
validation, not hand-rolled TUI widgets.
3. **Rendered charts.** Loss curves are drawn with a 2D plotting layer, not
ASCII or braille.
4. **Per-seat candidate dispatch.** The GUI already has per-color ownership and
search controls; assigning a different candidate to Black and White is a natural
extension of that model.

The lab therefore lives in `Project-Ellie/gomoku-gui`, consumes the same
artifact formats as rust-ml, and leaves rust-ml's training loop untouched.

## 2. Two-repo split of responsibilities

```text
rust-ml (training authority)          gomoku-gui (the lab)
─────────────────────────────────     ─────────────────────────────────
net::train writes model.mpk,           reads manifests/journals/metrics
run.json, metrics.jsonl                from local directories
net::checkpoint defines snapshots      renders charts, board overlays,
& resume format                        candidate registry UI
nettrain runs the loop                 launches nettrain as child process
engine/mcts/baseline/player            assigns NetEvaluator to seats via
provide search & tactics               existing AI worker seam
```

Dependency direction stays one-way: gomoku-gui → rust-ml. The GUI extends its
existing pinned-git dependency to include the `net` crate so it can load a
`Model` and wrap it in a `NetEvaluator` without a sibling rust-ml checkout
[invention — amends the integration plan]. rust-ml receives no GUI types, no
extra features, and no I/O changes beyond writing the artifact files it already
owns.

## 3. Lab modes and screens

Each mode is a native window or panel reachable from a new **Lab** menu. None
replace the play board; they are parallel views that can open beside or over the
main game window.

### 3.1 Corpora browser [invention]

- Select a corpus root (default `data/`) through a native file dialog or a
  remembered lab setting.
- List corpora; open one to show its `manifest.json` (seed, quotas, counts,
  shards, workspace version).
- Browse samples by index or filter by class (win/block/quiet).
- Render a sample as a board position using `Game::from_position` — the same
  path already used for puzzles.
- Overlay the sample's policy target (π) as a heatmap on the real board. The
  overlay renderer gains a generic "policy/value per intersection" data source
  [invention].
- For future mentor/game-record corpora: replay a game to a chosen ply and
  inspect stored visit distributions, priors, root values, and certified
  suffixes.

### 3.2 Run registry [invention]

- Select a run root (default `artifacts/gomoku/runs/`).
- Table view: name, seed, config preset, steps, final loss, final holdout
  metrics, tags.
- Detail view: full journal, rendered loss curve from `metrics.jsonl`, learning
  rate, and holdout metric curves.
- Diff view: pick two runs; show every changed config field and overlay their
  loss curves.
- Jump to the run's output corpus when `data_dir` is recorded in the journal
  [invention].
- Tag runs (`baseline`, `candidate`, `failed`, `experiment`) in a lightweight
  sidecar file inside the lab data directory [invention].

### 3.3 Candidate registry [invention]

- **Promote** a run to a candidate: choose a display name, tags, and notes. The
  registry stores only metadata pointing at the run directory; it does not copy
  `model.mpk`.
- List candidates with source run, final metrics, and model-file presence.
- **Assign to seat**: install a candidate as the evaluator for Black, White, or
  both. The GUI builds a per-seat `NetEvaluator` from the candidate's run
  directory and routes that seat's AI worker through it.
- **Unload**: revert a seat to its configured evaluator kind (`baseline` or
  tactics, in the first phases).
- **A/B watch**: load any position (from the current game, a sample, or a
  puzzle) and toggle between two candidates' policy overlays on the board to
  compare their priors.

### 3.4 Training launcher + live watch [invention]

- Native form for `nettrain` flags: output directory, data directory (file
  dialog), config preset (`tiny`/`mid`/`default`), device (`flex`/`wgpu`), seed,
  steps, batch size, eval interval. The GUI validates ranges before launch.
- Launch `nettrain` as a child process. The first implementation does not link
  Burn's training path into the GUI binary.
- Live watch window: rendered loss curve from `metrics.jsonl`, a tail of the
  run's stderr heartbeat, and an **Abort** button that sends SIGINT. Polling,
  no direct Burn coupling.
- On completion or abort, offer to promote the run to a candidate.

### 3.5 Resume manager [invention]

- Snapshot timeline per run: step, timestamp, corpus identity, parent run.
- Inspect snapshot metadata and verify that the model, optimizer, and scheduler
  records are present.
- **Resume from snapshot**: choose a snapshot and a new output directory, then
  launch a new run that continues from that state. The new run's journal carries
  `parent_run` pointing at the source run.
- Watch the resumed run in the same live-watch window.

## 4. Resume design and snapshot format

Current `nettrain` writes only a final model and journal. Burn 0.22 provides the
pieces for full training-state snapshots: `ModuleOptimizer` state implements
`RecordState` (burnpack-serializable), `ModuleLrScheduler` exposes
`LrSchedulerRecord`, the batcher uses a seeded deterministic counter, and the
`DataLoader` shuffle is seeded. A full resume additionally needs the model
record at that step, optimizer state, scheduler state, RNG/step position, corpus
identity, and journal lineage.

**The predecessor rust-ml lab spec omitted the intermediate model record from
its snapshot listing. That is fixed here: a snapshot must include the model
record at that step.** Optimizer and scheduler records alone cannot reproduce
the trajectory.

Proposed snapshot layout inside a run directory [invention]:

```text
artifacts/gomoku/runs/<run-name>/
├── model.mpk              # final model record (existing)
├── run.json               # run journal (existing)
├── metrics.jsonl          # per-step + per-eval records (Phase 0)
└── snapshots/
    ├── snapshot-0001.json       # snapshot metadata
    ├── snapshot-0001-MODEL.mpk  # model record at this step
    ├── snapshot-0001-OPT.mpk    # optimizer state record
    └── snapshot-0001-SCHED.mpk  # LR scheduler state record
```

Snapshot metadata fields:

| Field | Meaning |
|---|---|
| `step` | Step completed when the snapshot was taken. |
| `corpus` | Absolute or workspace-relative path to the corpus directory. |
| `rng_seed`, `rng_offset` | Position in the deterministic RNG stream. |
| `batcher_counter` | Position in the deterministic batcher counter. |
| `parent_run` | Optional path to the run this run resumed from. |
| `timestamp` | ISO timestamp of snapshot creation. |

**Journal lineage:** `run.json` gains optional `parent_run` and `data_dir`
fields [invention — amends the existing `RunJournal` contract, which is not a
locked architecture decision]. `data_dir` lets the run registry jump from a run
to its corpus.

**Snapshot cadence:** write a snapshot at every eval interval plus on an
explicit user command [invention]. Disk cost is dominated by the model record;
keeping a handful of optimizer snapshots is acceptable.

**Resume contract:** loading a snapshot and continuing training on Flex CPU
reproduces the loss trajectory of an uninterrupted run from the same seed and
corpus. GPU resume is not parity-verified; Flex remains the deterministic
reference.

## 5. Reuse of existing GUI machinery

The lab deliberately builds on existing contracts rather than inventing parallel
ones.

### 5.1 Game state and `Game::from_position`

Corpus samples and puzzle positions both load through `Game::from_position`,
which already supports an imported root, correct side-to-move, and a review
cursor that cannot undo past the root. No new core state machine is needed.

### 5.2 Seat controller and AI worker

The GUI's existing `ai::Controller` and `ai_worker::Worker` already route one
seat at a time through a decider built inside the worker thread. The lab extends
the controller to hold a per-seat evaluator assignment (baseline/tactics/net
candidate) and builds the worker with a `NetEvaluator` when a candidate is
assigned [invention — amends the current v1 limitation "no per-seat networks"
in `docs/how-to-first-model-in-the-cli.md`]. The generation counter retires
stale work exactly as it does today.

### 5.3 Overlay renderer

The GUI's existing overlay renderer draws text and markers projected onto board
intersections. For lab views it gains a generic heatmap input: a 225-element
vector of floats plus a color scale. MCTS visit counts, corpus policy targets,
candidate priors, and run-recorded priors all feed this same input
[invention]. The renderer itself stays board-centric.

### 5.4 Native dialogs and egui forms

`rfd` handles directory selection. egui handles parameter forms, validation
feedback, and live charts. The lab does not add a new UI framework.

## 6. Crate-level contract changes

This section states responsibilities only; it does not contain implementation
code. The rust-ml tutorials remain the owner's learn-by-doing material.

### 6.1 `gomoku-gui` (binary crate)

New responsibilities:

- Add a **Lab** menu and lab window/modal machinery.
- Add artifact readers for `manifest.json`, `run.json`, and `metrics.jsonl`
  (file-system only, no new database).
- Add chart rendering for loss curves and metric timelines.
- Extend the overlay renderer with a policy/value heatmap data source.
- Extend the seat controller to record per-seat evaluator kind and candidate
  assignment.
- Extend the AI worker factory to build either baseline or net deciders per
  seat.
- Add child-process management for launching and aborting `nettrain`.
- Add lab settings persistence (corpus root, run root, recent lab folders) in
  the existing config directory.

No new rust-ml crate dependencies beyond extending the existing git pin to
`net`.

### 6.2 `gomoku-core`

No new responsibilities. The lab uses the existing `Game`, record format, and
settings schema. The core crate stays Burn-free and I/O-light.

### 6.3 `net` (rust-ml)

New responsibilities:

- Define and write `metrics.jsonl` records during `train`.
- Define and write snapshot records (model + optimizer + scheduler + metadata)
  during `nettrain`.
- Support resuming from a snapshot: load model + optimizer + scheduler +
  RNG/step position into the training loop.
- Extend `RunJournal` with optional `parent_run` and `data_dir` fields.

These changes live in `net::checkpoint` and `net::train`; `network` and `loss`
are untouched.

### 6.4 `train` (rust-ml)

New responsibilities:

- Provide a public reader for a single sample by index within a corpus.
- Provide a public function to reconstruct a sample's board and policy target.
- Provide an iterator over samples matching a class filter.

No format change to `manifest.json` or shard files is required for the first
phases.

### 6.5 `mcts` and `engine` (rust-ml)

No changes required for the first phases. The lab can function with the current
`mcts::EvalRequest` seam that carries only `me`/`you` planes; the network's
last-move planes will be zero-filled, matching the existing v1 limitation.

If history-plane enrichment is adopted later, `mcts::EvalRequest` and
`engine::encode`/`Planes` would need a coordinated amendment. This is flagged
below as a locked-decision amendment, not silently relitigated.

### 6.6 `cli` (rust-ml)

No lab UI changes. The terminal study table remains the developer's minimal
environment and benefits from `metrics.jsonl` for simple command-line
inspection, but the lab itself is in the GUI.

## 7. Phasing and acceptance gates

Each phase leaves both repos in a usable, gate-verified state.

### Phase 0 — metrics.jsonl + stderr heartbeat (rust-ml)

**Goal:** make training runs observable without changing the GUI.

**Changes:**
- `net::train::train` writes `metrics.jsonl` in the run directory.
- `nettrain` emits a compact stderr heartbeat each step and each eval.

**Acceptance gates:**
1. A `nettrain` run produces a valid `metrics.jsonl` with one line per step
   and one line per eval.
2. The file parses with the defined schema and matches the final `run.json`
   metrics.
3. `cargo test`, `cargo clippy --all-targets -- -D warnings`,
   `cargo fmt --all` are green in rust-ml.

### Phase 1 — read-only inspection (GUI)

**Goal:** browse corpora and runs from the GUI without launching training.

**Changes:**
- Lab menu + corpora browser + run registry.
- Corpus browser: manifest view, sample list, sample board render with policy
  overlay.
- Run registry: list, journal view, rendered loss curves from `metrics.jsonl`,
  run diff.

**Acceptance gates:**
1. Selecting a corpus renders its manifest and any sample as a board with its
   policy target overlay.
2. Selecting a run shows its journal and a rendered loss curve.
3. Diffing two runs reports every changed config field and overlays their
   curves.
4. The existing play-vs-baseline flow continues to work unchanged.
5. gomoku-gui's existing gates (`cargo test --workspace --locked`, clippy,
   fmt) remain green.

### Phase 2 — snapshots + resume (rust-ml + GUI)

**Goal:** resume training from an intermediate state.

**Changes:**
- `net::checkpoint` defines snapshot records and writes them during
  `nettrain`.
- `nettrain` gains a resume path from a snapshot directory.
- `RunJournal` gains `parent_run`.
- GUI resume manager: snapshot timeline and resume launch.

**Acceptance gates:**
1. A run interrupted at step S and resumed from its latest snapshot reproduces
   the same loss trajectory on Flex as an uninterrupted run from the same seed
   and corpus.
2. The resumed run's `run.json` points to the parent run.
3. Snapshot files are full-precision and round-trip correctly.
4. All gates from Phase 1 remain green.

### Phase 3 — candidate registry + training launcher/watch with native forms (GUI)

**Goal:** promote runs, dispatch models to seats, and launch training from the
GUI. Because native forms are the GUI's natural parameter editor, forms arrive
here rather than in a later phase.

**Changes:**
- Candidate registry metadata file under the lab data directory.
- Promote, assign-to-seat, and unload commands.
- Training launcher as a native form that shells out to `nettrain`.
- Live watch window: rendered loss curve, stderr tail, abort.

**Acceptance gates:**
1. Promoting a run creates a named candidate that survives GUI restart.
2. Assigning a candidate to White only changes White's evaluator; unloading
   reverts to the configured evaluator kind.
3. The launcher form refuses invalid ranges before spawning `nettrain`.
4. Launching a run opens the live watch window and updates the loss curve as
   `metrics.jsonl` grows.
5. Aborting a run from the GUI leaves a usable partial run directory.
6. All gates from Phase 2 remain green.

### Phase 4 — resume forms, lab settings, and A/B watch polish (GUI)

**Goal:** generalize native forms to the remaining lab actions and refine
model comparison.

**Changes:**
- Resume manager form (snapshot picker + new output directory).
- Lab settings form (corpus root, run root, default device).
- A/B watch comparison view with synchronized board and side-by-side
  policy/value readouts.

**Acceptance gates:**
1. The resume form validates that a snapshot contains all required records.
2. The settings form persists and reloads lab roots across launches.
3. A/B watch toggles between two candidates without changing the underlying
   game state.
4. `Esc` cancels any form without side effects.
5. All gates from Phase 3 remain green.

## 8. Explicit non-goals

- **No second metadata store.** The lab reads `manifest.json` and `run.json`
  directly. The only added metadata is the lightweight candidate registry and
  run tags, stored inside gomoku-gui's lab data directory [invention].
- **No arena or Elo machinery.** Milestone 6 owns match play, rating, and
  promotion gating. The lab may display candidate tags, but it does not compute
  Elo or run arena sessions.
- **No GPU parity work.** GPU training correctness is noted as an adjacent
  concern (Burn issues #5162, #5626); the lab does not try to fix or verify it.
  Flex CPU remains the deterministic reference.
- **No distributed training.** The lab targets a single machine (Mac Studio).
  Multi-process mentor datagen is handled by shelling out multiple `selfplay`
  processes, not by the lab.
- **No replacement of `nettrain` as the training authority.** The GUI may
  launch `nettrain` and read its artifacts, but the training loop stays in
  `net::train`.
- **No change to rust-ml locked decisions without flagging.** Any amendment to
  the `mcts::EvalRequest` seam or other locked contracts is listed in §9 and
  requires explicit owner approval.
- **No telemetry or remote reporting.** All lab state is local files.
- **No automated hyperparameter search.** The launcher exposes `nettrain`
  flags; it does not sweep them.

## 9. Honesty conventions

Numeric and empirical claims are labelled:

- **[measured]** — directly observed on this project.
- **[derived]** — computed from measured values.
- **[experiment]** — free parameter chosen by measurement.
- **[invention]** — design choice not derived from a locked decision.

Locked decisions cited in this spec:

- Chapter 12 §6: `mcts` must not depend on `net`; the `Evaluator` trait is the
  seam.
- Chapter 12 §9: store games, not positions; replay to sample.
- Chapter 12 §13 milestones: milestone 6 owns arena/Elo.
- Chapter 14 D2: phase-0 supervised pre-training before self-play.
- Chapter 14 D6: mentor records join tier 3 alongside external engine records.
- `docs/how-to-first-model-in-the-cli.md` v1 limitations: last-move planes
  zero-filled, net seat runtime-only, no per-seat networks, no resume,
  overwrite hazard.

Proposed amendments to existing contracts, flagged explicitly:

1. **Extending the gomoku-gui rust-ml pin to the `net` crate** (§2, §6.1):
   amends `docs/plans/2026-11-07-rust-ml-integration.md`, which currently pins
   only `engine`, `player`, `mcts`, and `baseline`.
2. **Per-seat network assignment in the GUI** (§5.2, §6.1): amends the v1
   limitation "no per-seat networks" in
   `docs/how-to-first-model-in-the-cli.md`.
3. **`RunJournal` lineage fields** (§4, §6.3): extends the existing `run.json`
   schema with `parent_run` and `data_dir`.
4. **Snapshot includes the intermediate model record** (§4): fixes the gap in
   the predecessor rust-ml lab spec, which listed optimizer + scheduler +
   metadata but omitted the model record needed to reproduce the trajectory.
5. **History-plane enrichment of `mcts::EvalRequest`** (§6.5): would amend the
   current seam that carries only `me`/`you` planes. Not required for the
   lab's first phases; flagged for later decision.
6. **GUI overlay renderer as a generic policy/value heatmap carrier** (§5.3):
   extends the internal GUI meaning of the overlay layer without changing its
   core drawing primitives. Not a locked contract.

## 10. References

**Project documents — gomoku-gui:**

- [README.md](../../../README.md) — current capabilities, crate layout, build
  gates, and future milestones.
- [docs/specs/2026-09-22-gomoku-gui-design.md](2026-09-22-gomoku-gui-design.md)
  — approved UI architecture, core/gui split, state model, persistence,
  rendering.
- [docs/plans/2026-11-07-rust-ml-integration.md](../plans/2026-11-07-rust-ml-integration.md)
  — pinned git dependency shape, tiering, AI worker design, owner decisions.
- [docs/architecture/03_PERSISTENCE.md](../architecture/03_PERSISTENCE.md) —
  record format and validation rules.
- [docs/decisions/ADR-002-engine-reuse.md](../decisions/ADR-002-engine-reuse.md)
  — engine reuse rule and the report-don't-edit convention.
- [docs/decisions/ADR-008-engine-vendored.md](../decisions/ADR-008-engine-vendored.md)
  — superseded vendoring decision, replaced by the pinned-git plan.

**Project documents — rust-ml:**

- [docs/specs/2026-11-08-gomoku-lab-design.md](../../../../rust-ml/docs/specs/2026-11-08-gomoku-lab-design.md)
  — TUI-based predecessor spec; this document supersedes it.
- [docs/specs/2026-11-08-mentor-datagen-design.md](../../../../rust-ml/docs/specs/2026-11-08-mentor-datagen-design.md)
  — format precedent (status block, glossary, design narrative, inventions,
  acceptance gates, non-goals, references) and game-record format.
- [docs/12-gomoku-architecture.md](../../../../rust-ml/docs/12-gomoku-architecture.md)
  — locked crate map, `Evaluator` seam, store-games discipline, milestones,
  honesty convention.
- [docs/14-openings-and-supervised-curriculum.md](../../../../rust-ml/docs/14-openings-and-supervised-curriculum.md)
  — signal ladder, decisions D2/D3/D5/D6.
- [docs/how-to-first-model-in-the-cli.md](../../../../rust-ml/docs/how-to-first-model-in-the-cli.md)
  — current training/loading manual and v1 limitations.

**Source files consulted:**

- `gomoku-gui/crates/core/src/game.rs` — `Game::from_position`,
  `board_snapshot`, history/cursor model.
- `gomoku-gui/crates/core/src/record.rs` — versioned JSON record format.
- `gomoku-gui/crates/gui/src/session.rs` — seat ownership, AI eligibility,
  dialog model, generation counter.
- `gomoku-gui/crates/gui/src/ai.rs` — per-seat options, `AiRequest`,
  `Controller`.
- `gomoku-gui/crates/gui/src/ai_worker.rs` — thread-confined worker factory.
- `gomoku-gui/crates/gui/src/ui.rs` — egui menu bar, side panel, overlay
  drawing.
- `rust-ml/gomoku/crates/net/src/checkpoint.rs` — `RunJournal`, model save/load.
- `rust-ml/gomoku/crates/net/src/train.rs` — `StepMetrics`, `EvalMetrics`,
  training loop.
- `rust-ml/gomoku/crates/net/src/bin/nettrain.rs` — command-line flags.
- `rust-ml/gomoku/crates/cli/src/net_eval.rs` — `NetEvaluator` and the current
  `EvalRequest` seam limitation.
- `rust-ml/gomoku/crates/mcts/src/eval.rs` — `EvalRequest` definition.
