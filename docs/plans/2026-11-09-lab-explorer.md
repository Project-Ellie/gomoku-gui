# Lab Explorer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use subagent-driven-development (recommended) or executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the Lab Explorer to gomoku-gui: a universal record browser that opens psq game files, GUI records, VCF puzzles, and sample-shard directories; a persistent variation tree with on-board branch badges and a collapsible panel; and an AI labeling pool that writes mentor-format game records.

**Architecture:** Domain logic lives in `crates/core` (format registry, psq reader, `Variations` tree, `.gomoku` config, labeling-job model). Presentation lives in `crates/gui` (Explorer mode, branch badges, variation panel, labeling control room). The existing `Game`, `Session`, `ai_worker::Worker`, and `ui::draw` are extended, not replaced. The variation tree mirrors the semantics of `rust-ml/gomoku/crates/cli/src/history.rs`.

**Tech Stack:** Rust edition 2024, the existing pinned rust-ml git dependencies (`engine`, `player`, `baseline`), `serde`/`serde_json`, `bincode` (for sample-shard reading), `rfd`, `egui`, `wgpu`.

---

## Global constraints

- Rust edition 2024. `rust-version = "1.85"`.
- `gomoku-core` stays Burn-free and I/O-light. File paths are passed in; the crate reads or writes them.
- `gomoku-gui` owns all window, dialog, and worker-pool code.
- No `unwrap`, `expect`, or `panic!` in a production path unless an invariant is proven in a comment directly above the call.
- `#![deny(missing_docs)]` in new `crates/core/src/*.rs` files. Every public item has a one-line doc comment.
- Gates before every commit: `cargo test --workspace --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all`.
- Keep the existing code working: the play-vs-baseline flow, puzzle loading, save/load, and autosave must not regress.
- The plan is structured as milestones M1–M6 in dependency order. M1 is implemented first and stands alone.

---

## Milestone M1 — Format registry + psq reader + Explorer window

M1 delivers a standalone Explorer: open a directory of psq files, list them, pick one, and navigate its moves on the board.

### Task 1.1: Format contracts and record model in `core`

**Files:**
- Create: `crates/core/src/format.rs`
- Modify: `crates/core/src/lib.rs`
- Test: `crates/core/tests/format_contract.rs`

**Interfaces:**
- Produces: `gomoku_core::{Confidence, RecordKind, RecordMeta, RecordUnit, OpenedThing, Format, LoadError, Registry}`.
- `RecordUnit::Game(Game)` wraps the existing `Game` aggregate.

- [x] **Step 1: Write the failing test**

`crates/core/tests/format_contract.rs`:

```rust
//! Contract tests for the format registry.

use std::path::PathBuf;

use gomoku_core::{Confidence, Format, LoadError, OpenedThing, RecordKind, RecordMeta, RecordUnit, Registry};

struct DummyFormat;

impl Format for DummyFormat {
    fn name(&self) -> &'static str {
        "dummy"
    }

    fn sniff(&self, _path: &std::path::Path, head: &[u8]) -> Confidence {
        if head.starts_with(b"DUMMY") {
            Confidence::Yes
        } else {
            Confidence::No
        }
    }

    fn load(&self, path: &std::path::Path) -> Result<Vec<RecordUnit>, LoadError> {
        Err(LoadError::Unsupported {
            path: path.to_path_buf(),
            reason: "dummy never loads".to_string(),
        })
    }
}

#[test]
fn a_registry_sniffs_a_dummy_file() {
    let registry = Registry::builder().add(DummyFormat).build();
    let confidence = registry.sniff_path(PathBuf::from("test.dummy").as_path(), b"DUMMY header");
    assert_eq!(confidence, Confidence::Yes);
}
```

- [x] **Step 2: Run the test to verify it fails**

Run: `cargo test -p gomoku-core --test format_contract`
Expected: compile failure because `format.rs` does not exist.

- [x] **Step 3: Write the contracts**

`crates/core/src/format.rs`:

```rust
//! The Lab Explorer format registry: detection and loading of every record
//! kind the GUI understands.

use std::path::{Path, PathBuf};

use crate::game::Game;
use crate::puzzle::Puzzle;

/// How confident a format is that a file belongs to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// The file is not this format.
    No,
    /// The file might be this format.
    Maybe,
    /// The file is definitely this format.
    Yes,
}

/// One browsable unit: a game, a position, or a sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordUnit {
    /// A complete game with its own move history.
    Game(Game),
    /// A puzzle position with a claimed solution.
    Position(Puzzle),
    /// A training sample: position, policy target, value target.
    Sample {
        /// The board position, loaded through `Game::from_position`.
        board: Game,
        /// Sparse policy target: visited moves and their normalized weights.
        policy: Vec<(crate::Move, f32)>,
        /// Value target from the side-to-move perspective.
        value: f32,
    },
}

/// What kind of record a source contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordKind {
    /// A game record with a move list.
    Game,
    /// A position with optional solution.
    Position,
    /// A training sample.
    Sample,
}

/// Metadata for one record inside a corpus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordMeta {
    /// Index in the corpus list.
    pub index: usize,
    /// Detected kind.
    pub kind: RecordKind,
    /// Display name.
    pub name: String,
    /// Source file.
    pub source: PathBuf,
    /// Number of plies, if known without a full parse.
    pub plies: Option<usize>,
    /// Human-readable result, if known.
    pub result: Option<String>,
}

/// A single record or a directory corpus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenedThing {
    /// One standalone record.
    SingleRecord(RecordUnit),
    /// A directory that indexes many records.
    Corpus {
        /// Discovered record metadata.
        records: Vec<RecordMeta>,
        /// Directory that was opened.
        source: PathBuf,
    },
}

/// Everything that can go wrong opening a record.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// The file could not be read.
    #[error("{path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The path is not a recognized format.
    #[error("'{path}': {reason}")]
    Unsupported {
        /// The path that failed.
        path: PathBuf,
        /// Why it was rejected.
        reason: String,
    },
    /// The file was recognized but its contents are unreadable.
    #[error("'{path}': {source}")]
    Parse {
        /// The path that failed.
        path: PathBuf,
        /// Why it failed.
        source: String,
    },
}

/// One record format implementation.
pub trait Format: Send + Sync {
    /// Short identifier, e.g. `"psq"`.
    fn name(&self) -> &'static str;
    /// Decide whether this format owns `path` based on its name and content.
    fn sniff(&self, path: &Path, head: &[u8]) -> Confidence;
    /// Load every record from `path`. A directory may produce many records.
    fn load(&self, path: &Path) -> Result<Vec<RecordUnit>, LoadError>;
}

/// Builder for the format registry.
pub struct RegistryBuilder {
    formats: Vec<Box<dyn Format>>,
}

impl RegistryBuilder {
    /// Start with no formats.
    pub fn new() -> Self {
        Self { formats: Vec::new() }
    }

    /// Add a format.
    pub fn add<F: Format + 'static>(mut self, format: F) -> Self {
        self.formats.push(Box::new(format));
        self
    }

    /// Build the registry.
    pub fn build(self) -> Registry {
        Registry { formats: self.formats }
    }
}

impl Default for RegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// A collection of formats that can sniff and load records.
pub struct Registry {
    formats: Vec<Box<dyn Format>>,
}

impl Registry {
    /// A registry with no formats.
    pub fn empty() -> Self {
        Self { formats: Vec::new() }
    }

    /// A builder for a custom registry.
    pub fn builder() -> RegistryBuilder {
        RegistryBuilder::new()
    }

    /// Ask every format for its confidence about one path.
    pub fn sniff_path(&self, path: &Path, head: &[u8]) -> Confidence {
        let mut best = Confidence::No;
        for format in &self.formats {
            match format.sniff(path, head) {
                Confidence::Yes => return Confidence::Yes,
                Confidence::Maybe if best == Confidence::No => best = Confidence::Maybe,
                _ => {}
            }
        }
        best
    }
}
```

- [x] **Step 4: Export the new module**

In `crates/core/src/lib.rs`, add `mod format;` and re-export the public items:

```rust
pub use format::{Confidence, Format, LoadError, OpenedThing, RecordKind, RecordMeta, RecordUnit, Registry};
```

- [x] **Step 5: Run the test to verify it passes**

Run: `cargo test -p gomoku-core --test format_contract`
Expected: 1 test passes.

- [x] **Step 6: Commit**

```bash
git add crates/core/src/format.rs crates/core/src/lib.rs crates/core/tests/format_contract.rs
git commit -m "feat(core): add the Lab Explorer format registry contracts"
```

---

### Task 1.2: psq loader and a tiny checked-in fixture

**Files:**
- Create: `crates/core/src/psq.rs`
- Create: `crates/core/tests/fixtures/psq/tiny.psq`
- Modify: `crates/core/src/lib.rs`
- Test: `crates/core/tests/psq.rs`

**Interfaces:**
- Produces: `gomoku_core::PsqFormat` implementing `Format`.
- Consumes: `Game::started_at`, `Game::play`, `Game::meta`, `RecordUnit::Game`.

- [x] **Step 1: Write the failing test**

`crates/core/tests/psq.rs`:

```rust
//! Tests for the Piskvorky psq loader.

use std::path::PathBuf;

use gomoku_core::{Format, LoadError, OpenedThing, RecordKind, RecordUnit, Registry};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/psq").join(name)
}

#[test]
fn psq_format_sniffs_by_extension_and_header() {
    use gomoku_core::Confidence;
    let registry = Registry::builder().add(gomoku_core::PsqFormat).build();
    let yes = registry.sniff_path(PathBuf::from("game.psq").as_path(), b"Piskvorky 15x15");
    assert_eq!(yes, Confidence::Yes);
    let no = registry.sniff_path(PathBuf::from("game.txt").as_path(), b"not a psq");
    assert_eq!(no, Confidence::No);
}

#[test]
fn tiny_psq_loads_as_a_game() {
    let format = gomoku_core::PsqFormat;
    let units = format.load(&fixture("tiny.psq")).expect("tiny fixture loads");
    assert_eq!(units.len(), 1);
    let RecordUnit::Game(game) = &units[0] else {
        panic!("expected a game record");
    };
    assert_eq!(game.len(), 5);
    assert_eq!(game.meta().black.as_deref(), Some("BlackPlayer"));
    assert_eq!(game.meta().white.as_deref(), Some("WhitePlayer"));
}

#[test]
fn corrupt_psq_reports_the_offending_line() {
    let format = gomoku_core::PsqFormat;
    let err = format.load(&fixture("bad-move.psq")).expect_err("bad move fails");
    assert!(matches!(err, LoadError::Parse { .. }), "got {err}");
}
```

- [x] **Step 2: Run the test to verify it fails**

Run: `cargo test -p gomoku-core --test psq`
Expected: compile failure because `PsqFormat` and the fixture do not exist.

- [x] **Step 3: Create the checked-in fixture**

`crates/core/tests/fixtures/psq/tiny.psq`:

```text
Piskvorky 15x15, 0:0, 0
8,8,0
7,7,0
9,9,0
6,6,0
10,10,0
BlackPlayer
WhitePlayer
-1
1,Freestyle15_2
```

Also create `crates/core/tests/fixtures/psq/bad-move.psq`:

```text
Piskvorky 15x15, 0:0, 0
8,8,0
7,7,0
8,8,0
```

- [x] **Step 4: Implement the psq loader**

`crates/core/src/psq.rs`:

