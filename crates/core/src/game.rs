//! The game aggregate: one move tree, one board, one review cursor.
//!
//! The imported root plus the current branch describe the visible history.
//! `board` is always the root with `moves[..cursor]` applied. Sibling branches
//! remain in the tree when play continues from an earlier position. Every
//! change goes through the methods of `Game`.

use engine::{Board, Color, Move, Status};
use time::OffsetDateTime;

use crate::error::{GameError, PositionError};
use crate::history::{History, HistoryNode};

/// How a game was won.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinMethod {
    /// Five or more stones in a line. Overlines win.
    Five,
    /// The other player resigned at the live position.
    Resignation,
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
    /// The result of the selected branch, including a possible resignation.
    pub outcome: Outcome,
}

/// A game of freestyle Gomoku: the move list, the board at the review
/// cursor, and the metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Game {
    history: History,
    board: Board,
    meta: MetaData,
    /// Stones that were present before any move in `moves`, with their
    /// absolute colors. Empty for a normal game; populated by
    /// [`Game::from_position`].
    initial_stones: Vec<(Move, Color)>,
    /// The side that had the first move in this game. For a normal game this
    /// is always `Black`; for a game built from a puzzle position it is the
    /// side to move in that position.
    initial_to_move: Color,
    /// True when the game was built from an arbitrary position rather than an
    /// empty board. Such games cannot be saved as move-only records.
    from_position: bool,
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
        Game::from_board(Board::new(), created)
    }

    fn from_board(board: Board, created: OffsetDateTime) -> Game {
        Game {
            history: History::new(),
            board,
            meta: MetaData {
                black: None,
                white: None,
                created,
                outcome: Outcome::Ongoing,
            },
            initial_stones: Vec::new(),
            initial_to_move: Color::Black,
            from_position: false,
        }
    }

    /// Build a game from an arbitrary valid position.
    ///
    /// The `to_move` side is the side that plays next. The returned game has
    /// an empty move list and its cursor at the root, so that undo and rewind
    /// stop at the loaded position rather than at an empty board.
    ///
    /// # Errors
    /// [`PositionError`] if either color repeats a cell, the colors overlap,
    /// or the counts disagree with the side to move.
    pub fn from_position(
        black: Vec<Move>,
        white: Vec<Move>,
        to_move: Color,
    ) -> Result<Game, PositionError> {
        for stones in [&black, &white] {
            let mut seen = std::collections::HashSet::new();
            for mv in stones {
                if !seen.insert(mv.index()) {
                    return Err(PositionError::DuplicateStone);
                }
            }
        }
        let board = Board::from_position(&black, &white, to_move)?;
        let mut game = Game::from_board(board, OffsetDateTime::now_utc());
        game.initial_to_move = to_move;
        game.from_position = true;
        game.initial_stones = black
            .into_iter()
            .map(|mv| (mv, Color::Black))
            .chain(white.into_iter().map(|mv| (mv, Color::White)))
            .collect();
        game.meta.outcome = outcome_of(game.board.status());
        Ok(game)
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

    /// The stones on the visible board. Stones that came from a loaded puzzle
    /// position are listed first in the order they were given; moves played
    /// through this `Game` follow in play order.
    pub fn stones(&self) -> impl Iterator<Item = (Move, Color)> + '_ {
        let initial = self.initial_stones.iter().copied();
        let played = self.board.moves().iter().enumerate().map(|(index, &mv)| {
            let color = color_of(index, self.initial_to_move);
            (mv, color)
        });
        initial.chain(played)
    }

    /// Clone the board at the review cursor, including any imported root.
    ///
    /// The detached snapshot shares no mutable state with this game. A worker
    /// can analyze or play on its own copy without changing the game or record.
    pub fn board_snapshot(&self) -> Board {
        self.board.clone()
    }

    /// The status of the board at the review cursor.
    pub fn status(&self) -> Status {
        self.board.status()
    }

    /// The result of the selected branch.
    ///
    /// The result describes the branch tip, not the view. A rewound view keeps
    /// the result of the branch.
    pub fn outcome(&self) -> Outcome {
        self.meta.outcome
    }

    /// True when the game was built from an arbitrary position rather than an
    /// empty board. Such games cannot be round-tripped through the record
    /// format.
    pub fn is_from_position(&self) -> bool {
        self.from_position
    }

    /// The side to move at the review cursor.
    pub fn to_move(&self) -> Color {
        self.board.to_move()
    }

    /// The moves on the selected branch, from its root to its tip.
    pub fn moves(&self) -> &[Move] {
        self.history.moves()
    }

    /// Whether each move on the selected branch has a sibling in the tree.
    ///
    /// The flags are parallel to [`Game::moves`], so the flag at index zero
    /// describes move number one.
    pub fn moves_with_siblings(&self) -> Vec<bool> {
        self.history.moves_with_siblings()
    }

    /// Every stone on the selected branch, in play order, including any move
    /// after the cursor. Use this to list the branch; use `stones` for the board.
    pub fn record(&self) -> impl Iterator<Item = (Move, Color)> + '_ {
        self.moves()
            .iter()
            .enumerate()
            .map(|(index, &point)| (point, color_of(index, self.initial_to_move)))
    }

    /// The number of moves on the selected branch.
    pub fn len(&self) -> usize {
        self.moves().len()
    }

    /// True when the selected branch has no move.
    pub fn is_empty(&self) -> bool {
        self.moves().is_empty()
    }

    /// The number of moves applied to the visible board.
    pub fn cursor(&self) -> usize {
        self.history.cursor()
    }

    /// True when the view is behind the latest move.
    pub fn is_rewound(&self) -> bool {
        self.cursor() < self.len()
    }

    /// Whether more than one child continues from the cursor position.
    pub fn is_branch_point(&self) -> bool {
        self.history.is_branch_point()
    }

    /// The child moves at the cursor, in creation order.
    pub fn branch_options(&self) -> Vec<Move> {
        self.history.branch_options()
    }

    /// The selected child index at the cursor, if it has a selected child.
    pub fn selected_branch(&self) -> Option<usize> {
        self.history.selected_branch()
    }

    /// The last move of the visible board.
    pub fn last_move(&self) -> Option<Move> {
        self.board.moves().last().copied()
    }

    /// True when a stone can be played at an empty point in the visible view.
    ///
    /// A finished live game cannot continue, even if its board is ongoing
    /// after resignation. An ongoing rewound board can start a new branch.
    pub fn can_play(&self) -> bool {
        self.status() == Status::Ongoing
            && (self.is_rewound() || self.outcome() == Outcome::Ongoing)
    }

    /// Resign for the side to move, without placing a stone or changing turns.
    ///
    /// # Errors
    /// `GameError::NotLive` if the view is rewound.
    /// `GameError::GameOver` if the game is already finished.
    pub fn resign(&mut self) -> Result<(), GameError> {
        if self.is_rewound() {
            return Err(GameError::NotLive);
        }
        if !self.can_play() {
            return Err(GameError::GameOver);
        }
        self.history.resign_at_tip(self.to_move());
        self.refresh_outcome();
        Ok(())
    }

    /// Play a stone at `point`.
    ///
    /// At a reviewed position this follows an identical existing child or
    /// silently creates a sibling branch. No existing node is deleted. A
    /// rejected placement changes nothing.
    ///
    /// # Errors
    /// `GameError::Occupied` if the point holds a stone.
    /// `GameError::GameOver` if the board at the cursor is finished or the
    /// live game ended by resignation.
    pub fn play(&mut self, point: Move) -> Result<(), GameError> {
        if self.stone_at(point).is_some() {
            return Err(GameError::Occupied);
        }
        if !self.can_play() {
            return Err(GameError::GameOver);
        }
        self.board.play(point).map_err(GameError::from)?;
        self.history.play(point);
        self.refresh_outcome();
        Ok(())
    }

    /// Clear a resignation, delete the live tip, or rewind a reviewed position.
    ///
    /// Clearing resignation returns to live without removing a stone. At the
    /// live tip, ordinary undo deletes the last node and cannot be redone. While
    /// reviewing, it only steps back; `forward` still replays the selected line.
    pub fn undo(&mut self) -> bool {
        if self.history.resigned_at_tip().is_some() {
            self.live();
            self.history.clear_resignation();
            self.refresh_outcome();
            return true;
        }
        if self.is_rewound() {
            return self.rewind();
        }
        if !self.history.delete_tip() {
            return false;
        }
        self.board.undo();
        self.refresh_outcome();
        true
    }

    /// Step the view back one move. Returns false at the branch root.
    pub fn rewind(&mut self) -> bool {
        if !self.history.rewind() {
            return false;
        }
        self.board.undo();
        true
    }

    /// Step forward along the selected child branch. Returns false at its tip.
    pub fn forward(&mut self) -> bool {
        let Some(mv) = self.history.move_after_cursor() else {
            return false;
        };
        // Every stored child was legal at this position when inserted.
        self.board
            .play(mv)
            .expect("replaying a recorded move is legal");
        assert!(self.history.forward(), "a move implies a forward cursor");
        true
    }

    /// Change the selected child without entering it.
    ///
    /// The board and cursor stay put, while `forward` and the visible record
    /// follow the new continuation. Returns false for an invalid or unchanged
    /// selection.
    pub fn set_current_child(&mut self, index: usize) -> bool {
        if !self.history.set_current_child(index) {
            return false;
        }
        self.refresh_outcome();
        true
    }

    /// Select and enter one child branch by its index in `branch_options`.
    ///
    /// The choice is remembered for subsequent redo. Returns false without
    /// changing the game when the index does not exist.
    pub fn select_branch(&mut self, index: usize) -> bool {
        let Some(point) = self.history.select_branch(index) else {
            return false;
        };
        self.board
            .play(point)
            .expect("replaying a selected branch is legal");
        self.refresh_outcome();
        true
    }

    /// Move the view to a move number, clamped to the current branch length.
    ///
    /// Returns false when the view does not move.
    pub fn seek(&mut self, index: usize) -> bool {
        let target = index.min(self.len());
        if target == self.cursor() {
            return false;
        }
        while self.cursor() < target {
            self.forward();
        }
        while self.cursor() > target {
            self.rewind();
        }
        true
    }

    /// Return the view to the selected branch's latest move.
    pub fn live(&mut self) -> bool {
        self.seek(self.len())
    }

    fn refresh_outcome(&mut self) {
        let mut tip = self.board.clone();
        for &point in &self.moves()[self.cursor()..] {
            tip.play(point)
                .expect("the selected branch replays legally");
        }
        self.meta.outcome = match self.history.resigned_at_tip() {
            Some(resigned) => Outcome::Won {
                winner: resigned.other(),
                method: WinMethod::Resignation,
            },
            None => outcome_of(tip.status()),
        };
    }

    pub(crate) fn tree_nodes(&self) -> &[HistoryNode] {
        self.history.nodes()
    }

    pub(crate) fn board_at_node(&self, node: usize) -> Option<Board> {
        let mut board = Board::new();
        for point in self.history.path_to_node(node)? {
            board.play(point).ok()?;
        }
        Some(board)
    }

    pub(crate) fn restore_child(&mut self, parent: usize, point: Move) -> Result<usize, GameError> {
        let mut board = self.board_at_node(parent).ok_or(GameError::GameOver)?;
        if board.stone_at(point).is_some() {
            return Err(GameError::Occupied);
        }
        board.play(point).map_err(GameError::from)?;
        self.history
            .restore_child(parent, point)
            .ok_or(GameError::GameOver)
    }

    pub(crate) fn restore_current_child(&mut self, node: usize, child: Option<usize>) -> bool {
        self.history.restore_current_child(node, child)
    }

    pub(crate) fn restore_resignation(&mut self, node: usize, side: Color) -> bool {
        self.history.restore_resignation(node, side)
    }

    pub(crate) fn finish_restore(&mut self) -> bool {
        if !self.history.finish_restore() {
            return false;
        }
        self.board = Board::new();
        for point in self.moves().to_vec() {
            self.board
                .play(point)
                .expect("a validated record branch replays legally");
        }
        self.refresh_outcome();
        true
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

/// The colour of the stone played at a given index, starting from
/// `initial_to_move` and alternating.
fn color_of(index: usize, initial_to_move: Color) -> Color {
    if index % 2 == 0 {
        initial_to_move
    } else {
        initial_to_move.other()
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
    fn undo_deletes_the_live_tip() {
        let mut game = game();
        game.play(point(7, 7)).expect("empty cell");
        game.play(point(7, 8)).expect("empty cell");
        assert!(game.undo());
        assert_eq!(game.len(), 1);
        assert_eq!(game.cursor(), 1);
        assert_eq!(game.stone_at(point(7, 8)), None);
        assert_eq!(game.to_move(), Color::White);
        assert!(!game.forward());
    }

    #[test]
    fn undo_on_an_empty_game_does_nothing() {
        let mut game = game();
        assert!(!game.undo());
        assert!(game.is_empty());
    }

    #[test]
    fn undo_a_winning_move_allows_a_replacement() {
        let mut game = near_win();
        assert!(game.undo());
        assert_eq!(game.status(), Status::Ongoing);
        game.play(point(10, 10)).expect("empty cell");
        assert_eq!(game.len(), 9);
        assert_eq!(game.outcome(), Outcome::Ongoing);
        game.seek(8);
        assert_eq!(game.branch_options(), vec![point(10, 10)]);
        assert!(!game.is_branch_point(), "the winning child was deleted");
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
    fn undo_steps_back_from_the_current_view() {
        let mut game = game();
        game.play(point(7, 7)).expect("empty cell");
        game.play(point(7, 8)).expect("empty cell");
        game.rewind();
        assert!(game.undo());
        assert_eq!(game.len(), 2);
        assert_eq!(game.cursor(), 0);
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
    fn play_while_rewound_creates_a_sibling_branch() {
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
        game.seek(1);
        assert!(game.select_branch(0));
        assert_eq!(game.moves(), &[point(7, 7), point(7, 8), point(9, 9)]);
    }

    #[test]
    fn the_record_keeps_the_moves_that_the_board_has_left_behind() {
        let mut game = Game::new();
        for column in 0..4_u8 {
            game.play(point(0, column)).expect("the square is free");
            game.play(point(1, column)).expect("the square is free");
        }
        game.rewind();
        game.rewind();
        assert_eq!(game.stones().count(), 6, "the board stops at the cursor");
        let record: Vec<_> = game.record().collect();
        assert_eq!(record.len(), 8, "the record keeps every move");
        assert_eq!(record[6].0, point(0, 3), "Black plays the odd moves");
        assert_eq!(record[6].1, Color::Black);
        assert_eq!(record[7].0, point(1, 3));
        assert_eq!(record[7].1, Color::White, "and White plays the even ones");
    }

    #[test]
    fn meta_keeps_the_player_names_and_the_start_time() {
        let mut game = game();
        assert_eq!(game.meta().created, datetime!(2026-09-22 18:04:11 UTC));
        game.set_players(Some("Wolfie".to_string()), None);
        assert_eq!(game.meta().black.as_deref(), Some("Wolfie"));
        assert_eq!(game.meta().white, None);
    }

    #[test]
    fn from_position_loads_black_attacker_root() {
        let black = vec![point(7, 7)];
        let white = vec![point(6, 7)];
        let game = Game::from_position(black, white, Color::Black).expect("valid position");
        assert!(game.is_from_position());
        assert_eq!(game.to_move(), Color::Black);
        assert_eq!(game.len(), 0);
        assert_eq!(game.cursor(), 0);
        assert_eq!(game.stone_at(point(7, 7)), Some(Color::Black));
        assert_eq!(game.stone_at(point(6, 7)), Some(Color::White));
    }

    #[test]
    fn from_position_loads_white_attacker_root() {
        let black = vec![point(7, 7), point(7, 8)];
        let white = vec![point(6, 7)];
        let mut game = Game::from_position(black, white, Color::White).expect("valid position");
        assert_eq!(game.to_move(), Color::White);
        assert_eq!(game.len(), 0);
        // Play from the root must continue with the correct side.
        game.play(point(5, 7)).expect("empty cell");
        assert_eq!(game.stone_at(point(5, 7)), Some(Color::White));
        assert_eq!(game.to_move(), Color::Black);
    }

    #[test]
    fn play_after_from_position_alternates_from_the_root() {
        let black = vec![point(7, 7)];
        let white = vec![point(6, 7)];
        let mut game = Game::from_position(black, white, Color::Black).expect("valid position");
        game.play(point(5, 7)).expect("empty cell");
        game.play(point(8, 7)).expect("empty cell");
        assert_eq!(game.stone_at(point(5, 7)), Some(Color::Black));
        assert_eq!(game.stone_at(point(8, 7)), Some(Color::White));
    }

    #[test]
    fn rewind_and_undo_stop_at_the_position_root() {
        let black = vec![point(7, 7)];
        let white = vec![point(6, 7)];
        let mut game = Game::from_position(black, white, Color::Black).expect("valid position");
        game.play(point(5, 7)).expect("empty cell");
        game.play(point(8, 7)).expect("empty cell");
        assert!(game.rewind());
        assert!(game.rewind());
        assert!(
            !game.rewind(),
            "the root is the loaded position, not an empty board"
        );
        assert_eq!(game.stone_at(point(7, 7)), Some(Color::Black));
        assert_eq!(game.cursor(), 0);
        assert!(!game.undo(), "nothing to undo at the root");
    }

    #[test]
    fn from_position_refuses_inconsistent_counts() {
        let black = vec![point(7, 7), point(7, 8)];
        let white = vec![];
        assert_eq!(
            Game::from_position(black, white, Color::Black),
            Err(PositionError::InvalidCounts)
        );
    }

    #[test]
    fn from_position_refuses_a_duplicate_stone() {
        let black = vec![point(7, 7), point(7, 7)];
        let white = vec![];
        assert_eq!(
            Game::from_position(black, white, Color::White),
            Err(PositionError::DuplicateStone)
        );
    }
}
