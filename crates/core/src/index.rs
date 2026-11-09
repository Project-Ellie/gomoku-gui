//! Lazy corpus indexing: discover record files under a directory.

use std::fs;
use std::path::{Path, PathBuf};

use crate::format::{Confidence, LoadError, RecordKind, Registry};

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
            .filter_map(Result::ok)
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