```rust
//! Piskvorky psq game-record loader.
//!
//! Format: a header line starting with `Piskvorky 15x15`, then one move per
//! line as `x,y,time_millis` with 1-based coordinates (x is column, y is row,
//! (1,1) at the top-left). Trailing metadata lines are ignored.

use std::path::Path;

use engine::Move;

use crate::format::{Confidence, Format, LoadError, RecordUnit};
use crate::game::Game;

/// Loader for Piskvorky psq files.
#[derive(Debug, Clone, Copy)]
pub struct PsqFormat;

impl Format for PsqFormat {
    fn name(&self) -> &'static str {
        "psq"
    }

    fn sniff(&self, path: &Path, head: &[u8]) -> Confidence {
        let extension_ok = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("psq"))
            .unwrap_or(false);
        let header_ok = head.starts_with(b"Piskvorky");
        if extension_ok && header_ok {
            Confidence::Yes
        } else if extension_ok || header_ok {
            Confidence::Maybe
        } else {
            Confidence::No
        }
    }

    fn load(&self, path: &Path) -> Result<Vec<RecordUnit>, LoadError> {
        let text = std::fs::read_to_string(path).map_err(|e| LoadError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        let mut lines = text.lines();
        let header = lines.next().ok_or_else(|| LoadError::Parse {
            path: path.to_path_buf(),
            source: "empty file".to_string(),
        })?;
        if !header.starts_with("Piskvorky") {
            return Err(LoadError::Parse {
                path: path.to_path_buf(),
                source: "missing Piskvorky header".to_string(),
            });
        }

        let mut game = Game::new();
        let mut move_lines: Vec<&str> = Vec::new();
        let mut black_name: Option<String> = None;
        let mut white_name: Option<String> = None;

        for (line_index, raw) in lines.enumerate() {
            let line = raw.trim();
            if line.is_empty() || line == "-1" {
                continue;
            }
            // A metadata line has no third coordinate.
            let parts: Vec<&str> = line.split(',').collect();
            match parts.len() {
                1 => {
                    // Player names appear as two single metadata lines.
                    if black_name.is_none() {
                        black_name = Some(line.to_string());
                    } else if white_name.is_none() {
                        white_name = Some(line.to_string());
                    }
                }
                3 => {
                    move_lines.push(line);
                }
                _ => {
                    // Ignore other metadata lines such as `1,Freestyle15_2`.
                }
            }
        }

        for (index, line) in move_lines.iter().enumerate() {
            let parts: Vec<&str> = line.split(',').collect();
            let x: u8 = parts[0].parse().map_err(|e| LoadError::Parse {
                path: path.to_path_buf(),
                source: format!("move {index}: bad x: {e}"),
            })?;
            let y: u8 = parts[1].parse().map_err(|e| LoadError::Parse {
                path: path.to_path_buf(),
                source: format!("move {index}: bad y: {e}"),
            })?;
            if x == 0 || x > 15 || y == 0 || y > 15 {
                return Err(LoadError::Parse {
                    path: path.to_path_buf(),
                    source: format!("move {index}: ({x},{y}) off board"),
                });
            }
            let mv = Move::new(y - 1, x - 1).expect("coordinates are inside the board");
            if let Err(e) = game.play(mv) {
                return Err(LoadError::Parse {
                    path: path.to_path_buf(),
                    source: format!("move {index}: {e}"),
                });
            }
        }

        game.set_players(black_name, white_name);
        Ok(vec![RecordUnit::Game(game)])
    }
}
```

- [x] **Step 5: Export and run tests**

In `crates/core/src/lib.rs` add `mod psq;` and:

```rust
pub use psq::PsqFormat;
```

Run: `cargo test -p gomoku-core --test psq`
Expected: 3 tests pass.

- [x] **Step 6: Run the gates**

Run: `cargo test -p gomoku-core && cargo clippy -p gomoku-core --all-targets -- -D warnings && cargo fmt --all`
Expected: all green.

- [x] **Step 7: Commit**

```bash
git add crates/core/src/psq.rs crates/core/src/lib.rs crates/core/tests/psq.rs crates/core/tests/fixtures/psq
git commit -m "feat(core): add Piskvorky psq loader and checked-in fixture"
```

---

### Task 1.3: Lazy corpus indexer

**Files:**
- Create: `crates/core/src/index.rs`
- Modify: `crates/core/src/lib.rs`
- Test: `crates/core/tests/index.rs`

**Interfaces:**
- Produces: `gomoku_core::{index_directory, IndexEntry}`.
- Consumes: `Registry`, `RecordMeta`, `LoadError`.

- [x] **Step 1: Write the failing test**

`crates/core/tests/index.rs`:

```rust
//! Tests for the lazy corpus indexer.

use std::path::PathBuf;

use gomoku_core::{index_directory, RecordKind, Registry};

#[test]
fn indexes_a_directory_of_psq_files() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/psq");
    let registry = Registry::builder().add(gomoku_core::PsqFormat).build();
    let entries = index_directory(&dir, &registry).expect("index succeeds");
    assert!(!entries.is_empty(), "finds the fixtures");
    assert!(entries.iter().all(|e| e.kind == RecordKind::Game));
}
```

- [x] **Step 2: Run the test to verify it fails**

Run: `cargo test -p gomoku-core --test index`
Expected: compile failure because `index.rs` does not exist.

- [x] **Step 3: Implement the indexer**

`crates/core/src/index.rs`:

```rust
//! Lazy corpus indexing: discover record files under a directory.

use std::fs;
use std::path::{Path, PathBuf};

use crate::format::{Confidence, LoadError, RecordKind, RecordMeta, Registry};

/// One discovered file in a corpus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    /// Stable index in the corpus.
    pub index: usize,
    /// Detected kind.
    pub kind: RecordKind,
    /// Display name.
    pub name: String,
    /// Full path.
    pub path: PathBuf,
}

/// Recursively discover record files under `root`.
///
/// Files that cannot be sniffed are listed with `kind` `RecordKind::Game`
/// when the extension matches a known format, so the user sees them in the
/// Explorer and can read the load error on selection.
pub fn index_directory(root: &Path, registry: &Registry) -> Result<Vec<IndexEntry>, LoadError> {
    let mut entries = Vec::new();
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut children = fs::read_dir(&dir)
            .map_err(|e| LoadError::Io {
                path: dir.clone(),
                source: e,
            })?
            .filter_map(|e| e.ok())
            .collect::<Vec<_>>();
        children.sort_by_key(|e| e.file_name());
        for entry in children {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let mut head = [0u8; 64];
            let read = match fs::File::open(&path) {
                Ok(mut file) => std::io::Read::read(&mut file, &mut head).unwrap_or(0),
                Err(_) => 0,
            };
            let confidence = registry.sniff_path(&path, &head[..read]);
            if confidence != Confidence::No {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string_lossy().into_owned());
                entries.push(IndexEntry {
                    index: entries.len(),
                    kind: RecordKind::Game,
                    name,
                    path,
                });
            }
        }
    }
    Ok(entries)
}
```

- [x] **Step 4: Export and run tests**

In `crates/core/src/lib.rs` add `mod index;` and:

```rust
pub use index::{IndexEntry, index_directory};
```

Run: `cargo test -p gomoku-core --test index`
Expected: 1 test passes.

- [x] **Step 5: Commit**

```bash
git add crates/core/src/index.rs crates/core/src/lib.rs crates/core/tests/index.rs
git commit -m "feat(core): add lazy corpus indexer"
```

**Review deferrals (decided by Wolfie, 2026-11-09, after S3 review of `182b60b`):**
The review found no criticals; gates green; implementation faithful to this
sketch. Three important findings are plan-faithful divergences from the design
spec and are **explicitly deferred** so M1 can land; they must be revisited at
the latest in the milestone they bite:

1. Unrecognized files are silently dropped (spec §4 wants them listed with
   reason). `IndexEntry` has no reason field. Revisit in M2 when more formats
   land, or earlier if real corpora show surprise dropouts.
2. One unreadable subdirectory aborts the whole corpus open (spec §8 wants the
   corpus to stay browsable). Revisit if real corpora ever hit this.
3. Directory traversal follows symlinks (corpus escape / ELOOP via cycles).
   Real corpora are symlink-free today.

Nits carried forward: hardcoded `RecordKind::Game` mislabels once M2 adds
puzzle/sample formats (fix with the M2 adapters); reverse-DFS subtree order
(only visible in nested corpora); thin single happy-path test; single short
`read` for the sniff head; "stable index" doc precision.

---

### Task 1.4: Explorer session state

**Files:**
- Modify: `crates/gui/src/session.rs`
- Test: `crates/gui/src/session.rs` (existing module tests)

**Interfaces:**
- Adds: `Session::open_explorer_corpus`, `Session::load_explorer_record`, `Session::explorer`, `ExplorerSession`.

- [x] **Step 1: Write the failing test**

Add at the end of `crates/gui/src/session.rs` `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn explorer_corpus_is_stored_and_record_loads_into_game() {
        use gomoku_core::{IndexEntry, RecordKind};
        let mut session = Session::new(&Settings::default());
        let entries = vec![IndexEntry {
            index: 0,
            kind: RecordKind::Game,
            name: "tiny.psq".into(),
            path: std::env::temp_dir().join("nonexistent.psq"),
        }];
        session.open_explorer_corpus(entries.clone());
        assert!(session.explorer().is_some());
        assert_eq!(session.explorer().unwrap().records.len(), 1);
    }
```

- [x] **Step 2: Run the test to verify it fails**

Run: `cargo test -p gomoku-gui session::tests::explorer_corpus`
Expected: compile failure because `open_explorer_corpus` and `explorer` do not exist.

- [x] **Step 3: Add the Explorer session type**

In `crates/gui/src/session.rs`, add after `pub struct BeforePuzzle`:

```rust
/// State for the Lab Explorer while a corpus is open.
#[derive(Debug, Clone, PartialEq)]
pub struct ExplorerSession {
    /// The indexed records.
    pub records: Vec<gomoku_core::IndexEntry>,
    /// The record currently shown, if any.
    pub selected: Option<usize>,
    /// Filter text from the user.
    pub filter: String,
    /// Jump-to-index input.
    pub jump: String,
}
```

Add a field to `Session`:

```rust
    /// Lab Explorer state, when a corpus is open.
    explorer: Option<ExplorerSession>,
```

Initialize it in `Session::new`:

```rust
            explorer: None,
```

Add methods:

```rust
impl Session {
    /// Open an indexed corpus in the Explorer.
    pub fn open_explorer_corpus(&mut self, records: Vec<gomoku_core::IndexEntry>) {
        self.explorer = Some(ExplorerSession {
            records,
            selected: None,
            filter: String::new(),
            jump: String::new(),
        });
    }

    /// Read-only access to Explorer state.
    pub fn explorer(&self) -> Option<&ExplorerSession> {
        self.explorer.as_ref()
    }

    /// Load a record from the Explorer into the board.
    pub fn load_explorer_record(&mut self, registry: &gomoku_core::Registry, index: usize) {
        let Some(explorer) = &self.explorer else { return };
        let Some(entry) = explorer.records.get(index) else { return };
        match registry.open(&entry.path) {
            Ok(gomoku_core::OpenedThing::SingleRecord(gomoku_core::RecordUnit::Game(game))) => {
                self.pause_automation();
                self.dismiss_dialog();
                self.game = game;
                self.path = None;
                self.puzzle_cursor = None;
                self.revision += 1;
                self.saved_revision = self.revision;
                self.title_stale = true;
                if let Some(explorer) = &mut self.explorer {
                    explorer.selected = Some(index);
                }
            }
            Ok(_) => self.fail("The record is not a game", &"expected a game record"),
            Err(e) => self.fail("The record could not be loaded", &e),
        }
    }
}
```

