//! The game aggregate: one move list, one board, one review cursor.
//!
//! `moves` is the only truth about the game. `board` is always the position
//! after `moves[..cursor]`, and `cursor <= moves.len()` always holds.
//! Every change goes through the methods of `Game`, so both facts hold by
//! construction.

use engine::{Board, Color, Move, Status};
use time::OffsetDateTime;

use crate::error::GameError;

/// How a game was won.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinMethod {
    /// Five or more stones in a line. Overlines win.
    Five,
}

/// The result of a game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The game is still open.
    Ongoing,
    /// A player has won.
    Won {
        /// The winner.
        winner: Color,
        /// How the game was won.
        method: WinMethod,
    },
    /// The board is full with no line.
    Draw,
}

/// Game data that is not a move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaData {
    /// The name of the Black player, if given.
    pub black: Option<String>,
    /// The name of the White player, if given.
    pub white: Option<String>,
    /// When the game started, in UTC.
    pub created: OffsetDateTime,
    /// The result of the game. `Game` keeps this in step with the board.
    pub outcome: Outcome,
}

/// A game of freestyle Gomoku: the move list, the board at the review
/// cursor, and the metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Game {
    moves: Vec<Move>,
    board: Board,
    cursor: usize,
    meta: MetaData,
}

impl Game {
    /// A new empty game, started now.
    pub fn new() -> Game {
        Game::started_at(OffsetDateTime::now_utc())
    }

    /// A new empty game with a given start time.
    ///
    /// The record loader uses this to keep the original start time.
    pub fn started_at(created: OffsetDateTime) -> Game {
        Game {
            moves: Vec::new(),
            board: Board::new(),
            cursor: 0,
            meta: MetaData {
                black: None,
                white: None,
                created,
                outcome: Outcome::Ongoing,
            },
        }
    }

    /// The metadata of the game.
    pub fn meta(&self) -> &MetaData {
        &self.meta
    }

    /// Set the player names.
    pub fn set_players(&mut self, black: Option<String>, white: Option<String>) {
        self.meta.black = black;
        self.meta.white = white;
    }

    /// The stone at a point, if the point holds one.
    pub fn stone_at(&self, point: Move) -> Option<Color> {
        self.board.stone_at(point)
    }

    /// The stones on the visible board, in play order.
    ///
    /// Freestyle play alternates from Black, so the color follows the
    /// position in the list.
    pub fn stones(&self) -> impl Iterator<Item = (Move, Color)> + '_ {
        self.board.moves().iter().enumerate().map(|(index, &mv)| {
            let color = if index % 2 == 0 {
                Color::Black
            } else {
                Color::White
            };
            (mv, color)
        })
    }

    /// The status of the board at the review cursor.
    pub fn status(&self) -> Status {
        self.board.status()
    }

    /// The result of the game.
    ///
    /// The result describes the game, not the view. A rewound view keeps
    /// the result of the game.
    pub fn outcome(&self) -> Outcome {
        self.meta.outcome
    }

    /// The side to move at the review cursor.
    pub fn to_move(&self) -> Color {
        self.board.to_move()
    }

    /// The moves of the game.
    pub fn moves(&self) -> &[Move] {
        &self.moves
    }

    /// The number of moves in the game.
    pub fn len(&self) -> usize {
        self.moves.len()
    }

    /// True when the game has no move.
    pub fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }

    /// The number of moves applied to the visible board.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// True when the view is behind the latest move.
    pub fn is_rewound(&self) -> bool {
        self.cursor < self.moves.len()
    }

    /// The number of moves that a placement would delete.
    pub fn pending_truncation(&self) -> usize {
        self.moves.len() - self.cursor
    }

    /// The last move of the visible board.
    pub fn last_move(&self) -> Option<Move> {
        self.board.moves().last().copied()
    }

    /// Play a stone at `point`.
    ///
    /// When the view is rewound, the moves after the cursor are deleted as
    /// part of the placement, so a rejected placement changes nothing. Read
    /// `pending_truncation` before you call this, and confirm with the user
    /// when it is not zero.
    ///
    /// # Errors
    /// `GameError::Occupied` if the point holds a stone.
    /// `GameError::GameOver` if the board at the cursor is finished.
    pub fn play(&mut self, point: Move) -> Result<(), GameError> {
        if self.stone_at(point).is_some() {
            return Err(GameError::Occupied);
        }
        if self.status() != Status::Ongoing {
            return Err(GameError::GameOver);
        }
        self.board.play(point).map_err(GameError::from)?;
        self.moves.truncate(self.cursor);
        self.moves.push(point);
        self.cursor += 1;
        self.meta.outcome = outcome_of(self.board.status());
        Ok(())
    }

    /// Delete the last move of the game. The move is gone.
    ///
    /// Returns false when the game has no move.
    pub fn undo(&mut self) -> bool {
        if self.moves.is_empty() {
            return false;
        }
        // Undo acts on the last move of the game, so leave a rewound view.
        self.live();
        self.moves.pop();
        // The board holds exactly `cursor` moves, and that is at least one.
        self.board.undo();
        self.cursor -= 1;
        self.meta.outcome = outcome_of(self.board.status());
        true
    }

    /// Step the view back one move. Returns false at the first move.
    pub fn rewind(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.board.undo();
        self.cursor -= 1;
        true
    }

    /// Step the view forward one move. Returns false at the latest move.
    pub fn forward(&mut self) -> bool {
        if self.cursor == self.moves.len() {
            return false;
        }
        let mv = self.moves[self.cursor];
        // The move was legal when it was played, and the board is that same
        // position, so the engine cannot reject it here.
        self.board
            .play(mv)
            .expect("replaying a recorded move is legal");
        self.cursor += 1;
        true
    }

    /// Move the view to a move number, clamped to the game length.
    ///
    /// Returns false when the view does not move.
    pub fn seek(&mut self, index: usize) -> bool {
        let target = index.min(self.moves.len());
        if target == self.cursor {
            return false;
        }
        while self.cursor < target {
            self.forward();
        }
        while self.cursor > target {
            self.rewind();
        }
        true
    }

    /// Return the view to the latest move.
    pub fn live(&mut self) -> bool {
        self.seek(self.moves.len())
    }
}

