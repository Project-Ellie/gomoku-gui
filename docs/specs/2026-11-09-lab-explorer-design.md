# Lab Explorer — universal record browser, branching, and AI labeling

- Date: 2026-11-09
- Status: approved by owner after formal brainstorming (2026-11-08/09
  session). This spec **amends**
  [2026-11-08-gomoku-gui-lab-design.md](2026-11-08-gomoku-gui-lab-design.md):
  it adds the Explorer (universal file opening), the variation tree with
  branch UI, AI labeling, and the `.gomoku` config directory. The lab
  spec's modes (run registry, candidates, launcher, resume manager)
  remain as written. Authority order unchanged: chapters 12/13/14 locked
  decisions → mentor-datagen spec → lab spec → this spec.
- Owner: Wolfie
- Repository: `Project-Ellie/gomoku-gui`

## 0. Philosophy and mission — read this first, every session

**The Lab is an exploratory journey, not a fixed contract.** We are
turning the existing gomoku-gui client into the project's training
laboratory, step by step, learning as we go. We expect to discover new
things, amend course, rethink priorities, and possibly drop goals that
prove too ambitious. This document records the *current* north star;
the implementation plan's milestone log records what we actually
learned. Any new session working on the Lab starts by reading this
section and the milestone log.

**Mission:** one native app where Wolfie can open any game, position,
or sample file the project understands, explore it on the real wooden
board, branch off any position, and turn what the AI sees into training
data — without leaving the board.

**Foundation rule:** we extend the existing application; we do not
develop from scratch. The rendered board, the Go-menu navigation, the
menu bar and side panel, the file dialogs, the AI worker seam, the
versioned record format, and crash-safe session discipline already
exist and are reused (see §2).

## Glossary

- **Explorer** — the new lab mode added by this spec: unified open
  dialog, record list, board view, branch UI, labeling controls.
- **Format** — a known file/directory type with a `sniff` (detection)
  and `load` (parse) implementation in the registry.
- **Record** — one browsable unit: a game, a position, or a sample.
- **Corpus** — a directory treated as a collection of records (a psq
  tree, a sample-shard directory).
- **Variations** — the persistent variation tree in `core`: arena of
  nodes (parent, move, children) plus a cursor; semantics mirror the
  rust-ml CLI's history.
- **Branch badge** — on-board ghost stone + numbered marker on each
  child move of a branching node.
- **Labeling job** — a managed worker-pool run that annotates positions
  with MCTS results and writes mentor-format game records.
- **Mentor game record** — the durable format locked by the
  mentor-datagen spec: header (configuration, provenance), per-ply
  records (move played, visit distribution π, root value, prior), game
  outcome, TSS-certified endgame suffixes.
- **`.gomoku` directory** — the lab's config directory, default
  `./.gomoku`, overridable with `--config`; holds `config.json` with the
  artifact roots.
- **Claim labels** — [invention] / [measured] per the lab spec.

## 1. Relationship to existing documents

The lab spec (2026-11-08) designed corpus browsing for *sample shards*
only, plus run registry, candidates, launcher, and resume. This spec
plugs its largest gap — it could not open game files at all — and adds
branching and labeling. Decision record from the brainstorming session:

| Question | Decision |
|---|---|
| Relationship to lab spec | **Extend it** — Explorer is a new lab mode; old modes stay |
| Foundation | Extend the existing gomoku-gui app, never rewrite |
| Format registry v1 | psq + GUI record JSON + VCF puzzle JSON + sample-shard dirs |
| Open semantics | Files **and** directories, recursively indexed |
| Branch UI | On-board badges **and** collapsible variation panel |
| Label content | Full mentor game-record format (π, root value, priors, outcome) |
| Labeling execution | In-process worker pool on the AI-worker seam |
| Config | Single `config.json` + dedicated config panel with raw-JSON tab |

## 2. What we reuse (the existing application)

- **The board** — wgpu wooden board, slate/shell stones, contact
  shadows, knock audio, camera/fit/flip; overlays (coordinates, move
  numbers, last move, winning line). Every record renders here.
- **Go-menu navigation** — to start / back / forward / to latest
  (`session.seek/rewind/forward`). This *is* the in-game navigation;
  the Explorer rides on it, re-powered by the variation tree (§5).