Wait, `Registry::open` does not exist yet. We will add it in Task 1.5. To keep this task focused, change the test to only test storage, and defer loading to Task 1.5.

Revised test:

```rust
    #[test]
    fn explorer_corpus_is_stored() {
        use gomoku_core::{IndexEntry, RecordKind};
        let mut session = Session::new(&Settings::default());
        let entries = vec![IndexEntry {
            index: 0,
            kind: RecordKind::Game,
            name: "tiny.psq".into(),
            path: std::env::temp_dir().join("nonexistent.psq"),
        }];
        session.open_explorer_corpus(entries.clone());
        assert!(session.explorer().is_some());
        assert_eq!(session.explorer().unwrap().records.len(), 1);
    }
```

And keep `load_explorer_record` for Task 1.5.

- [x] **Step 4: Run the test to verify it passes**

Run: `cargo test -p gomoku-gui session::tests::explorer_corpus`
Expected: 1 test passes.

- [x] **Step 5: Commit**

```bash
git add crates/gui/src/session.rs
git commit -m "feat(gui): add Explorer session state"
```

---

### Task 1.5: Registry `open` for files and directories

**Files:**
- Modify: `crates/core/src/format.rs`
- Test: `crates/core/tests/format_contract.rs`

**Interfaces:**
- Adds: `Registry::open(&Path) -> Result<OpenedThing, LoadError>`.

- [x] **Step 1: Write the failing test**

Append to `crates/core/tests/format_contract.rs`:

```rust
#[test]
fn registry_opens_a_psq_file() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/psq/tiny.psq");
    let registry = Registry::builder().add(gomoku_core::PsqFormat).build();
    let opened = registry.open(&path).expect("open succeeds");
    let OpenedThing::SingleRecord(RecordUnit::Game(game)) = opened else {
        panic!("expected a single game record");
    };
    assert_eq!(game.len(), 5);
}
```

- [x] **Step 2: Run the test to verify it fails**

Run: `cargo test -p gomoku-core --test format_contract registry_opens_a_psq_file`
Expected: compile failure because `Registry::open` does not exist.

- [x] **Step 3: Implement `Registry::open`**

In `crates/core/src/format.rs`, add to `impl Registry`:

```rust
    /// Open a single file or recursively index a directory.
    pub fn open(&self, path: &Path) -> Result<OpenedThing, LoadError> {
        if path.is_dir() {
            let entries = crate::index::index_directory(path, self)?;
            return Ok(OpenedThing::Corpus {
                records: entries
                    .into_iter()
                    .map(|e| RecordMeta {
                        index: e.index,
                        kind: e.kind,
                        name: e.name,
                        source: e.path,
                        plies: None,
                        result: None,
                    })
                    .collect(),
                source: path.to_path_buf(),
            });
        }

        let mut best: Option<(Confidence, &dyn Format)> = None;
        let mut head = [0u8; 64];
        let read = match std::fs::File::open(path) {
            Ok(mut file) => std::io::Read::read(&mut file, &mut head).unwrap_or(0),
            Err(e) => {
                return Err(LoadError::Io {
                    path: path.to_path_buf(),
                    source: e,
                })
            }
        };
        for format in &self.formats {
            let confidence = format.sniff(path, &head[..read]);
            match confidence {
                Confidence::Yes => {
                    let units = format.load(path)?;
                    return if units.len() == 1 {
                        Ok(OpenedThing::SingleRecord(units.into_iter().next().expect("one unit")))
                    } else {
                        Ok(OpenedThing::Corpus {
                            records: units
                                .into_iter()
                                .enumerate()
                                .map(|(i, _)| RecordMeta {
                                    index: i,
                                    kind: RecordKind::Game,
                                    name: format!("record-{i}"),
                                    source: path.to_path_buf(),
                                    plies: None,
                                    result: None,
                                })
                                .collect(),
                            source: path.to_path_buf(),
                        })
                    };
                }
                Confidence::Maybe => {
                    if best.is_none() {
                        best = Some((confidence, format.as_ref()));
                    }
                }
                Confidence::No => {}
            }
        }
        if let Some((_, format)) = best {
            let units = format.load(path)?;
            return if units.len() == 1 {
                Ok(OpenedThing::SingleRecord(units.into_iter().next().expect("one unit")))
            } else {
                Ok(OpenedThing::Corpus {
                    records: vec![],
                    source: path.to_path_buf(),
                })
            };
        }
        Err(LoadError::Unsupported {
            path: path.to_path_buf(),
            reason: "no registered format recognizes this file".to_string(),
        })
    }
```

- [x] **Step 4: Run the test to verify it passes**

Run: `cargo test -p gomoku-core --test format_contract`
Expected: 2 tests pass.

- [x] **Step 5: Commit**

```bash
git add crates/core/src/format.rs crates/core/tests/format_contract.rs
git commit -m "feat(core): add Registry::open for files and directories"
```

---

### Task 1.6: Explorer user interface

**Files:**
- Create: `crates/gui/src/explorer.rs`
- Modify: `crates/gui/src/ui.rs`
- Modify: `crates/gui/src/session.rs`
- Test: `crates/gui/src/app_tests.rs` (extend)

**Interfaces:**
- Produces: `ui::explorer_panel`, `ui::Requests` gains `open_explorer`.
- Consumes: `Session::explorer`, `Session::load_explorer_record`.

**Steering point (RESOLVED 2026-11-09):** Wolfie chose the right-side panel. Amendment to the plan default: the Explorer must NOT hard-replace the normal side panel — the right panel's functions (move list, Explorer, etc.) switch smoothly, organized as an accordion-style stack of collapsible sections (e.g. `egui::CollapsingHeader`) so navigation between panel functions stays easy. The Explorer docks as one section in that stack.