impl Default for Game {
    fn default() -> Game {
        Game::new()
    }
}

/// Read the outcome from an engine status.
fn outcome_of(status: Status) -> Outcome {
    match status {
        Status::Ongoing => Outcome::Ongoing,
        Status::Won(winner) => Outcome::Won {
            winner,
            method: WinMethod::Five,
        },
        Status::Draw => Outcome::Draw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn point(row: u8, col: u8) -> Move {
        Move::new(row, col).expect("row and col are inside the board")
    }

    fn game() -> Game {
        Game::started_at(datetime!(2026-09-22 18:04:11 UTC))
    }

    /// Five black stones on row 7 at columns 3 to 7, with white replies.
    fn near_win() -> Game {
        let mut game = game();
        for (black, white) in [(3, 0), (4, 1), (5, 2), (6, 3)] {
            game.play(point(7, black)).expect("empty cell");
            game.play(point(0, white)).expect("empty cell");
        }
        game.play(point(7, 7)).expect("empty cell");
        game
    }

    #[test]
    fn a_new_game_is_empty() {
        let game = game();
        assert_eq!(game.len(), 0);
        assert!(game.is_empty());
        assert_eq!(game.cursor(), 0);
        assert!(!game.is_rewound());
        assert_eq!(game.to_move(), Color::Black);
    }

    #[test]
    fn play_places_a_stone_and_flips_the_side() {
        let mut game = game();
        game.play(point(7, 7)).expect("empty cell");
        assert_eq!(game.stone_at(point(7, 7)), Some(Color::Black));
        assert_eq!(game.to_move(), Color::White);
        assert_eq!(game.len(), 1);
        assert_eq!(game.last_move(), Some(point(7, 7)));
    }

    #[test]
    fn play_rejects_an_occupied_cell_and_changes_nothing() {
        let mut game = game();
        game.play(point(7, 7)).expect("empty cell");
        assert_eq!(game.play(point(7, 7)), Err(GameError::Occupied));
        assert_eq!(game.len(), 1);
        assert_eq!(game.to_move(), Color::White);
    }

    #[test]
    fn play_is_rejected_after_a_win() {
        let mut game = near_win();
        assert_eq!(
            game.outcome(),
            Outcome::Won {
                winner: Color::Black,
                method: WinMethod::Five
            }
        );
        assert_eq!(game.play(point(10, 10)), Err(GameError::GameOver));
        assert_eq!(game.len(), 9);
    }

    #[test]
    fn stones_are_listed_in_play_order_with_alternating_colors() {
        let mut game = game();
        game.play(point(7, 7)).expect("empty cell");
        game.play(point(7, 8)).expect("empty cell");
        let stones: Vec<_> = game.stones().collect();
        assert_eq!(
            stones,
            vec![(point(7, 7), Color::Black), (point(7, 8), Color::White)]
        );
    }

    #[test]
    fn undo_removes_the_last_move() {
        let mut game = game();
        game.play(point(7, 7)).expect("empty cell");
        game.play(point(7, 8)).expect("empty cell");
        assert!(game.undo());
        assert_eq!(game.len(), 1);
        assert_eq!(game.cursor(), 1);
        assert_eq!(game.stone_at(point(7, 8)), None);
        assert_eq!(game.to_move(), Color::White);
    }

    #[test]
    fn undo_on_an_empty_game_does_nothing() {
        let mut game = game();
        assert!(!game.undo());
        assert!(game.is_empty());
    }

    #[test]
    fn undo_reopens_a_won_game() {
        let mut game = near_win();
        assert!(game.undo());
        assert_eq!(game.outcome(), Outcome::Ongoing);
        assert_eq!(game.status(), Status::Ongoing);
        game.play(point(7, 2)).expect("empty cell");
        assert_eq!(game.len(), 9);
    }

    #[test]
    fn the_outcome_describes_the_game_not_the_view() {
        let mut game = near_win();
        assert!(game.rewind());
        assert_eq!(game.status(), Status::Ongoing);
        assert_eq!(
            game.outcome(),
            Outcome::Won {
                winner: Color::Black,
                method: WinMethod::Five
            }
        );
    }

    #[test]
    fn undo_leaves_a_rewound_view_first() {
        let mut game = game();
        game.play(point(7, 7)).expect("empty cell");
        game.play(point(7, 8)).expect("empty cell");
        game.rewind();
        assert!(game.undo());
        assert_eq!(game.len(), 1);
        assert_eq!(game.cursor(), 1);
    }

    #[test]
    fn rewind_and_forward_move_the_view_only() {
        let mut game = game();
        game.play(point(7, 7)).expect("empty cell");
        game.play(point(7, 8)).expect("empty cell");

        assert!(game.rewind());
        assert!(game.is_rewound());
        assert_eq!(game.cursor(), 1);
        assert_eq!(game.len(), 2);
        assert_eq!(game.stone_at(point(7, 8)), None);
        assert_eq!(game.to_move(), Color::White);

        assert!(game.forward());
        assert!(!game.is_rewound());
        assert_eq!(game.stone_at(point(7, 8)), Some(Color::White));
        assert_eq!(game.to_move(), Color::Black);
    }

    #[test]
    fn rewind_and_forward_stop_at_the_ends() {
        let mut game = game();
        assert!(!game.rewind());
        assert!(!game.forward());
        game.play(point(7, 7)).expect("empty cell");
        assert!(!game.forward());
        assert!(game.rewind());
        assert!(!game.rewind());
    }

    #[test]
    fn seek_clamps_to_the_game_length() {
        let mut game = game();
        game.play(point(7, 7)).expect("empty cell");
        game.play(point(7, 8)).expect("empty cell");
        game.seek(0);
        assert!(game.seek(999));
        assert_eq!(game.cursor(), 2);
        assert!(game.seek(0));
        assert_eq!(game.cursor(), 0);
        assert_eq!(game.stone_at(point(7, 7)), None);
        assert!(!game.seek(0));
    }

    #[test]
    fn live_returns_to_the_latest_move() {
        let mut game = game();
        game.play(point(7, 7)).expect("empty cell");
        game.play(point(7, 8)).expect("empty cell");
        game.seek(0);
        assert!(game.live());
        assert_eq!(game.cursor(), 2);
        assert!(!game.live());
    }

    #[test]
    fn pending_truncation_reports_the_moves_after_the_cursor() {
        let mut game = game();
        for col in 0..4 {
            game.play(point(7, col)).expect("empty cell");
        }
        assert_eq!(game.pending_truncation(), 0);
        game.seek(1);
        assert_eq!(game.pending_truncation(), 3);
    }

    #[test]
    fn play_while_rewound_deletes_the_moves_after_the_cursor() {
        let mut game = game();
        game.play(point(7, 7)).expect("empty cell");
        game.play(point(7, 8)).expect("empty cell");
        game.play(point(9, 9)).expect("empty cell");
        game.seek(1);
        game.play(point(3, 3)).expect("empty cell");

        assert_eq!(game.len(), 2);
        assert_eq!(game.cursor(), 2);
        assert!(!game.is_rewound());
        assert_eq!(game.stone_at(point(7, 8)), None);
        assert_eq!(game.stone_at(point(9, 9)), None);
        assert_eq!(game.stone_at(point(3, 3)), Some(Color::White));
    }

    #[test]
    fn meta_keeps_the_player_names_and_the_start_time() {
        let mut game = game();
        assert_eq!(game.meta().created, datetime!(2026-09-22 18:04:11 UTC));
        game.set_players(Some("Wolfie".to_string()), None);
        assert_eq!(game.meta().black.as_deref(), Some("Wolfie"));
        assert_eq!(game.meta().white, None);
    }
}
