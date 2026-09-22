//! `Board`: absolute colors + `to_move`, play/undo, legality, status.
//! Slice 3. See docs/13-engine-design.md, "Board".
use crate::bitboard::{Bitboard, VALID, idx};
use crate::moveset::Move;
use crate::zobrist;

/// Absolute stone color. Black moves first in normal play; during the
/// Swap2 opening colors are placed non-alternatingly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    /// The first-moving color.
    Black,
    /// The second color.
    White,
}

impl Color {
    /// The opposite color.
    pub fn other(self) -> Color {
        match self {
            Color::Black => Color::White,
            Color::White => Color::Black,
        }
    }
}

/// Game state. An enum, not bool flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The game is still open.
    Ongoing,
    /// The game has been won by the given color.
    Won(Color),
    /// The board is full with no winner.
    Draw,
}

/// Errors returned by `Board::play` and the Swap2 opening transitions.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PlayError {
    /// The target cell already contains a stone.
    #[error("Cell is already occupied.")]
    Occupied,
    /// The game has already ended.
    #[error("Game is already over.")]
    GameOver,
    /// The opening stage has the wrong stone counts.
    #[error("Opening has the wrong stone counts for this stage.")]
    BadOpeningCounts,
}

/// Errors returned when constructing a board from an arbitrary position.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PositionError {
    /// A cell appears in both the black and white lists.
    #[error("Black and white stones overlap on the same cell.")]
    OverlappingColors,
    /// Stone counts differ by more than one or disagree with `to_move`.
    #[error("Stone counts are inconsistent with the side to move.")]
    InvalidCounts,
}

/// The production board. Two bitboards in ABSOLUTE colors (ch. 13,
/// decision 5: Swap2's non-alternating opening cannot be expressed in
/// a relative me/you store), plus side to move, status, and full
/// move history (encoding, undo, game records).
///
/// `PartialEq` is derived for tests ("undo everything ⇒ equals
/// `Board::new()`"); it compares bitboards, not game meaning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    black: Bitboard,
    white: Bitboard,
    to_move: Color,
    status: Status,
    moves: Vec<Move>,
    key: u64, // the zobrist key
}

impl Board {
    /// An empty board with Black to move.
    pub fn new() -> Board {
        Board {
            black: Bitboard::EMPTY,
            white: Bitboard::EMPTY,
            to_move: Color::Black,
            status: Status::Ongoing,
            moves: Vec::new(),
            key: 0, // empty board is black to move, and black starts with 0
        }
    }

    /// The incremental Zobrist key of the current position.
    pub fn zobrist(&self) -> u64 {
        self.key
    }

    /// Play a stone for the side to move.
    ///
    /// # Errors
    /// * `PlayError::GameOver` if the game is no longer ongoing.
    /// * `PlayError::Occupied` if the cell is already occupied.
    ///
    /// # Examples
    ///
    /// ```
    /// use engine::{Board, Move};
    ///
    /// let mut b = Board::new();
    /// b.play(Move::new(7, 7).unwrap()).unwrap();
    /// assert_eq!(b.moves().len(), 1);
    /// ```
    pub fn play(&mut self, mv: Move) -> Result<(), PlayError> {
        if self.status != Status::Ongoing {
            return Err(PlayError::GameOver);
        }
        let i = idx(mv.row(), mv.col());
        if (self.black | self.white).test(i) {
            return Err(PlayError::Occupied);
        }
        match self.to_move {
            Color::Black => self.black = self.black.with_bit(i),
            Color::White => self.white = self.white.with_bit(i),
        }
        self.moves.push(mv);

        if crate::win::has_any_five(&self.stones(self.to_move)) {
            self.status = Status::Won(self.to_move);
        } else if self.moves.len() == 225 {
            self.status = Status::Draw;
        }

        self.key ^= zobrist::key_for(self.to_move, mv.index());
        self.key ^= zobrist::WHITE_TO_MOVE;
        self.to_move = self.to_move.other();

        Ok(())
    }

    /// The current game status.
    pub fn status(&self) -> Status {
        self.status
    }

    /// The side whose turn it is to move.
    pub fn to_move(&self) -> Color {
        self.to_move
    }

    /// The move history, in play order.
    pub fn moves(&self) -> &[Move] {
        &self.moves
    }

    /// True if `mv` is legal in the current position.
    pub fn is_legal(&self, mv: Move) -> bool {
        self.status == Status::Ongoing && !(self.black | self.white).test(idx(mv.row(), mv.col()))
    }