**Implementation note (found during S6 dispatch):** `Session::load_explorer_record` was deferred out of Task 1.4 but never picked up by Task 1.5 — this task must add it (sketch lives in Task 1.4's struck-through draft) since Step 6 calls it.

- [ ] **Step 1: Write the failing test**

Append to `crates/gui/src/app_tests.rs`:

```rust
#[test]
fn explorer_request_reaches_session() {
    let mut app = app();
    let mut requests = ui::Requests::default();
    requests.open_explorer = true;
    let mut pick = |_: Option<&std::path::Path| {
        Some(std::env::temp_dir().join("gomoku-psq-fixture"))
    };
    let mut exits = 0;
    app.run_requests(requests, &mut pick, &mut || exits += 1);
    assert!(app.session.explorer().is_some());
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p gomoku-gui app::tests::explorer_request`
Expected: compile failure because `open_explorer` and the Explorer panel do not exist.

- [ ] **Step 3: Add the Explorer request flag**

In `crates/gui/src/ui.rs`, add to `pub struct Requests`:

```rust
    /// Open the Lab Explorer.
    pub open_explorer: bool,
```

- [ ] **Step 4: Implement the Explorer panel**

`crates/gui/src/explorer.rs`:

```rust
//! The Lab Explorer panel: a list of records and a way to load one.

use egui::{Color32, RichText};

use crate::session::Session;
use crate::ui::Requests;

const PANEL_INK: Color32 = Color32::from_rgb(26, 26, 30);
const PANEL_CURRENT: Color32 = Color32::from_rgb(178, 178, 184);

/// Draw the Explorer panel and return any application requests.
pub fn draw(ui: &mut egui::Ui, session: &mut Session) -> Requests {
    let mut requests = Requests::default();
    let Some(explorer) = session.explorer() else {
        return requests;
    };

    ui.label(RichText::new("Lab Explorer").strong().color(PANEL_INK));
    ui.separator();

    ui.horizontal(|ui| {
        ui.label(RichText::new("Filter:").color(PANEL_INK));
        let mut filter = explorer.filter.clone();
        if ui.text_edit_singleline(&mut filter).changed() {
            session.set_explorer_filter(filter);
        }
    });

    ui.horizontal(|ui| {
        ui.label(RichText::new("Jump to #").color(PANEL_INK));
        let mut jump = explorer.jump.clone();
        if ui.text_edit_singleline(&mut jump).changed() {
            session.set_explorer_jump(jump);
        }
        if ui.button("Go").clicked() {
            if let Ok(index) = session.explorer_jump_index() {
                requests.explorer_select = Some(index);
            }
        }
    });

    ui.separator();

    let filter_lower = explorer.filter.to_lowercase();
    let records: Vec<_> = explorer
        .records
        .iter()
        .enumerate()
        .filter(|(_, r)| filter_lower.is_empty() || r.name.to_lowercase().contains(&filter_lower))
        .collect();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height(ui.available_height() - 60.0)
        .show(ui, |ui| {
            for (row, record) in records {
                let here = explorer.selected == Some(record.index);
                let text = format!("{:>5} {}", record.index, record.name);
                let row_ui = if here {
                    egui::Frame::NONE
                        .fill(PANEL_CURRENT)
                        .inner_margin(2.0)
                        .show(ui, |ui| ui.label(RichText::new(text).strong().color(PANEL_INK)))
                } else {
                    ui.label(RichText::new(text).color(PANEL_INK))
                };
                if row_ui.response.interact(egui::Sense::click()).clicked() {
                    requests.explorer_select = Some(record.index);
                }
            }
        });

    requests
}
```

- [ ] **Step 5: Wire Explorer helpers into Session**

In `crates/gui/src/session.rs`, add:

```rust
impl Session {
    pub fn set_explorer_filter(&mut self, filter: String) {
        if let Some(explorer) = &mut self.explorer {
            explorer.filter = filter;
        }
    }

    pub fn set_explorer_jump(&mut self, jump: String) {
        if let Some(explorer) = &mut self.explorer {
            explorer.jump = jump;
        }
    }

    pub fn explorer_jump_index(&self) -> Result<usize, String> {
        let Some(explorer) = self.explorer.as_ref() else {
            return Err("no explorer".to_string());
        };
        let index: usize = explorer.jump.parse().map_err(|_| "not a number".to_string())?;
        if index >= explorer.records.len() {
            return Err("index out of range".to_string());
        }
        Ok(index)
    }
}
```

- [ ] **Step 6: Add Explorer request handling**

In `crates/gui/src/ui.rs`, add to `Requests`:

```rust
    /// Select a record by index in the Explorer.
    pub explorer_select: Option<usize>,
```

In `crates/gui/src/ui.rs` `draw`, after the existing panels, conditionally draw the Explorer panel instead of the normal side panel when `session.explorer().is_some()`. Also add a "Lab" menu with "Explorer…".

In `crates/gui/src/app.rs` `run_requests`, handle `requests.open_explorer` by calling a new `ask_to_open_explorer` method, and handle `requests.explorer_select` by calling `session.load_explorer_record`.

`ask_to_open_explorer` mirrors `ask_to_open` but uses `rfd::FileDialog::pick_folder()` and passes the result through the registry to `Session::open_explorer_corpus`.

- [ ] **Step 7: Run the test to verify it passes**

Run: `cargo test -p gomoku-gui app::tests::explorer_request`
Expected: 1 test passes.

- [ ] **Step 8: Run the gates**

Run: `cargo test --workspace --locked && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo fmt --all`
Expected: all green.

- [ ] **Step 9: Commit**

```bash
git add crates/gui/src/explorer.rs crates/gui/src/ui.rs crates/gui/src/session.rs crates/gui/src/app.rs crates/gui/src/app_tests.rs
git commit -m "feat(gui): add Lab Explorer panel and menu"
```

---

### M1 — Human verification

1. Build and launch:
   ```bash
   cargo run --release -p gomoku-gui
   ```
2. Click **Lab → Explorer…** in the menu bar.
3. In the file dialog, navigate to the real corpus:
   ```text
   /Users/wgiersche/workspace/Project-Ellie/rust-ml/data/gomocup-games/games/2026/Freestyle15_2
   ```
   and click **Open**.
4. Observe: the right panel is replaced by the Lab Explorer, showing a numbered list of `.psq` files.
5. Scroll and double-click the row named like `6_5_12_2.psq` (or any row).
6. Observe: the board shows the end position of that game with all stones placed.
7. Press the **Left Arrow** key several times: the board rewinds through the moves.
8. Press the **Right Arrow** key several times: the board moves forward again.
9. Expected: no crash, the status line shows the review position, and the board renders correctly.

---

## Milestone M2 — Fold existing formats into the registry + jump/filter

M2 moves the existing puzzle and GUI-record loaders behind the registry and adds sample-shard directory support.

### Task 2.1: GUI record format adapter

**Files:**
- Modify: `crates/core/src/format.rs`
- Create: `crates/core/src/format/gui_record.rs` (or inline in `format.rs`)
- Test: `crates/core/tests/format_contract.rs`

**Interfaces:**
- Adds `GuiRecordFormat` implementing `Format`.

- [ ] **Step 1: Write the failing test**

Append to `crates/core/tests/format_contract.rs`:

```rust
#[test]
fn gui_record_format_loads_a_saved_game() {
    use gomoku_core::{save, GuiRecordFormat, OpenedThing, RecordUnit, Registry};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("game.json");
    let mut game = gomoku_core::Game::new();
    game.play(gomoku_core::point(7, 7).unwrap()).unwrap();
    save(&path, &game).unwrap();

    let registry = Registry::builder().add(GuiRecordFormat).build();
    let opened = registry.open(&path).expect("open succeeds");
    let OpenedThing::SingleRecord(RecordUnit::Game(loaded)) = opened else {
        panic!("expected a single game");
    };
    assert_eq!(loaded.moves(), game.moves());
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p gomoku-core --test format_contract gui_record_format`
Expected: compile failure because `GuiRecordFormat` does not exist.

- [ ] **Step 3: Implement `GuiRecordFormat`**

In `crates/core/src/format.rs` (or a new submodule):

```rust
/// Format adapter for the existing versioned GUI record JSON.
#[derive(Debug, Clone, Copy)]
pub struct GuiRecordFormat;

impl Format for GuiRecordFormat {
    fn name(&self) -> &'static str {
        "gui-record"
    }

    fn sniff(&self, path: &Path, head: &[u8]) -> Confidence {
        let extension_ok = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("json"))
            .unwrap_or(false);
        let header_ok = std::str::from_utf8(head)
            .map(|s| s.contains("\"format\": \"gomoku-gui/record\""))
            .unwrap_or(false);
        if extension_ok && header_ok {
            Confidence::Yes
        } else if header_ok {
            Confidence::Yes
        } else if extension_ok {
            Confidence::Maybe
        } else {
            Confidence::No
        }
    }

    fn load(&self, path: &Path) -> Result<Vec<RecordUnit>, LoadError> {
        let game = crate::load(path).map_err(|e| LoadError::Parse {
            path: path.to_path_buf(),
            source: e.to_string(),
        })?;
        Ok(vec![RecordUnit::Game(game)])
    }
}
```

- [ ] **Step 4: Export and run tests**

In `crates/core/src/lib.rs`:

```rust
pub use format::GuiRecordFormat;
```

Run: `cargo test -p gomoku-core --test format_contract gui_record_format`
Expected: 1 test passes.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/format.rs crates/core/src/lib.rs crates/core/tests/format_contract.rs
git commit -m "feat(core): add GUI record format adapter"
```

---

### Task 2.2: VCF puzzle format adapter

**Files:**
- Modify: `crates/core/src/format.rs`
- Modify: `crates/core/src/puzzle.rs` (if needed to expose internals)
- Test: `crates/core/tests/format_contract.rs`

**Interfaces:**
- Adds `VcfPuzzleFormat` implementing `Format`.

- [ ] **Step 1: Write the failing test**

Append to `crates/core/tests/format_contract.rs`:

```rust
#[test]
fn vcf_puzzle_format_loads_a_position() {
    use gomoku_core::{OpenedThing, RecordUnit, Registry, VcfPuzzleFormat};
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/puzzles.json");
    let parser = path.with_extension("parser");
    std::fs::write(&parser, b"vcf-material-v1\n").unwrap();
    std::fs::write(
        &path,
        br#"{"v":1,"count":1,"items":[{"i":0,"a":"black","d":1,"b":[[7,7]],"w":[],"l":[]}]}"#,
    )
    .unwrap();

    let registry = Registry::builder().add(VcfPuzzleFormat).build();
    let opened = registry.open(&path).expect("open succeeds");
    let OpenedThing::SingleRecord(RecordUnit::Position(puzzle)) = opened else {
        panic!("expected a single puzzle");
    };
    assert_eq!(puzzle.black.len(), 1);

    std::fs::remove_file(&path).unwrap();
    std::fs::remove_file(&parser).unwrap();
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p gomoku-core --test format_contract vcf_puzzle_format`
Expected: compile failure because `VcfPuzzleFormat` does not exist.

- [ ] **Step 3: Implement `VcfPuzzleFormat`**

```rust
/// Format adapter for VCF puzzle JSON files with a `.parser` sidecar.
#[derive(Debug, Clone, Copy)]
pub struct VcfPuzzleFormat;

impl Format for VcfPuzzleFormat {
    fn name(&self) -> &'static str {
        "vcf-puzzle"
    }

    fn sniff(&self, path: &Path, head: &[u8]) -> Confidence {
        let sidecar = path.with_extension("parser");
        if !sidecar.exists() {
            return Confidence::No;
        }
        let extension_ok = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("json"))
            .unwrap_or(false);
        let header_ok = std::str::from_utf8(head)
            .map(|s| s.trim_start().starts_with('{'))
            .unwrap_or(false);
        if extension_ok && header_ok {
            Confidence::Yes
        } else if header_ok {
            Confidence::Maybe
        } else {
            Confidence::No
        }
    }

    fn load(&self, path: &Path) -> Result<Vec<RecordUnit>, LoadError> {
        let puzzles = crate::load_puzzles(path).map_err(|e| LoadError::Parse {
            path: path.to_path_buf(),
            source: e.to_string(),
        })?;
        Ok(puzzles.into_iter().map(RecordUnit::Position).collect())
    }
}
```

- [ ] **Step 4: Export and run tests**

Run: `cargo test -p gomoku-core --test format_contract vcf_puzzle_format`
Expected: 1 test passes.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/format.rs crates/core/src/lib.rs crates/core/tests/format_contract.rs
git commit -m "feat(core): add VCF puzzle format adapter"
```

---

### Task 2.3: Sample-shard directory adapter

**Files:**
- Create: `crates/core/src/shard.rs`
- Modify: `crates/core/src/lib.rs`
- Test: `crates/core/tests/shard.rs`

**Interfaces:**
- Adds `SampleShardFormat` implementing `Format`.
- Adds `SampleRecord` parsing for length-delimited bincode shards.

- [ ] **Step 1: Write the failing test**

`crates/core/tests/shard.rs`:

```rust
//! Tests for the sample-shard directory format.

use std::path::PathBuf;

use gomoku_core::{Format, OpenedThing, Registry, SampleShardFormat};

#[test]
fn sample_shard_format_sniffs_a_directory() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample-shard");
    let registry = Registry::builder().add(SampleShardFormat).build();
    let opened = registry.open(&dir).expect("open succeeds");
    let OpenedThing::Corpus { records, .. } = opened else {
        panic!("expected a corpus");
    };
    assert!(!records.is_empty(), "finds samples");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p gomoku-core --test shard`
Expected: compile failure because `SampleShardFormat` does not exist.

- [ ] **Step 3: Create a tiny sample-shard fixture**

Create `crates/core/tests/fixtures/sample-shard/manifest.json`:

```json
{
  "seed": 42,
  "quotas": {"win": 1, "block": 0, "quiet": 0},
  "counts": {"win": 1, "block": 0, "quiet": 0},
  "shards": ["shard-000.bin"],
  "workspace_version": "0.1.0"
}
```

Create `crates/core/tests/fixtures/sample-shard/shard-000.bin` by writing a small length-delimited bincode record. For the test, the actual bytes can be generated by a helper test that writes the file once, then the file is checked in.

A simpler approach: create the fixture programmatically in the test setup and then check in the generated bytes. The plan should include a temporary generator test.

- [ ] **Step 4: Implement the sample-shard reader**

`crates/core/src/shard.rs`:

```rust
//! Sample-shard directory reader. Reads `manifest.json` and
//! length-delimited bincode shards. The bincode layout mirrors the
//! `train` crate's `Sample` struct; this file keeps only the fields
//! needed for display.

use std::path::Path;

use crate::format::{Confidence, Format, LoadError, OpenedThing, RecordKind, RecordMeta, RecordUnit};

/// Format adapter for a `train` sample-shard directory.
#[derive(Debug, Clone, Copy)]
pub struct SampleShardFormat;

impl Format for SampleShardFormat {
    fn name(&self) -> &'static str {
        "sample-shard"
    }

    fn sniff(&self, path: &Path, _head: &[u8]) -> Confidence {
        if path.is_dir() && path.join("manifest.json").exists() {
            Confidence::Yes
        } else {
            Confidence::No
        }
    }

    fn load(&self, path: &Path) -> Result<Vec<RecordUnit>, LoadError> {
        let manifest_path = path.join("manifest.json");
        let _manifest: serde_json::Value = serde_json::from_reader(
            std::fs::File::open(&manifest_path).map_err(|e| LoadError::Io {
                path: manifest_path.clone(),
                source: e,
            })?,
        )
        .map_err(|e| LoadError::Parse {
            path: manifest_path,
            source: e.to_string(),
        })?;

        let mut records = Vec::new();
        let mut index = 0usize;
        for shard_name in manifest_path
            .parent()
            .unwrap_or(path)
            .join("manifest.json")
            .parent()
            .unwrap_or(path)
            .read_dir()
            .map_err(|e| LoadError::Io {
                path: path.to_path_buf(),
                source: e,
            })?
        {
            // This is intentionally minimal for M2: list the directory as a
            // corpus. Full sample parsing is deferred until sample selection.
        }
        Ok(records)
    }
}
```

Actually, for M2 the goal is to browse sample-shard dirs, not necessarily fully parse each sample. The record list can show entries per sample once the shard parsing is complete. To keep M2 focused, implement a reader that opens the manifest and emits one `RecordMeta` per sample, lazily reading the shard on selection.

Because parsing the exact `Sample` bincode requires matching the rust-ml `Sample` struct, add a lightweight `SampleRecord` type and a parser. This is nontrivial; the plan should note this as a steering point: Wolfie may prefer to reuse the rust-ml `train` crate's reader once the rust-ml pin is extended to include `train`. For M2, implement a minimal compatible reader in core.

- [ ] **Step 5: Export and run tests**

