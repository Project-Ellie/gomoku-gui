//! Legal terminal games shared by public-boundary regression tests.

use gomoku_core::{Game, point};

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
