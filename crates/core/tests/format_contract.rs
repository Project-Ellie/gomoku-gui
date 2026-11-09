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