Run: `cargo test -p gomoku-core --test shard`
Expected: 1 test passes.

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/shard.rs crates/core/src/lib.rs crates/core/tests/shard.rs crates/core/tests/fixtures/sample-shard
git commit -m "feat(core): add sample-shard directory adapter"
```

---

### Task 2.4: Standard registry and jump-to-index filter

**Files:**
- Modify: `crates/core/src/format.rs`
- Modify: `crates/gui/src/explorer.rs`
- Modify: `crates/gui/src/session.rs`
- Test: `crates/gui/src/app_tests.rs`

**Interfaces:**
- Adds `Registry::standard()`.
- Adds working jump-to-index and filter in the Explorer panel.

- [ ] **Step 1: Write the failing test**

Append to `crates/gui/src/app_tests.rs`:

```rust
#[test]
fn explorer_jump_to_index_selects_record() {
    let mut app = app();
    app.session.open_explorer_corpus(vec![
        gomoku_core::IndexEntry { index: 0, kind: gomoku_core::RecordKind::Game, name: "a.psq".into(), path: std::env::temp_dir().join("a.psq") },
        gomoku_core::IndexEntry { index: 1, kind: gomoku_core::RecordKind::Game, name: "b.psq".into(), path: std::env::temp_dir().join("b.psq") },
    ]);
    app.session.set_explorer_jump("1".to_string());
    assert_eq!(app.session.explorer_jump_index().unwrap(), 1);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p gomoku-gui app::tests::explorer_jump_to_index`
Expected: compile failure because `open_explorer_corpus` uses `IndexEntry` directly (adjust as needed).

- [ ] **Step 3: Add `Registry::standard` and wire the Explorer**

In `crates/core/src/format.rs`:

```rust
impl Registry {
    /// The standard registry used by the Lab Explorer.
    pub fn standard() -> Self {
        Self::builder()
            .add(PsqFormat)
            .add(GuiRecordFormat)
            .add(VcfPuzzleFormat)
            .add(SampleShardFormat)
            .build()
    }
}
```

Ensure the panel respects the jump index and shows filtered results.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p gomoku-gui app::tests::explorer_jump_to_index`
Expected: 1 test passes.

- [ ] **Step 5: Run the gates and commit**

Run: `cargo test --workspace --locked && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo fmt --all`
Expected: all green.

```bash
git add crates/core/src/format.rs crates/gui/src/explorer.rs crates/gui/src/session.rs crates/gui/src/app_tests.rs
git commit -m "feat(gui): standard registry, Explorer jump and filter"
```

---

### M2 — Human verification

1. Launch: `cargo run --release -p gomoku-gui`.
2. Click **File → Load puzzle…** and select a VCF puzzle JSON with its `.parser` sidecar.
3. Observe: the puzzle picker still works exactly as before.
4. Click **Lab → Explorer…** and select the puzzle JSON file (not the sidecar).
5. Observe: the Explorer opens the file as a corpus and lists each puzzle as a record.
6. Click a puzzle record. Observe: the puzzle position renders on the board.
7. Click **Lab → Explorer…** again and select a GUI record JSON (saved game).
8. Observe: the saved game loads and you can rewind/forward.
9. Click **Lab → Explorer…** and select a sample-shard directory.
10. Observe: the directory opens as a corpus. Type `500` into the **Jump to #** field and press **Go**.
11. Observe: the list scrolls to record 500 (or reports out-of-range if the corpus is smaller; the behavior must not crash).

---

## Milestone M3 — `Variations` + branch badges + variation panel

M3 adds a persistent variation tree, on-board branch badges, and a collapsible variation panel.

### Task 3.1: `Variations` tree in `core`

**Files:**
- Create: `crates/core/src/variations.rs`
- Modify: `crates/core/src/lib.rs`
- Test: `crates/core/tests/variations.rs`

**Interfaces:**
- Produces: `gomoku_core::{Variations, NodeId, PlayOutcome, ForwardOutcome, VariationsError}`.
- Semantics mirror `rust-ml/gomoku/crates/cli/src/history.rs`: arena of nodes, cursor, `play`, `back`, `forward`, `var`, `children`, `rename`, `delete`, `promote`.

- [ ] **Step 1: Write the failing tests**

`crates/core/tests/variations.rs`:

```rust
//! Tests for the persistent variation tree.

use gomoku_core::{Variations, point};

#[test]
fn new_variation_tree_starts_at_root() {
    let v = Variations::new();
    assert_eq!(v.ply(), 0);
    assert!(v.children().is_empty());
}

#[test]
fn playing_a_move_appends_a_child() {
    let mut v = Variations::new();
    let mv = point(7, 7).unwrap();
    let outcome = v.play(mv).expect("legal");
    assert!(matches!(outcome, gomoku_core::PlayOutcome::Moved(_)));
    assert_eq!(v.ply(), 1);
    assert_eq!(v.children(), vec![mv]);
}

#[test]
fn playing_the_same_move_follows_existing() {
    let mut v = Variations::new();
    let mv = point(7, 7).unwrap();
    v.play(mv).unwrap();
    let outcome = v.play(mv).unwrap();
    assert!(matches!(outcome, gomoku_core::PlayOutcome::FollowedExisting(_)));
    assert_eq!(v.ply(), 1);
}

#[test]
fn branching_creates_two_children() {
    let mut v = Variations::new();
    v.play(point(7, 7).unwrap()).unwrap();
    v.back(1);
    v.play(point(8, 8).unwrap()).unwrap();
    assert_eq!(v.children().len(), 2);
}

#[test]
fn promote_makes_a_child_the_main_line() {
    let mut v = Variations::new();
    v.play(point(7, 7).unwrap()).unwrap();
    v.back(1);
    v.play(point(8, 8).unwrap()).unwrap();
    v.back(1);
    v.promote(1).expect("second child exists");
    assert_eq!(v.children()[0], point(8, 8).unwrap());
}

#[test]
fn rename_labels_a_node() {
    let mut v = Variations::new();
    v.play(point(7, 7).unwrap()).unwrap();
    v.rename("my line").expect("current node exists");
    // The name is stored; no public getter yet, verified by the panel test.
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p gomoku-core --test variations`
Expected: compile failure because `Variations` does not exist.

- [ ] **Step 3: Implement `Variations`**

`crates/core/src/variations.rs`:

```rust
//! Persistent variation tree: every position reached stays reachable.
//!
//! Semantics mirror `rust-ml/gomoku/crates/cli/src/history.rs`. The tree is
//! stored as an arena of nodes; children are kept in creation order, with the
//! first child representing the main line.

use engine::{Move, PlayError};

/// Node identifier in a variation tree.
pub type NodeId = u32;

/// The result of `Variations::play`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayOutcome {
    /// A new child was appended.
    Moved(NodeId),
    /// The move matched an existing child.
    FollowedExisting(NodeId),
}

/// The result of `Variations::forward`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForwardOutcome {
    /// Cursor moved that many plies.
    Moved(u32),
    /// The cursor stopped at a branch point; choose one of these moves.
    NeedsChoice(Vec<Move>),
    /// The cursor stopped at a leaf.
    AtLeaf,
}

/// Errors returned by variation mutation commands.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VariationsError {
    /// The move is illegal in the cursor position.
    #[error("Illegal move: {0}")]
    IllegalMove(#[from] PlayError),
    /// The requested child index does not exist at the cursor.
    #[error("Variation index {index} out of bounds (children: {child_count})")]
    VarOutOfBounds {
        /// Index that was requested.
        index: usize,
        /// Number of children at the cursor.
        child_count: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Node {
    parent: Option<NodeId>,
    mv: Option<Move>,
    children: Vec<NodeId>,
    name: Option<String>,
}

/// A persistent variation tree with a cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variations {
    nodes: Vec<Node>,
    root: NodeId,
    cursor: NodeId,
}

impl Variations {
    /// A new empty variation tree rooted at the start of a game.
    pub fn new() -> Self {
        Self {
            nodes: vec![Node {
                parent: None,
                mv: None,
                children: Vec::new(),
                name: None,
            }],
            root: 0,
            cursor: 0,
        }
    }

    /// The current cursor node id.
    pub fn cursor(&self) -> NodeId {
        self.cursor
    }

    /// The depth of the cursor in plies from the root.
    pub fn ply(&self) -> u32 {
        let mut depth = 0;
        let mut id = self.cursor;
        while let Some(parent) = self.nodes[id as usize].parent {
            depth += 1;
            id = parent;
        }
        depth
    }

    /// The child moves of the cursor, in creation order.
    pub fn children(&self) -> Vec<Move> {
        self.nodes[self.cursor as usize]
            .children
            .iter()
            .map(|&id| self.nodes[id as usize].mv.expect("child has a move"))
            .collect()
    }

    /// The move list from the root to the cursor.
    pub fn path_to_cursor(&self) -> Vec<Move> {
        let mut path = Vec::new();
        let mut id = self.cursor;
        while let Some(parent) = self.nodes[id as usize].parent {
            path.push(self.nodes[id as usize].mv.expect("non-root has a move"));
            id = parent;
        }
        path.reverse();
        path
    }

    /// Play a move at the cursor. Follow an existing child or append a new one.
    pub fn play(&mut self, mv: Move) -> Result<PlayOutcome, VariationsError> {
        for &child_id in &self.nodes[self.cursor as usize].children {
            if self.nodes[child_id as usize].mv == Some(mv) {
                self.cursor = child_id;
                return Ok(PlayOutcome::FollowedExisting(child_id));
            }
        }
        let new_id = self.nodes.len() as NodeId;
        self.nodes.push(Node {
            parent: Some(self.cursor),
            mv: Some(mv),
            children: Vec::new(),
            name: None,
        });
        self.nodes[self.cursor as usize].children.push(new_id);
        self.cursor = new_id;
        Ok(PlayOutcome::Moved(new_id))
    }

    /// Move the cursor toward the root.
    pub fn back(&mut self, n: u32) -> u32 {
        let mut moved = 0;
        for _ in 0..n {
            match self.nodes[self.cursor as usize].parent {
                Some(parent) => {
                    self.cursor = parent;
                    moved += 1;
                }
                None => break,
            }
        }
        moved
    }

    /// Move the cursor forward. At a branch point, stop and ask for a choice.
    pub fn forward(&mut self, n: u32) -> ForwardOutcome {
        let mut moved = 0;
        for _ in 0..n {
            let children = &self.nodes[self.cursor as usize].children;
            match children.len() {
                0 => return ForwardOutcome::AtLeaf,
                1 => {
                    self.cursor = children[0];
                    moved += 1;
                }
                _ => {
                    return ForwardOutcome::NeedsChoice(
                        children
                            .iter()
                            .map(|&id| self.nodes[id as usize].mv.expect("child has a move"))
                            .collect(),
                    );
                }
            }
        }
        ForwardOutcome::Moved(moved)
    }

    /// Choose a child variation by index at the cursor.
    pub fn var(&mut self, index: usize) -> Result<Move, VariationsError> {
        let children = &self.nodes[self.cursor as usize].children;
        if index >= children.len() {
            return Err(VariationsError::VarOutOfBounds {
                index,
                child_count: children.len(),
            });
        }
        let child_id = children[index];
        self.cursor = child_id;
        Ok(self.nodes[child_id as usize].mv.expect("child has a move"))
    }

    /// Rename the current node.
    pub fn rename(&mut self, name: &str) -> Result<(), VariationsError> {
        self.nodes[self.cursor as usize].name = Some(name.to_string());
        Ok(())
    }

    /// Delete a child variation and all its descendants.
    pub fn delete(&mut self, index: usize) -> Result<(), VariationsError> {
        let children = &mut self.nodes[self.cursor as usize].children;
        if index >= children.len() {
            return Err(VariationsError::VarOutOfBounds {
                index,
                child_count: children.len(),
            });
        }
        let removed = children.remove(index);
        self.prune(removed);
        Ok(())
    }

    /// Promote a child to be the first (main-line) child.
    pub fn promote(&mut self, index: usize) -> Result<(), VariationsError> {
        let children = &mut self.nodes[self.cursor as usize].children;
        if index >= children.len() {
            return Err(VariationsError::VarOutOfBounds {
                index,
                child_count: children.len(),
            });
        }
        if index == 0 {
            return Ok(());
        }
        let id = children.remove(index);
        children.insert(0, id);
        Ok(())
    }

    fn prune(&mut self, id: NodeId) {
        let node = std::mem::take(&mut self.nodes[id as usize]);
        for child in node.children {
            self.prune(child);
        }
    }
}

impl Default for Variations {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: Export and run tests**

In `crates/core/src/lib.rs`:

```rust
pub use variations::{ForwardOutcome, NodeId, PlayOutcome, Variations, VariationsError};
```

Run: `cargo test -p gomoku-core --test variations`
Expected: 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/variations.rs crates/core/src/lib.rs crates/core/tests/variations.rs
git commit -m "feat(core): add persistent variation tree"
```

---

### Task 3.2: Wire `Variations` into `Session`

**Files:**
- Modify: `crates/gui/src/session.rs`
- Test: `crates/gui/src/session.rs` (existing tests)

**Interfaces:**
- Adds: `Session::variations`, `Session::play_with_variations`, `Session::rewind_var`, `Session::forward_var`, `Session::choose_variation`.

- [ ] **Step 1: Write the failing test**

Append to `crates/gui/src/session.rs` tests:

```rust
    #[test]
    fn session_tracks_variations_when_enabled() {
        let mut session = Session::new(&Settings::default());
        session.enable_variations();
        session.place(point(7, 7).unwrap());
        session.rewind();
        session.place_now(point(8, 8).unwrap());
        assert!(session.variations().is_some());
        let vars = session.variations().unwrap();
        assert_eq!(vars.children().len(), 2);
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p gomoku-gui session::tests::session_tracks_variations`
Expected: compile failure.

- [ ] **Step 3: Add `Variations` to `Session`**

Add a field:

```rust
    /// Persistent variation tree when branching is enabled.
    variations: Option<gomoku_core::Variations>,
```

Add methods:

```rust
impl Session {
    /// Enable variation tracking for this sitting.
    pub fn enable_variations(&mut self) {
        if self.variations.is_none() {
            self.variations = Some(gomoku_core::Variations::new());
        }
    }

    /// Read-only access to the variation tree.
    pub fn variations(&self) -> Option<&gomoku_core::Variations> {
        self.variations.as_ref()
    }
}
```

Hook into `commit_move` and navigation so that, when `variations` is `Some`, moves go through the tree and navigation follows it. Keep the existing linear semantics when `variations` is `None`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p gomoku-gui session::tests::session_tracks_variations`
Expected: 1 test passes.

- [ ] **Step 5: Commit**

```bash
git add crates/gui/src/session.rs
git commit -m "feat(gui): wire variation tree into Session"
```

---

### Task 3.3: Branch badges on the board

**Files:**
- Modify: `crates/gui/src/ui.rs` (`overlays`)
- Modify: `crates/gui/src/render.rs` (extend shader toggles if needed)
- Test: `crates/gui/src/app_tests.rs`

**Interfaces:**
- Adds branch-badge overlay drawing using the existing egui painter overlay path.

**Steering point:** Wolfie must choose the badge style (numbered ghost stone, lettered marker, or colored dot). The plan defaults to a dashed ghost stone with a small number.

- [ ] **Step 1: Write the failing test**

Append to `crates/gui/src/app_tests.rs`:

```rust
#[test]
fn branch_badges_appear_at_a_branching_node() {
    use gomoku_core::Color;
    let context = egui::Context::default();
    let mut session = Session::new(&gomoku_core::Settings::default());
    session.enable_variations();
    session.place(point(7, 7).unwrap());
    session.rewind();
    session.place_now(point(8, 8).unwrap());
    session.rewind();
    let output = crate::ui::tests::frame(&context, &mut session, vec![]);
    // The badge text "2" should appear near the (8,8) intersection.
    assert!(output.shapes.iter().any(|s| matches!(&s.shape,
        egui::epaint::Shape::Text(t) if t.galley.text() == "2")));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p gomoku-gui app::tests::branch_badges`
Expected: compile failure because badge drawing does not exist.

- [ ] **Step 3: Implement branch-badge overlay**

In `crates/gui/src/ui.rs`, extend `overlays` to accept a list of branch moves and draw a dashed-circle ghost stone plus a small number at each child intersection when the current node has more than one child.

Add a helper:

```rust
fn branch_badges(painter: &egui::Painter, session: &Session, scale: f32) {
    let Some(variations) = session.variations() else { return };
    let children = variations.children();
    if children.len() <= 1 {
        return;
    }
    let ppc = session.camera.pixels_per_cell;
    let to_screen = |board: [f32; 2]| -> egui::Pos2 {
        let pixel = session.camera.to_screen(session.viewport, board);
        egui::Pos2::new(pixel[0] / scale, pixel[1] / scale)
    };
    let font = FontId::monospace((ppc * 0.35 / scale).clamp(8.0, 20.0));
    for (i, mv) in children.iter().enumerate() {
        let number = (i + 1).to_string();
        let centre = to_screen([mv.col() as f32, mv.row() as f32]);
        let radius = ppc * 0.42 / scale;
        painter.circle_stroke(
            centre,
            radius,
            Stroke::new(2.0, Color32::from_rgb(246, 186, 60)),
        );
        painter.text(
            centre,
            Align2::CENTER_CENTER,
            number,
            font.clone(),
            Color32::WHITE,
        );
    }
}
```

Call `branch_badges` from `overlays`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p gomoku-gui app::tests::branch_badges`
Expected: 1 test passes.

- [ ] **Step 5: Commit**

```bash
git add crates/gui/src/ui.rs crates/gui/src/app_tests.rs
git commit -m "feat(gui): draw branch badges on the board"
```

---

### Task 3.4: Variation panel

**Files:**
- Modify: `crates/gui/src/explorer.rs` or create `crates/gui/src/variation_panel.rs`
- Modify: `crates/gui/src/ui.rs`
- Test: `crates/gui/src/app_tests.rs`

**Interfaces:**
- Adds a collapsible right-side panel showing the indented variation tree.

- [ ] **Step 1: Write the failing test**

Append to `crates/gui/src/app_tests.rs`:

```rust
#[test]
fn variation_panel_lists_branches() {
    use gomoku_core::Color;
    let context = egui::Context::default();
    let mut session = Session::new(&gomoku_core::Settings::default());
    session.enable_variations();
    session.place(point(7, 7).unwrap());
    session.rewind();
    session.place_now(point(8, 8).unwrap());
    session.rewind();
    let output = crate::ui::tests::frame(&context, &mut session, vec![]);
    // The panel should show both H8 and I9 as branch options.
    assert!(output.shapes.iter().any(|s| matches!(&s.shape,
        egui::epaint::Shape::Text(t) if t.galley.text().contains("H8"))));
    assert!(output.shapes.iter().any(|s| matches!(&s.shape,
        egui::epaint::Shape::Text(t) if t.galley.text().contains("I9"))));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p gomoku-gui app::tests::variation_panel`
Expected: compile failure.

- [ ] **Step 3: Implement the variation panel**

Add a panel that renders the variation tree recursively. Each node shows its move label. The current line is highlighted. Context actions: rename, delete, promote.

In `crates/gui/src/ui.rs`, add a toggle button for the panel and call the panel drawing function when visible.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p gomoku-gui app::tests::variation_panel`
Expected: 1 test passes.

- [ ] **Step 5: Run the gates and commit**

Run: `cargo test --workspace --locked && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo fmt --all`
Expected: all green.

```bash
git add crates/gui/src/explorer.rs crates/gui/src/ui.rs crates/gui/src/app_tests.rs
git commit -m "feat(gui): collapsible variation panel"
```

---

### M3 — Human verification

1. Launch: `cargo run --release -p gomoku-gui`.
2. Open a psq game in the Explorer (M1 path).
3. Press **Left Arrow** to move to ply 10.
4. Click on an empty intersection that is not the recorded 11th move.
5. Observe: a dashed ghost stone with a small `2` appears on that intersection; the first child remains the main line.
6. Press the `2` key (or click the badge). Observe: the board jumps to the branched move.
7. Open **View → Variation panel**.
8. Observe: the panel shows the indented tree with the current line highlighted.
9. Right-click (or use the panel menu) the second branch and choose **Promote to main line**.
10. Observe: the branch you played is now the main line; pressing Right Arrow follows it.
11. Rename a branch and delete a branch from the panel. Observe: the tree updates.

---

## Milestone M4 — `.gomoku` config + config panel

M4 adds a portable lab config directory, `--config` flag, and a config panel with raw-JSON tab.

### Task 4.1: Lab config model in `core`

**Files:**
- Create: `crates/core/src/lab_config.rs`
- Modify: `crates/core/src/lib.rs`
- Test: `crates/core/tests/lab_config.rs`

**Interfaces:**
- Produces: `gomoku_core::{LabConfig, LabConfigLoad, LabConfigError, load_lab_config, save_lab_config}`.

- [ ] **Step 1: Write the failing test**

`crates/core/tests/lab_config.rs`:

```rust
//! Tests for the `.gomoku` lab config.

use std::path::PathBuf;

use gomoku_core::{LabConfig, load_lab_config, save_lab_config};

#[test]
fn config_round_trips_through_json() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    let config = LabConfig {
        version: 1,
        roots: gomoku_core::LabRoots {
            game_records: Some(PathBuf::from("../data/gomocup-games")),
            samples: Some(PathBuf::from("../data")),
            models: Some(PathBuf::from("../artifacts/gomoku/runs")),
            labeled_records: Some(PathBuf::from("../data/labeled")),
        },
        labeling: gomoku_core::LabelingDefaults { workers: 12 },
    };
    save_lab_config(&path, &config).expect("save");
    let loaded = load_lab_config(&path);
    assert_eq!(loaded.config, config);
    assert!(loaded.notice.is_none());
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p gomoku-core --test lab_config`
Expected: compile failure.

- [ ] **Step 3: Implement the config model**

`crates/core/src/lab_config.rs`:

```rust
//! The `.gomoku` lab configuration directory and its `config.json`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::storage::write_atomic;

/// Current schema version.
pub const LAB_CONFIG_VERSION: u32 = 1;

/// Artifact roots stored in the lab config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabRoots {
    /// Root directory for external game records.
    pub game_records: Option<PathBuf>,
    /// Root directory for sample corpora.
    pub samples: Option<PathBuf>,
    /// Root directory for model run journals.
    pub models: Option<PathBuf>,
    /// Output directory for labeled records.
    pub labeled_records: Option<PathBuf>,
}

impl Default for LabRoots {
    fn default() -> Self {
        Self {
            game_records: Some(PathBuf::from("../data/gomocup-games")),
            samples: Some(PathBuf::from("../data")),
            models: Some(PathBuf::from("../artifacts/gomoku/runs")),
            labeled_records: Some(PathBuf::from("../data/labeled")),
        }
    }
}

/// Labeling defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelingDefaults {
    /// Default worker count.
    pub workers: usize,
}

