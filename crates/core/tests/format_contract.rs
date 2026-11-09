//! Contract tests for the format registry.

use std::path::PathBuf;

use gomoku_core::{Confidence, Format, LoadError, OpenedThing, RecordUnit, Registry};

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

#[test]
fn corpus_probe_recognizes_the_psq_fixture_directory() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/psq");
    let registry = Registry::builder().add(gomoku_core::PsqFormat).build();
    assert!(registry.looks_like_corpus(&path));
}

#[test]
fn corpus_probe_rejects_a_directory_without_record_files() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(directory.path().join("notes.txt"), "not a game record")
        .expect("the text file writes");
    let registry = Registry::builder().add(gomoku_core::PsqFormat).build();
    assert!(!registry.looks_like_corpus(directory.path()));
}

#[test]
fn corpus_probe_finds_a_record_one_subdirectory_down() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let nested = directory.path().join("games");
    std::fs::create_dir(&nested).expect("the nested directory is created");
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/psq/tiny.psq");
    std::fs::copy(fixture, nested.join("tiny.psq")).expect("the psq fixture copies");
    let registry = Registry::builder().add(gomoku_core::PsqFormat).build();
    assert!(registry.looks_like_corpus(directory.path()));
}