- **Chrome** — menu bar, resizable right panel, `rfd` native dialogs,
  egui forms, status line in the menu bar.
- **`core` domain** — `Game` with outcome, versioned `Record` JSON,
  `puzzle.rs` VCF loader, `config.rs` settings schema, `storage.rs`
  recents/autosave discipline.
- **AI worker seam** — dedicated worker running pinned baseline
  evaluation on Burn's Flex backend; per-color ownership and search
  controls.
- **Crash-safe sessions** — labeling jobs inherit this discipline (§6).

## 3. Architecture and placement

Domain logic goes to `crates/core`; presentation to `crates/gui`
(owner-approved Option 1). No new crate.

```text
crates/core                          crates/gui
─────────────────────────────────    ────────────────────────────────
format registry (sniff/load)         Explorer mode (record list, menus)
record model (Game/Position/Sample)  branch badges on the board
corpus indexer (lazy, background)    collapsible variation panel
Variations tree                      labeling dialog + control room
labeling scheduler (worker pool)     config panel (+ raw JSON tab)
.gomoku config read/write            live-watch of labeling on board
```

The variation tree is **reimplemented** in `core` (~250 lines, arena +
cursor), shaped by the rust-ml CLI's proven semantics — the `cli` crate
itself cannot be depended upon (it drags in the whole REPL).

## 4. Format registry and record model

```rust
// Sketch, not API-locked.
trait Format {
    fn sniff(&self, path: &Path, head: &[u8]) -> Confidence;
    fn load(&self, path: &Path) -> Result<Vec<Record>, LoadError>;
}

enum RecordKind {
    Game,                 // header + move list (+optional annotations)
    Position,             // stones + to_move (+optional claimed solution)
    Sample,               // position + policy targets + value target
}

enum OpenedThing {
    SingleRecord(Record),
    Corpus { records: Vec<RecordMeta>, source: PathBuf },
}
```

- **v1 formats**: psq (new), GUI record JSON, VCF puzzle JSON (existing
  loader moves behind the registry), sample-shard directories (reader
  per the lab spec). The three existing readers are *folded into* the
  registry — no parallel readers survive.
- **Detection**: extension vote + content probe (magic/header/parse
  probe). Directories get a *role* sniff: psq collection / shard corpus
  / mixed (mixed directories are grouped by detected type, each group
  browsable separately).
- **Directories** open as corpora, recursively. Indexing is **lazy**:
  opening a 13k-file psq tree lists files instantly; per-record
  metadata (plies, result, players) is parsed on demand, with a
  background indexer filling the table progressively [invention].
- **Unrecognized files** land in an "unrecognized" list with the sniff
  failure reason. Never a crash, never silently dropped.

## 5. Navigation and branching

- The Explorer opens with the record list (table: index, type, plies,
  result, source file; filter box; jump-to-index field) beside the
  board.
- Selecting a **game** shows its **end position**; navigation is the
  existing Go-menu/keyboard rewind/forward, backed by `Variations`.
- Selecting a **position** or **sample** shows it; samples additionally
  render their policy target as a heatmap overlay (the lab spec's
  overlay extension). Psq games have no target until labeling creates
  one.
- **Branching**: playing an off-line move at any node appends a new
  child — never overwrites. Loaded games open as the main line.
  Branch events mirror the CLI's.
- **Branch badges**: at a branching node, each child move renders as a
  dashed ghost stone with a numbered badge on its intersection. Click a
  badge or press 1–9 to follow that child. First child = main line.
- **Variation panel**: collapsible right-side panel with the indented
  variation tree; current line highlighted; click-to-jump; context
  actions rename / delete / promote-to-main. Panel visibility persists
  in session settings.
- Branches on external records are **session-local**; "Save as…" writes
  a new GUI record whose versioned format gains an optional variations
  section (backward compatible: old readers see the main line).

## 6. AI labeling

- **Label dialog**: evaluator kind (baseline; candidate nets later via
  the lab's candidate registry), MCTS parameters (simulations,
  temperature, Dirichlet on/off), worker count (default: cores − 2,
  i.e. 12 on the owner's 14-CPU machine), scope (this game / selected
  records / whole corpus), output corpus name.