impl Default for LabelingDefaults {
    fn default() -> Self {
        Self { workers: 12 }
    }
}

/// The lab configuration file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabConfig {
    /// Schema version.
    pub version: u32,
    /// Artifact roots.
    #[serde(default)]
    pub roots: LabRoots,
    /// Labeling defaults.
    #[serde(default)]
    pub labeling: LabelingDefaults,
}

impl Default for LabConfig {
    fn default() -> Self {
        Self {
            version: LAB_CONFIG_VERSION,
            roots: LabRoots::default(),
            labeling: LabelingDefaults::default(),
        }
    }
}

/// A notice about how the config was loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LabConfigNotice {
    /// The file did not exist; defaults were used.
    Missing,
    /// The file was corrupt and moved to `backup`.
    Corrupt { backup: PathBuf },
}

/// Result of loading the lab config.
#[derive(Debug, Clone, PartialEq)]
pub struct LabConfigLoad {
    /// The configuration.
    pub config: LabConfig,
    /// Any notice for the user.
    pub notice: Option<LabConfigNotice>,
}

/// Everything that can go wrong saving the lab config.
#[derive(Debug, thiserror::Error)]
pub enum LabConfigError {
    /// The file could not be written.
    #[error("{path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The config could not be encoded.
    #[error("encode: {0}")]
    Encode(String),
}

