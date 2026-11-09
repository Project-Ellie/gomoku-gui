//! Tests for the lazy corpus indexer.

use std::path::PathBuf;

use gomoku_core::{RecordKind, Registry, index_directory};

#[test]
fn indexes_a_directory_of_psq_files() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/psq");
    let registry = Registry::builder().add(gomoku_core::PsqFormat).build();
    let entries = index_directory(&dir, &registry).expect("index succeeds");
    assert!(!entries.is_empty(), "finds the fixtures");
    assert!(entries.iter().all(|e| e.kind == RecordKind::Game));
}