    /// Undo the last move.
    ///
    /// # Panics
    /// Panics if the move history is empty.
    pub fn undo(&mut self) {
        let mv = self.moves.pop().expect("undo with empty history");
        let i = idx(mv.row(), mv.col());
        match self.to_move.other() {
            Color::Black => self.black = self.black.without_bit(i),
            Color::White => self.white = self.white.without_bit(i),
        }

        self.key ^= zobrist::key_for(self.to_move.other(), mv.index());
        self.key ^= zobrist::WHITE_TO_MOVE;
        self.to_move = self.to_move.other(); // flip back
        // Undoing the winning move un-wins the game; undoing into a
        // would-be draw likewise reopens it. Ongoing is always right.
        self.status = Status::Ongoing;
    }

    /// An iterator over all legal empty cells.
    pub fn empty_moves(&self) -> impl Iterator<Item = Move> + '_ {
        // `!occupied` sets ALL padding bits — mask immediately.
        // This is the one place complements are allowed, and the mask
        // is non-negotiable (the padding invariant, ch. 13).
        let empty = !(self.black | self.white) & VALID;

        // The classic set-bit walk, word by word. `bits & (bits - 1)`
        // clears the lowest set bit — commit it to memory, you will
        // meet it in every bitboard codebase.
        empty.0.into_iter().enumerate().flat_map(|(w, mut bits)| {
            std::iter::from_fn(move || {
                if bits == 0 {
                    return None; // word exhausted → next word
                }
                let bit = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                let i = w * 64 + bit; // stride-16 index
                // Back across the boundary: stride-16 → (row, col) →
                // stride-15 Move. The `unwrap` is justified by VALID:
                // padding bits are never set in `empty`, so c < 15
                // always. Drop the mask and this panics — LOUD, never
                // silently wrong.
                Some(Move::new((i / 16) as u8, (i % 16) as u8).unwrap())
            })
        })
    }

    /// The bitboard of stones for `color` (internal layout).
    pub(crate) fn stones(&self, color: Color) -> Bitboard {
        match color {
            Color::Black => self.black,
            Color::White => self.white,
        }
    }

    /// The color of the stone at `mv`, if any.
    pub fn stone_at(&self, mv: Move) -> Option<Color> {
        let idx = mv.row() as usize * 16 + mv.col() as usize;
        if self.black.test(idx) {
            Some(Color::Black)
        } else if self.white.test(idx) {
            Some(Color::White)
        } else {
            None
        }
    }

    /// Returns true if `color` has five or more stones in a row.
    ///
    /// This is the public primitive the win-detection benchmark measures;
    /// it exposes the result without leaking the internal `Bitboard` type.
    #[inline]
    pub fn has_five_any(&self, color: Color) -> bool {
        crate::win::has_any_five(&self.stones(color))
    }

    /// Build a board from an arbitrary valid position.
    ///
    /// Validates that the two colors do not overlap and that the stone
    /// counts differ by at most one, with `to_move` being the color
    /// that has fewer-or-equal stones. The Zobrist key is computed from
    /// scratch via `zobrist::compute_key`.
    ///
    /// # Errors
    /// * `PositionError::OverlappingColors` if a cell appears in both lists.
    /// * `PositionError::InvalidCounts` if counts differ by more than one
    ///   or do not match `to_move`.
    pub fn from_position(
        black: &[Move],
        white: &[Move],
        to_move: Color,
    ) -> Result<Board, PositionError> {
        let mut bb_black = Bitboard::EMPTY;
        let mut bb_white = Bitboard::EMPTY;
        for mv in black {
            bb_black = bb_black.with_bit(idx(mv.row(), mv.col()));
        }
        for mv in white {
            bb_white = bb_white.with_bit(idx(mv.row(), mv.col()));
        }

        if !(bb_black & bb_white).is_zero() {
            return Err(PositionError::OverlappingColors);
        }

        let black_count = bb_black.count() as i32;
        let white_count = bb_white.count() as i32;
        if (black_count - white_count).abs() > 1 {
            return Err(PositionError::InvalidCounts);
        }

        let expected_to_move = if white_count < black_count {
            Color::White
        } else {
            Color::Black
        };
        if to_move != expected_to_move {
            return Err(PositionError::InvalidCounts);
        }

        let mut board = Board {
            black: bb_black,
            white: bb_white,
            to_move,
            status: Status::Ongoing,
            moves: Vec::new(),
            key: 0,
        };
        board.key = zobrist::compute_key(&board.black, &board.white, board.to_move);
        board.status = if crate::win::has_any_five(&board.black) {
            Status::Won(Color::Black)
        } else if crate::win::has_any_five(&board.white) {
            Status::Won(Color::White)
        } else if board.black.count() + board.white.count() == 225 {
            Status::Draw
        } else {
            Status::Ongoing
        };
        Ok(board)
    }
}