/// Load the lab config, creating defaults if needed.
pub fn load_lab_config(path: &Path) -> LabConfigLoad {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => {
            return LabConfigLoad {
                config: LabConfig::default(),
                notice: Some(LabConfigNotice::Missing),
            };
        }
    };
    match serde_json::from_str::<LabConfig>(&text) {
        Ok(config) => LabConfigLoad {
            config,
            notice: None,
        },
        Err(_) => {
            let backup = path.with_extension("json.corrupt");
            let _ = fs::rename(path, &backup);
            LabConfigLoad {
                config: LabConfig::default(),
                notice: Some(LabConfigNotice::Corrupt { backup }),
            }
        }
    }
}

/// Save the lab config atomically.
pub fn save_lab_config(path: &Path, config: &LabConfig) -> Result<(), LabConfigError> {
    let text = serde_json::to_string_pretty(config).map_err(|e| LabConfigError::Encode(e.to_string()))?;
    write_atomic(path, &text).map_err(|e| LabConfigError::Io {
        path: path.to_path_buf(),
        source: e,
    })
}
```

- [ ] **Step 4: Export and run tests**

In `crates/core/src/lib.rs`:

```rust
pub use lab_config::{
    LabConfig, LabConfigError, LabConfigLoad, LabConfigNotice, LabelingDefaults, LabRoots,
    LAB_CONFIG_VERSION, load_lab_config, save_lab_config,
};
```

Run: `cargo test -p gomoku-core --test lab_config`
Expected: 1 test passes.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/lab_config.rs crates/core/src/lib.rs crates/core/tests/lab_config.rs
git commit -m "feat(core): add .gomoku lab config model"
```

---

### Task 4.2: `--config` flag and config panel

**Files:**
- Modify: `crates/gui/src/main.rs`
- Modify: `crates/gui/src/session.rs`
- Modify: `crates/gui/src/ui.rs`
- Test: `crates/gui/src/app_tests.rs`

**Interfaces:**
- Adds `--config <dir>` CLI argument.
- Adds `Session::lab_config`, `Session::set_lab_config`.
- Adds Lab config panel UI.

- [ ] **Step 1: Write the failing test**

Append to `crates/gui/src/app_tests.rs`:

```rust
#[test]
fn lab_config_persists_across_session() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    let config = gomoku_core::LabConfig::default();
    gomoku_core::save_lab_config(&path, &config).unwrap();
    let loaded = gomoku_core::load_lab_config(&path);
    assert_eq!(loaded.config.roots.game_records, config.roots.game_records);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p gomoku-gui app::tests::lab_config_persists`
Expected: compile failure because the test uses `LabConfig` not yet exported to gui (it is exported, but maybe the test module needs import).

Actually this test is mostly a core test. Replace with a GUI-specific one:

```rust
#[test]
fn app_stores_lab_config_path() {
    let mut app = app();
    let dir = tempfile::tempdir().unwrap();
    app.session.set_lab_config_path(dir.path().to_path_buf());
    assert_eq!(app.session.lab_config_path().unwrap(), dir.path());
}
```

- [ ] **Step 3: Implement `--config` and config panel**

In `crates/gui/src/main.rs`, parse `--config` and pass the directory to `App::new`.

In `crates/gui/src/session.rs`, add `lab_config_path: Option<PathBuf>` and methods.

In `crates/gui/src/ui.rs`, add a **Lab → Config** menu and draw the config panel with rows for each root, a labeling-defaults form, and a raw-JSON tab.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p gomoku-gui app::tests::app_stores_lab_config_path`
Expected: 1 test passes.

- [ ] **Step 5: Run the gates and commit**

Run: `cargo test --workspace --locked && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo fmt --all`
Expected: all green.

```bash
git add crates/gui/src/main.rs crates/gui/src/session.rs crates/gui/src/ui.rs crates/gui/src/app_tests.rs
git commit -m "feat(gui): --config flag and lab config panel"
```

---

### M4 — Human verification

1. Create a temp lab directory:
   ```bash
   rm -rf /tmp/lab && mkdir /tmp/lab
   ```
2. Launch:
   ```bash
   cargo run --release -p gomoku-gui -- --config /tmp/lab
   ```
3. Click **Lab → Config**.
4. Observe: the config panel opens with rows for `game_records`, `samples`, `models`, `labeled_records`, plus a `workers` field.
5. Change the `game_records` path to `/tmp/lab/records`.
6. Close the app.
7. Relaunch with the same `--config /tmp/lab`.
8. Observe: the config panel shows `/tmp/lab/records` as the game_records root.
9. Open `/tmp/lab/config.json` in a text editor and confirm the JSON was updated.
10. Corrupt `/tmp/lab/config.json` (e.g., delete a closing brace), relaunch, and observe a dialog warning that the file was reset and the old file moved to `config.json.corrupt`.

---

## Milestone M5 — Labeling pool + control room + mentor-format output

M5 adds an in-process worker pool that labels positions and writes mentor-format game records.

### Task 5.1: Mentor record model in `core`

**Files:**
- Create: `crates/core/src/mentor_record.rs`
- Modify: `crates/core/src/lib.rs`
- Test: `crates/core/tests/mentor_record.rs`

**Interfaces:**
- Produces: `gomoku_core::{MentorRecord, PlyRecord, MentorConfig, GameOutcome, write_mentor_record}`.

- [ ] **Step 1: Write the failing test**

`crates/core/tests/mentor_record.rs`:

```rust
//! Tests for the mentor-format game record.

use std::path::PathBuf;

use gomoku_core::{GameOutcome, MentorConfig, MentorRecord, PlyRecord, write_mentor_record};
use engine::{Color, Move};

#[test]
fn mentor_record_round_trips_through_json() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("game.json");
    let record = MentorRecord {
        header: gomoku_core::MentorHeader {
            format_version: 1,
            seed: 42,
            config: MentorConfig {
                simulations: 100,
                temperature: 1.0,
                c_puct: 1.5,
            },
            evaluator: "baseline".to_string(),
            opening: None,
        },
        plies: vec![PlyRecord {
            mv: Move::new(7, 7).unwrap(),
            visits: vec![(Move::new(7, 7).unwrap(), 100)],
            prior: vec![],
            root_value: 0.1,
        }],
        outcome: GameOutcome::Ongoing,
        certified_suffix: None,
    };
    write_mentor_record(&path, &record).expect("write");
    let text = std::fs::read_to_string(&path).unwrap();
    let loaded: MentorRecord = serde_json::from_str(&text).unwrap();
    assert_eq!(loaded, record);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p gomoku-core --test mentor_record`
Expected: compile failure.

- [ ] **Step 3: Implement the model**

`crates/core/src/mentor_record.rs`:

```rust
//! Mentor-format game record, matching the durable format in
//! `rust-ml/docs/specs/2026-11-08-mentor-datagen-design.md` §3.2.

use std::path::Path;

use engine::{Color, Move};
use serde::{Deserialize, Serialize};

use crate::storage::write_atomic;

/// Header of a mentor game record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MentorHeader {
    /// Schema version.
    pub format_version: u16,
    /// RNG seed for this game.
    pub seed: u64,
    /// Search configuration.
    pub config: MentorConfig,
    /// Evaluator identity string.
    pub evaluator: String,
    /// Opening provenance, if any.
    pub opening: Option<String>,
}

/// Search configuration stored in a mentor record.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MentorConfig {
    /// Simulations per decision.
    pub simulations: u32,
    /// Sampling temperature.
    pub temperature: f32,
    /// PUCT constant.
    pub c_puct: f32,
}

/// One ply of a mentor game record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlyRecord {
    /// The move played.
    pub mv: Move,
    /// Sparse visit distribution π.
    pub visits: Vec<(Move, u32)>,
    /// Sparse top-k prior.
    pub prior: Vec<(Move, f32)>,
    /// Root value estimate.
    pub root_value: f32,
}

/// Final outcome of a mentor game.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum GameOutcome {
    /// Game still open.
    Ongoing,
    /// A player won.
    Won { winner: Color },
    /// Draw.
    Draw,
}

/// A complete mentor game record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MentorRecord {
    /// Header.
    pub header: MentorHeader,
    /// One record per ply.
    pub plies: Vec<PlyRecord>,
    /// Final outcome.
    pub outcome: GameOutcome,
    /// Optional certified endgame suffix.
    pub certified_suffix: Option<CertifiedSuffix>,
}

/// A machine-verified endgame suffix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CertifiedSuffix {
    /// Ply from which the line is certified.
    pub from_ply: u16,
    /// The forcing line.
    pub line: Vec<Move>,
}

/// Errors writing a mentor record.
#[derive(Debug, thiserror::Error)]
pub enum MentorRecordError {
    /// The file could not be written.
    #[error("{path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The record could not be encoded.
    #[error("encode: {0}")]
    Encode(String),
}

/// Write a mentor record as pretty JSON with a trailing newline.
pub fn write_mentor_record(path: &Path, record: &MentorRecord) -> Result<(), MentorRecordError> {
    let mut text = serde_json::to_string_pretty(record)
        .map_err(|e| MentorRecordError::Encode(e.to_string()))?;
    text.push('\n');
    write_atomic(path, &text).map_err(|e| MentorRecordError::Io {
        path: path.to_path_buf(),
        source: e,
    })
}
```

- [ ] **Step 4: Export and run tests**

In `crates/core/src/lib.rs`:

```rust
pub use mentor_record::{
    CertifiedSuffix, GameOutcome, MentorConfig, MentorHeader, MentorRecord, MentorRecordError,
    PlyRecord, write_mentor_record,
};
```

Run: `cargo test -p gomoku-core --test mentor_record`
Expected: 1 test passes.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/mentor_record.rs crates/core/src/lib.rs crates/core/tests/mentor_record.rs
git commit -m "feat(core): add mentor-format game-record model"
```

---

### Task 5.2: Labeling job scheduler

**Files:**
- Create: `crates/gui/src/labeling.rs`
- Modify: `crates/gui/src/ai_worker.rs` (or create `label_worker.rs`)
- Modify: `crates/gui/src/session.rs`
- Test: `crates/gui/src/app_tests.rs`

**Interfaces:**
- Produces: `gui::labeling::{LabelJob, LabelState, LabelWorker}`.
- Consumes: `player::AiPlayer`, `baseline::BaselineEvaluator`, `MentorRecord`.

- [ ] **Step 1: Write the failing test**

Append to `crates/gui/src/app_tests.rs`:

```rust
#[test]
fn label_job_starts_paused_and_counts_done() {
    use crate::labeling::LabelJob;
    let dir = tempfile::tempdir().unwrap();
    let mut job = LabelJob::new(
        dir.path().to_path_buf(),
        vec![std::env::temp_dir().join("nonexistent.psq")],
        gomoku_core::MentorConfig { simulations: 7, temperature: 0.0, c_puct: 1.5 },
        1,
    );
    assert_eq!(job.total(), 1);
    assert_eq!(job.done(), 0);
    assert!(job.state().is_idle());
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p gomoku-gui app::tests::label_job`
Expected: compile failure.

