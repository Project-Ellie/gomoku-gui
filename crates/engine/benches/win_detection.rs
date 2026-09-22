//! Criterion benchmarks for the engine's hottest primitives.
//! Slice 10. See docs/13-engine-design.md, "Test plan".

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use engine::{Board, Color, Move};

/// Build a board by filling four consecutive rows starting at `row`.
/// Produces ~60 stones with no five-in-a-row (alternating play gives a
/// checkerboard-like pattern inside the band).
fn board_filled_from(row: u8) -> Board {
    let mut b = Board::new();
    for dr in 0..4u8 {
        let r = (row + dr) % 15;
        for c in 0..15u8 {
            b.play(Move::new(r, c).unwrap()).unwrap();
        }
    }
    b
}

fn bench_has_five_any(c: &mut Criterion) {
    // Several distinct mid-game boards (~60 stones each).
    let boards = vec![
        board_filled_from(0),
        board_filled_from(4),
        board_filled_from(8),
        board_filled_from(12),
    ];

    c.bench_function("has_five_any", |b| {
        b.iter(|| {
            let mut acc = 0u64;
            for board in black_box(&boards) {
                acc ^= board.has_five_any(Color::Black) as u64;
                acc ^= board.has_five_any(Color::White) as u64;
            }
            black_box(acc);
        })
    });
}

fn bench_play_undo(c: &mut Criterion) {
    // A quiet 30-ply script: rows 0 and 1, alternating colors.
    let script: Vec<Move> = (0..2u8)
        .flat_map(|r| (0..15u8).map(move |c| Move::new(r, c).unwrap()))
        .collect();

    c.bench_function("play_undo_30ply", |b| {
        b.iter(|| {
            let mut board = Board::new();
            for &mv in &script {
                board.play(mv).unwrap();
            }
            for _ in 0..script.len() {
                board.undo();
            }
            black_box(board);
        })
    });
}

criterion_group!(benches, bench_has_five_any, bench_play_undo);
criterion_main!(benches);
