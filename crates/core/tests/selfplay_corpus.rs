//! Opening rust-ml self-play corpora: probe, eager parse, listing data.

use std::path::PathBuf;

use engine::{Color, Move};
use gomoku_core::{OpenedThing, Registry};
use selfplay::corpus::write_corpus;
use selfplay::game::{Outcome, WinReason};
use selfplay::opening::{Opening, OpeningAnswer, OpeningProvenance};
use selfplay::record::{
    GAME_RECORD_FORMAT_VERSION, GameHeader, GameRecord, GuardSettings, RecordedPly, SeatConfig,
    SeatPairing, TssProbeSettings,
};
use selfplay::schedule::ExplorationSchedule;

fn mv(row: u8, col: u8) -> Move {
    Move::new(row, col).expect("test coordinates on board")
}

fn seat(simulations: u32) -> SeatConfig {
    SeatConfig {
        simulations,
        c_puct: 1.5,
        temperature: 0.0,
        noise_epsilon: 0.0,
        noise_alpha: 0.3,
        tss_probe: TssProbeSettings {
            enabled: true,
            max_nodes: 10_000,
            max_depth: 30,
        },
        guard: GuardSettings {
            enabled: true,
            max_nodes: 100,
            max_depth: 8,
        },
        resign: false,
        candidate_checks: 10,
    }
}

/// Black's horizontal five in row 7, columns 5–9, after a Take ceremony.
pub(crate) fn fixture_record(seed: u64) -> GameRecord {
    let opening = Opening::new(
        [mv(7, 7), mv(8, 8), mv(7, 8)],
        OpeningAnswer::Take(Color::Black),
        OpeningProvenance::Generated,
    );
    let plies: Vec<RecordedPly> = [
        (mv(6, 6), 400u32, 0.31f32, 0.05f32),
        (mv(7, 6), 390, 0.28, -0.04),
        (mv(9, 9), 377, 0.22, 0.08),
        (mv(7, 9), 401, 0.35, -0.11),
        (mv(5, 5), 365, 0.19, 0.15),
        (mv(7, 5), 412, 0.41, -0.02),
    ]
    .into_iter()
    .map(|(mv, visits, prior, value)| RecordedPly {
        mv,
        visits: vec![(mv, visits)],
        prior: vec![(mv, prior)],
        root_value: Some(value),
    })
    .collect();

    GameRecord {
        header: GameHeader {
            format_version: GAME_RECORD_FORMAT_VERSION,
            seed,
            config: SeatPairing {
                black: seat(400),
                white: seat(400),
                exploration: ExplorationSchedule::default(),
            },
            evaluator: "net:first-corpus@0.1.0".to_owned(),
            opening,
        },
        plies,
        outcome: Outcome::Won {
            winner: Color::Black,
            reason: WinReason::FiveOrMore,
        },
        certified_suffix: None,
    }
}

fn fixture_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "gomoku-core-selfplay-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn a_selfplay_corpus_is_probed_opened_and_listed() {
    let dir = fixture_dir("open");
    let record = fixture_record(42);
    write_corpus(std::slice::from_ref(&record), &dir, 100).expect("corpus writes");

    let registry = Registry::builder().add(gomoku_core::PsqFormat).build();
    assert!(registry.looks_like_corpus(&dir), "manifest + shard probe");

    let OpenedThing::SelfplayCorpus {
        manifest,
        records,
        source,
    } = registry.open(&dir).expect("corpus opens")
    else {
        panic!("a manifest-plus-shards directory opens as a self-play corpus");
    };
    assert_eq!(source, dir);
    assert_eq!(records, vec![record]);
    assert_eq!(manifest.game_count, 1);
    assert_eq!(manifest.seeds, vec![42]);
    assert_eq!(manifest.pairing.black.simulations, 400);
    assert_eq!(manifest.pairing.exploration.plies, 10);

    let _ = std::fs::remove_dir_all(&source);
}

#[test]
fn a_plain_directory_is_not_a_selfplay_corpus() {
    let dir = fixture_dir("plain");
    std::fs::create_dir_all(&dir).expect("dir exists");

    let registry = Registry::builder().add(gomoku_core::PsqFormat).build();
    assert!(!registry.looks_like_corpus(&dir));
    let opened = registry.open(&dir).expect("directory opens");
    assert!(matches!(opened, OpenedThing::Corpus { .. }));

    let _ = std::fs::remove_dir_all(&dir);
}