- **Execution**: in-process worker pool reusing the AI-worker pattern
  [decision: not an external process]. A shared queue of (record, ply)
  units; each worker owns its MCTS instance and evaluator. MCTS stays
  single-threaded per search (locked: no tokio); parallelism is across
  workers.
- **Output**: mentor game-record format — per ply: played move, visit
  distribution π, root value, priors; header: evaluator identity,
  parameters, versions, provenance; game outcome appended. Identical
  format to future mentor-datagen output, so training reads one format.
- **Incremental and crash-safe**: results are written per completed
  game; a job file tracks done/pending/skipped-with-reason; relaunching
  a job resumes where it stopped.
- **Control room**: progress bar, positions/s, ETA, pause/resume/cancel,
  skip log. "Watch live" opens the game currently being labeled on the
  board with search overlays.
- Output lands in the `labeled_records` config root.
- **GPU**: deferred. Meaningful GPU labeling needs a batched evaluator
  service, which the mentor-datagen spec explicitly defers ("worker
  actors and evaluator service arrive later") and ch. 12's phased GPU
  scheduling locks. GPU-batched evaluation is milestone M6, a stretch
  goal that may be dropped.

## 7. The `.gomoku` config directory

- Startup flag `--config <dir>`; default `./.gomoku/`. Contains
  `config.json` (schema version 1):

```json
{
  "version": 1,
  "roots": {
    "game_records": "../data/gomocup-games",
    "samples": "../data",
    "models": "../artifacts/gomoku/runs",
    "labeled_records": "../data/labeled"
  },
  "labeling": { "workers": 12 }
}
```

- Paths resolve relative to the config file's location; the directory
  is portable.
- Missing file → created with defaults on first run. Unknown keys are
  preserved (forward compatible). Writes are atomic (temp + rename).
  Corruption → backup aside + defaults + a dialog; never silent loss.
- **Config panel**: one row per root (name, path, native folder picker,
  exists/valid indicator), a labeling-defaults form, and a raw-JSON
  tab. Changes save immediately and take effect without restart.

## 8. Error handling

- Unreadable/corrupt records: listed as such with reason; the corpus
  stays browsable.
- Labeling failures on a single position: recorded as
  skipped-with-reason in the job file; the job continues.
- Illegal moves inside an external game record (it happens in the
  wild): record opens up to the offending ply and is flagged.
- All worker-pool failures surface in the control room log, not in
  dialog spam.

## 9. Testing strategy

- **Headless (core)**: sniff/load per format with fixture files
  (including a minimal hand-written psq); `Variations` decision tests
  mirroring the CLI's semantics; labeling scheduler with a deterministic
  mock evaluator; config round-trip and forward-compat; labeling-job
  crash-resume.
- **GUI**: extend the existing `app_tests` pattern; render layer keeps
  its headless screenshot discipline.
- **Human verification**: every milestone below ships with
  "open the app, do X, you should see Y" instructions that take
  minutes to check.

## 10. Milestones

| # | Milestone | Human verification |
|---|---|---|
| M1 | Format registry + psq reader + Explorer window: open a psq directory, record list, pick a game → end position on the board, rewind/forward | Open `data/gomocup-games/…/Freestyle15_2`, double-click game 5, press ←/→ |
| M2 | Registry folded: VCF puzzles, GUI records, sample-shard dirs through the same dialog; jump-to-index + filters | Open a puzzle JSON, then a shard dir; jump to record #500 |
| M3 | `Variations` + branch badges + variation panel (rename/delete/promote) | At ply 10 play a different move; badge 2 appears; open panel; promote it |
| M4 | `.gomoku` config + config panel | Launch `--config /tmp/lab`, edit a root in the UI, relaunch, it persists |
| M5 | Labeling pool + control room + mentor-format output + resume | Label 3 games (baseline, 100 sims); quit mid-job; relaunch → resumes; inspect one output record |
| M6 | *(stretch, may be dropped)* GPU-batched evaluator service as AiConfig option | — |

Milestones are independently verifiable and in dependency order. The
implementation plan (separate document) breaks each into steps and
keeps the milestone log that records what we learned — including
course amendments, reprioritizations, and dropped goals, per §0.