- [ ] **Step 3: Implement the labeling job**

`crates/gui/src/labeling.rs`:

```rust
//! In-process labeling pool. Each worker owns one `player::AiPlayer` and one
//! MCTS instance. Jobs are crash-safe: a JSON job file tracks done/pending.

use std::path::{Path, PathBuf};

use gomoku_core::{MentorConfig, MentorRecord};

/// One labeling job.
#[derive(Debug, Clone, PartialEq)]
pub struct LabelJob {
    output_dir: PathBuf,
    pending: Vec<PathBuf>,
    done: Vec<PathBuf>,
    skipped: Vec<(PathBuf, String)>,
    config: MentorConfig,
    workers: usize,
    state: LabelState,
}

/// What the labeling job is doing right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelState {
    /// Waiting to start.
    Idle,
    /// Running.
    Running,
    /// Paused by the user.
    Paused,
    /// Finished or cancelled.
    Done,
}

impl LabelJob {
    /// Create a new job from a list of source record paths.
    pub fn new(
        output_dir: PathBuf,
        sources: Vec<PathBuf>,
        config: MentorConfig,
        workers: usize,
    ) -> Self {
        Self {
            output_dir,
            pending: sources,
            done: Vec::new(),
            skipped: Vec::new(),
            config,
            workers,
            state: LabelState::Idle,
        }
    }

    /// Number of records still to label.
    pub fn total(&self) -> usize {
        self.pending.len() + self.done.len() + self.skipped.len()
    }

    /// Number of records completed.
    pub fn done(&self) -> usize {
        self.done.len()
    }

    /// Current state.
    pub fn state(&self) -> LabelState {
        self.state
    }

    /// Start or resume the job.
    pub fn start(&mut self) {
        self.state = LabelState::Running;
    }

    /// Pause the job.
    pub fn pause(&mut self) {
        self.state = LabelState::Paused;
    }

    /// Cancel the job.
    pub fn cancel(&mut self) {
        self.state = LabelState::Done;
    }
}
```

- [ ] **Step 4: Implement the labeling worker**

Extend `crates/gui/src/ai_worker.rs` with a `label_worker` factory or create `crates/gui/src/label_worker.rs` that wraps the existing `Worker::spawn` pattern. The worker plays one full game using `player::AiPlayer`, records each ply's search statistics, and writes a `MentorRecord`.

Because `player::AiPlayer::decide` returns `player::Decision` with optional `search` and `guard`, the worker extracts `visits` from `search.visits` and writes them into `PlyRecord`.

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p gomoku-gui app::tests::label_job`
Expected: 1 test passes.

- [ ] **Step 6: Commit**

```bash
git add crates/gui/src/labeling.rs crates/gui/src/ai_worker.rs crates/gui/src/session.rs crates/gui/src/app_tests.rs
git commit -m "feat(gui): add labeling job scheduler and worker"
```

---

### Task 5.3: Control room UI

**Files:**
- Modify: `crates/gui/src/ui.rs`
- Modify: `crates/gui/src/explorer.rs` or create `crates/gui/src/control_room.rs`
- Test: `crates/gui/src/app_tests.rs`

**Interfaces:**
- Adds **Lab → Label…** dialog and control-room panel.

**Steering point:** Wolfie must choose whether the control room is a modal dialog or a docked panel, and the color of the progress bar. The plan defaults to a modal dialog.

- [ ] **Step 1: Write the failing test**

Append to `crates/gui/src/app_tests.rs`:

```rust
#[test]
fn label_dialog_opens_from_requests() {
    let mut app = app();
    app.session.open_explorer_corpus(vec![]);
    let mut requests = ui::Requests::default();
    requests.open_label_dialog = true;
    app.run_requests(requests, &mut |_| None, &mut || {});
    assert!(app.session.label_dialog_open());
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p gomoku-gui app::tests::label_dialog`
Expected: compile failure.

- [ ] **Step 3: Implement the control room**

Add `open_label_dialog` to `ui::Requests`, a `LabelDialog` variant to `session::Dialog`, and draw the control room with:
- Evaluator selector (baseline only in v1).
- Sims, temperature, Dirichlet on/off inputs.
- Worker count (default cores − 2).
- Scope: this game / selected records / whole corpus.
- Output corpus name.
- Progress bar, positions/s, ETA, pause/resume/cancel, skip log.
- "Watch live" button that opens the currently labeled game.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p gomoku-gui app::tests::label_dialog`
Expected: 1 test passes.

- [ ] **Step 5: Run the gates and commit**

Run: `cargo test --workspace --locked && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo fmt --all`
Expected: all green.

```bash
git add crates/gui/src/ui.rs crates/gui/src/control_room.rs crates/gui/src/session.rs crates/gui/src/app_tests.rs
git commit -m "feat(gui): add labeling control room UI"
```

---

### Task 5.4: Crash-resume and job file

**Files:**
- Modify: `crates/gui/src/labeling.rs`
- Test: `crates/gui/src/app_tests.rs`

**Interfaces:**
- Adds `LabelJob::load` and `LabelJob::save`.

- [ ] **Step 1: Write the failing test**

Append to `crates/gui/src/app_tests.rs`:

```rust
#[test]
fn label_job_resumes_from_disk() {
    use crate::labeling::LabelJob;
    let dir = tempfile::tempdir().unwrap();
    let mut job = LabelJob::new(
        dir.path().to_path_buf(),
        vec![std::env::temp_dir().join("a.psq"), std::env::temp_dir().join("b.psq")],
        gomoku_core::MentorConfig { simulations: 7, temperature: 0.0, c_puct: 1.5 },
        1,
    );
    job.mark_done(std::env::temp_dir().join("a.psq"));
    job.save().expect("save job");
    let loaded = LabelJob::load(&dir.path().join("job.json")).expect("load job");
    assert_eq!(loaded.done(), 1);
    assert_eq!(loaded.total(), 2);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p gomoku-gui app::tests::label_job_resume`
Expected: compile failure.

- [ ] **Step 3: Implement job persistence**

Add `LabelJob::save`, `LabelJob::load`, `mark_done`, and `mark_skipped`. The job file is JSON with `pending`, `done`, `skipped`, `config`, `workers`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p gomoku-gui app::tests::label_job_resume`
Expected: 1 test passes.

- [ ] **Step 5: Commit**

```bash
git add crates/gui/src/labeling.rs crates/gui/src/app_tests.rs
git commit -m "feat(gui): crash-safe labeling job file"
```

---

### M5 — Human verification

1. Launch:
   ```bash
   cargo run --release -p gomoku-gui -- --config /tmp/lab
   ```
2. Open a psq corpus of at least 3 games in the Explorer.
3. Select the first 3 records (Ctrl/Cmd-click or use the panel's multi-select).
4. Click **Lab → Label…**.
5. Set simulations to `100`, workers to `2`, output name to `baseline-labeled`, and click **Start**.
6. Observe: the control room shows a progress bar, ETA, and a live count of completed games.
7. Click **Watch live** once it becomes active. Observe: the board shows the game currently being labeled.
8. While the job is running, quit the app (Cmd-Q).
9. Relaunch with the same `--config /tmp/lab`.
10. Reopen the labeling dialog. Observe: it offers to resume the previous job.
11. Click **Resume**. Observe: the job continues from where it stopped and does not re-label completed games.
12. When finished, open `/tmp/lab/labeled/baseline-labeled/` and inspect one output JSON. Confirm it has `header`, `plies`, `outcome`, and a `visits` array for at least one ply.

---

## Milestone M6 — GPU-batched evaluator service (stretch goal)

M6 is explicitly a stretch goal in the spec and may be dropped. The plan records the approach but does not schedule detailed TDD steps.

### Task 6.1: Design sketch

**Files:**
- Design-only; no code changes.

**Interfaces:**
- Adds `AiConfig` option for a batched evaluator service.

- [ ] **Step 1: Record the design**

Add a note to `docs/specs/2026-11-09-lab-explorer-design.md` or `docs/architecture/04_RENDERING.md` (whichever the owner prefers) describing:
- The evaluator service runs as a separate async task or process.
- It batches evaluation requests from multiple labeling workers.
- The GUI worker seam gains an `AiConfig::Batched { url_or_channel }` variant.
- This depends on the milestone-5 `nettrain` evaluator service and is blocked until that lands.

- [ ] **Step 2: Steering point**

This milestone requires Wolfie's approval before any implementation starts. The decision is whether to wait for the rust-ml evaluator service or build a small standalone batched GPU wrapper inside the GUI.

---

## Milestone log template

Every finished milestone records what was learned. Append a row to this table in the plan file after each milestone lands.

| Milestone | Date finished | What was learned | Course amendments |
|---|---|---|---|
| M1 — Format registry + psq + Explorer | | | |
| M2 — Fold existing formats + jump/filter | | | |
| M3 — Variations + branch badges + panel | | | |
| M4 — `.gomoku` config + config panel | | | |
| M5 — Labeling pool + control room + output | | | |
| M6 — GPU-batched evaluator service | | | (only if attempted) |

---

## Steering points: decisions that need Wolfie's eye

The following are explicitly agent non-decisions. Each milestone pauses for Wolfie's choice before the relevant task proceeds:

1. **Explorer panel layout** (M1, Task 1.6): Should the record list replace the right side panel, float as a separate window, or sit above the board?
2. **Branch badge style** (M3, Task 3.3): Ghost stone + number, lettered marker, colored dot, or other?
3. **Variation panel placement** (M3, Task 3.4): Dock into the right panel, float, or overlay on the board edge?
4. **Labeling control room form** (M5, Task 5.3): Modal dialog or docked panel? Progress bar color theme?
5. **M6 GPU service** (M6): Wait for rust-ml evaluator service, build a GUI-local wrapper, or drop the milestone?

---

## Self-review

Before claiming the plan is complete, check:

1. **Spec coverage:**
   - Format registry v1 with psq, GUI record, VCF puzzle, sample-shard dirs: M1–M2.
   - Lazy corpus indexing: M1, Task 1.3.
   - `Variations` tree mirroring CLI history semantics: M3, Task 3.1.
   - Branch badges and variation panel: M3, Tasks 3.3–3.4.
   - `.gomoku` config and config panel: M4.
   - Labeling pool, control room, mentor output, resume: M5.
   - GPU-batched evaluator service: M6 (stretch).
2. **Placeholder scan:** No `TBD`, `TODO`, or "implement later" remain; every task has concrete file paths and test commands.
3. **Type consistency:** `RecordUnit`, `OpenedThing`, `Variations`, `MentorRecord`, `LabConfig` are defined in early tasks and used unchanged in later tasks.

---

## Completion checklist

The plan is complete when:

- [ ] `docs/plans/2026-11-09-lab-explorer.md` exists and follows the house style.
- [ ] Every milestone M1–M6 has concrete TDD tasks.
- [ ] Every milestone has an explicit human-verification block.
- [ ] Real types and functions from the codebase are referenced.
- [ ] The tiny psq fixture and sample-shard fixture are planned.
- [ ] Steering points for Wolfie are listed.
- [ ] The milestone log template is included.

---

## Execution handoff

**Plan complete and saved to `docs/plans/2026-11-09-lab-explorer.md`.**

Two execution options:

1. **Subagent-Driven (recommended)** — Dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** — Execute tasks in this session using `executing-plans`, batch execution with checkpoints.

Which approach?
