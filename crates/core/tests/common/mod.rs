//! Legal terminal games shared by public-boundary regression tests.

use engine::Move;
use gomoku_core::{Color, Game, Refutation, point};

pub fn won_game() -> Game {
    let mut game = Game::new();
    for col in 0..4 {
        game.play(point(7, col).unwrap()).unwrap();
        game.play(point(0, col).unwrap()).unwrap();
    }
    game.play(point(7, 4).unwrap()).unwrap();
    game
}

pub fn drawn_game() -> Game {
    // No five in any direction. The completed board has 113 black stones.
    let mut black = Vec::new();
    let mut white = Vec::new();
    for row in 0..15 {
        for col in 0..15 {
            let stones = if (row + 2 * col) % 4 < 2 {
                &mut black
            } else {
                &mut white
            };
            stones.push(point(row, col).unwrap());
        }
    }
    let mut game = Game::new();
    for index in 0..white.len() {
        game.play(black[index]).unwrap();
        game.play(white[index]).unwrap();
    }
    game.play(*black.last().unwrap()).unwrap();
    game
}

/// A position where the side to move loses by force: the threat-sequence
/// shape with the colors swapped, so White has a verified VCF and it is Black
/// to move. Any far-corner Black candidate is refuted by it.
pub fn lost_game() -> Game {
    let mut game = Game::new();
    for (row, col) in [
        (7, 3),
        (7, 4),
        (0, 0),
        (7, 5),
        (0, 2),
        (7, 6),
        (0, 4),
        (6, 6),
        (0, 6),
        (8, 8),
    ] {
        game.play(point(row, col).unwrap()).unwrap();
    }
    assert_eq!(game.to_move(), Color::Black);
    game
}

/// The verified opponent proof for a candidate move, exactly as a resignation
/// check would produce it.
pub fn refutation(game: &Game, ordinal: u8, candidate: Move) -> Refutation {
    let mut after = game.board_snapshot();
    after.play(candidate).unwrap();
    let budget = engine::SearchBudget {
        max_nodes: 100_000,
        max_depth: 9,
    };
    let proof = engine::prove_shortest_forced_win(&after, after.to_move(), budget)
        .filter(|proof| engine::verify_line(&after, proof))
        .expect("the candidate is refuted by a verified opponent VCF");
    Refutation { ordinal, proof }
}