impl Default for Board {
    /// Defaults to an empty board with Black to move.
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Move, Status, reference};

    #[test]
    fn new_board_has_225_legal_moves_black_to_move() {
        let b = Board::new();
        assert_eq!(b.status(), Status::Ongoing);
        assert_eq!(b.to_move(), Color::Black);
        assert_eq!(b.empty_moves().count(), 225);
        assert!(b.moves().is_empty());
    }

    #[test]
    fn play_places_a_stone_and_flips() {
        let mut b = Board::new();
        let mv = Move::new(7, 7).unwrap();
        b.play(mv).unwrap();
        assert_eq!(b.to_move(), Color::White);
        assert!(!b.is_legal(mv));
        assert_eq!(b.empty_moves().count(), 224);
    }

    #[test]
    fn empty_moves_and_legality_track_the_stones() {
        let mut b = Board::new();
        let mv = Move::new(7, 7).unwrap();
        assert!(b.is_legal(mv));

        b.play(mv).unwrap();
        assert!(!b.is_legal(mv));
        assert_eq!(b.empty_moves().count(), 224);
        assert!(b.empty_moves().all(|m| m != mv));

        for r in 0..15u8 {
            for c in 0..15u8 {
                let _ = b.play(Move::new(r, c).unwrap());
            }
        }
        assert_eq!(b.empty_moves().count(), 225 - b.moves.len());
        assert_eq!(b.status(), Status::Won(Color::White))
    }

    #[test]
    fn horizontal_five_wins() {
        let mut b = reference::Board::new();
        let script = [
            (7, 3),
            (0, 0),
            (7, 4),
            (0, 2),
            (7, 5),
            (0, 4),
            (7, 6),
            (0, 6),
            (7, 7),
        ];
        for (r, c) in script {
            b.play(Move::new(r, c).unwrap()).unwrap();
        }
        assert_eq!(b.status(), Status::Won(Color::Black));

        assert_eq!(b.play(Move::new(13, 13).unwrap()), Err(PlayError::GameOver))
    }

    #[test]
    fn undo_everything_returns_a_pristine_board() {
        let mut b = Board::new();
        let script = [(5, 6), (5, 7), (4, 6), (3, 6)];
        for (r, c) in script {
            b.play(Move::new(r, c).unwrap()).unwrap();
        }
        for _ in 0..script.len() {
            b.undo();
        }
        assert_eq!(b, Board::new());
    }

    #[test]
    fn undo_the_winning_move_unwins_the_game() {
        let mut b = Board::new();
        let script = [
            (7, 3),
            (0, 0),
            (7, 4),
            (0, 2),
            (7, 5),
            (0, 4),
            (7, 6),
            (0, 6),
            (7, 7),
        ];
        for (r, c) in script {
            b.play(Move::new(r, c).unwrap()).unwrap();
        }
        assert_eq!(b.status(), Status::Won(Color::Black));

        b.undo();
        assert_eq!(b.status(), Status::Ongoing);
        assert_eq!(b.to_move(), Color::Black);
        assert!(b.is_legal(Move::new(7, 7).unwrap()));
        assert_eq!(b.moves.len(), 8);
    }

    #[test]
    fn stone_at_impls_agree_for_all_positions() {
        let mut fast = Board::new();
        let mut naive = reference::Board::new();
        for &(r, c) in &[(7u8, 7u8), (0, 14), (14, 0), (3, 11)] {
            let mv = Move::new(r, c).unwrap();
            assert_eq!(fast.play(mv), naive.play(mv));
        }
        for r in 0..15 {
            for c in 0..15 {
                let mv = Move::new(r, c).unwrap();
                assert_eq!(fast.stone_at(mv), naive.stone_at(mv), "at ({r}, {r})");
            }
        }
        assert_eq!(fast.status(), Status::Ongoing);
    }
}
