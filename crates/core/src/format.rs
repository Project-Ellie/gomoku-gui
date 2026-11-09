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
// No `Eq`: the `Sample` variant carries `f32` targets, and floats are not
// `Eq` (NaN). Approved deviation from the plan's derive list.
#[derive(Debug, Clone, PartialEq)]
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
// No `Eq`: it contains `RecordUnit`, which carries floats.
// `large_enum_variant` allowed: the Explorer opens one record at a time, so
// the inline `RecordUnit` is never copied in a hot loop, and keeping the
// variant inline preserves the public shape later tasks construct and match.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
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
    // The payload is named `reason`, not `source`: thiserror treats any field
    // named `source` as an implicit `#[source]`, which must implement
    // `std::error::Error`, and `String` does not.
    #[error("'{path}': {reason}")]
    Parse {
        /// The path that failed.
        path: PathBuf,
        /// Why it failed.
        reason: String,
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
        Self {
            formats: Vec::new(),
        }
    }

    /// Add a format.
    pub fn add<F: Format + 'static>(mut self, format: F) -> Self {
        self.formats.push(Box::new(format));
        self
    }

    /// Build the registry.
    pub fn build(self) -> Registry {
        Registry {
            formats: self.formats,
        }
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
        Self {
            formats: Vec::new(),
        }
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
