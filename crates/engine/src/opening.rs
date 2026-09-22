//! Swap2 opening protocol as a typestate machine.
//! Slice 9. See docs/13-engine-design.md, "Swap2 as a typestate machine".

use std::marker::PhantomData;

use crate::bitboard::{Bitboard, idx};
use crate::board::{Board, Color, PlayError};

#[cfg(test)]
use crate::PositionError;
use crate::moveset::Move;

/// Player A places three stones: two Black and one White.
#[derive(Debug)]
pub struct Placing3;

/// Player B chooses a color or places two more stones.
#[derive(Debug)]
pub struct FirstChoice;

/// Player B places one Black and one White stone.
#[derive(Debug)]
pub struct Placing2;

/// Player A picks a color after the extra stones were placed.
#[derive(Debug)]
pub struct FinalChoice;

/// The Swap2 opening protocol, parameterized by its current state.
///
/// Invalid transitions are compile errors: `take_black` cannot be called
/// on a fresh opening, and `finish` cannot be called twice.
#[derive(Debug)]
pub struct Swap2<State> {
    black: Bitboard,
    white: Bitboard,
    _state: PhantomData<State>,
}

impl Swap2<Placing3> {
    /// Start a fresh Swap2 opening.
    pub fn new() -> Self {
        Self {
            black: Bitboard::EMPTY,
            white: Bitboard::EMPTY,
            _state: PhantomData,
        }
    }
}

impl Default for Swap2<Placing3> {
    fn default() -> Self {
        Self::new()
    }
}

impl Swap2<Placing3> {
    /// Place one stone of the given color on the board.
    ///
    /// # Errors
    /// Returns `PlayError::Occupied` if the cell already has a stone.
    pub fn place(&mut self, mv: Move, color: Color) -> Result<(), PlayError> {
        let i = idx(mv.row(), mv.col());
        if (self.black | self.white).test(i) {
            return Err(PlayError::Occupied);
        }
        match color {
            Color::Black => self.black = self.black.with_bit(i),
            Color::White => self.white = self.white.with_bit(i),
        }
        Ok(())
    }

    /// Finish the first placement phase.
    ///
    /// # Errors
    /// Returns `PlayError::BadOpeningCounts` unless the position is exactly
    /// 2 Black stones and 1 White stone.
    pub fn finish(self) -> Result<Swap2<FirstChoice>, PlayError> {
        if self.black.count() == 2 && self.white.count() == 1 {
            Ok(Swap2 {
                black: self.black,
                white: self.white,
                _state: PhantomData,
            })
        } else {
            Err(PlayError::BadOpeningCounts)
        }
    }
}

impl Swap2<FirstChoice> {
    /// Take Black and continue with White to move (White has fewer stones).
    pub fn take_black(self) -> Board {
        self.build_board(Color::Black)
    }

    /// Take White and continue with White to move (White has fewer stones).
    pub fn take_white(self) -> Board {
        self.build_board(Color::White)
    }

    /// Player B places two more stones instead of choosing a color.
    pub fn place_two_more(self) -> Swap2<Placing2> {
        Swap2 {
            black: self.black,
            white: self.white,
            _state: PhantomData,
        }
    }

    fn build_board(self, _chosen: Color) -> Board {
        let black = moves_of(&self.black);
        let white = moves_of(&self.white);
        // The side to move is determined by stone counts, not by who chose.
        let to_move = if white.len() < black.len() {
            Color::White
        } else {
            Color::Black
        };
        Board::from_position(&black, &white, to_move)
            .expect("Swap2 invariant guarantees a valid position")
    }
}

impl Swap2<Placing2> {
    /// Place one stone of the given color.
    ///
    /// # Errors
    /// Returns `PlayError::Occupied` if the cell is already occupied.
    pub fn place(&mut self, mv: Move, color: Color) -> Result<(), PlayError> {
        let i = idx(mv.row(), mv.col());
        if (self.black | self.white).test(i) {
            return Err(PlayError::Occupied);
        }
        match color {
            Color::Black => self.black = self.black.with_bit(i),
            Color::White => self.white = self.white.with_bit(i),
        }
        Ok(())
    }

    /// Finish the second placement phase.
    ///
    /// # Errors
    /// Returns `PlayError::BadOpeningCounts` unless the position is exactly
    /// 3 Black stones and 2 White stones.
    pub fn finish(self) -> Result<Swap2<FinalChoice>, PlayError> {
        if self.black.count() == 3 && self.white.count() == 2 {
            Ok(Swap2 {
                black: self.black,
                white: self.white,
                _state: PhantomData,
            })
        } else {
            Err(PlayError::BadOpeningCounts)
        }
    }
}

impl Swap2<FinalChoice> {
    /// Take Black and continue with White to move (White has fewer stones).
    pub fn take_black(self) -> Board {
        self.build_board(Color::Black)
    }

    /// Take White and continue with White to move (White has fewer stones).
    pub fn take_white(self) -> Board {
        self.build_board(Color::White)
    }

    fn build_board(self, _chosen: Color) -> Board {
        let black = moves_of(&self.black);
        let white = moves_of(&self.white);
        let to_move = if white.len() < black.len() {
            Color::White
        } else {
            Color::Black
        };
        Board::from_position(&black, &white, to_move)
            .expect("Swap2 invariant guarantees a valid position")
    }
}

fn moves_of(bb: &Bitboard) -> Vec<Move> {
    bb.iter_set_bits()
        .map(|i| Move::new((i / 16) as u8, (i % 16) as u8).expect("VALID bits are real cells"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zobrist;

    fn mv(r: u8, c: u8) -> Move {
        Move::new(r, c).unwrap()
    }

    #[test]
    fn branch_1_take_white() {
        let mut s = Swap2::<Placing3>::new();
        s.place(mv(7, 7), Color::Black).unwrap();
        s.place(mv(6, 6), Color::Black).unwrap();
        s.place(mv(5, 5), Color::White).unwrap();
        let s = s.finish().unwrap();
        let b = s.take_white();
        assert_eq!(b.stones(Color::Black).count(), 2);
        assert_eq!(b.stones(Color::White).count(), 1);
        assert_eq!(b.to_move(), Color::White);
    }

    #[test]
    fn branch_2_take_black() {
        let mut s = Swap2::<Placing3>::new();
        s.place(mv(7, 7), Color::Black).unwrap();
        s.place(mv(6, 6), Color::Black).unwrap();
        s.place(mv(5, 5), Color::White).unwrap();
        let s = s.finish().unwrap();
        let b = s.take_black();
        assert_eq!(b.stones(Color::Black).count(), 2);
        assert_eq!(b.stones(Color::White).count(), 1);
        assert_eq!(b.to_move(), Color::White);
    }

    #[test]
    fn branch_3_place_two_more_then_choose() {
        let mut s = Swap2::<Placing3>::new();
        s.place(mv(7, 7), Color::Black).unwrap();
        s.place(mv(6, 6), Color::Black).unwrap();
        s.place(mv(5, 5), Color::White).unwrap();
        let s = s.finish().unwrap();
        let mut s = s.place_two_more();
        s.place(mv(4, 4), Color::Black).unwrap();
        s.place(mv(3, 3), Color::White).unwrap();
        let s = s.finish().unwrap();

        let black = s.take_black();
        assert_eq!(black.stones(Color::Black).count(), 3);
        assert_eq!(black.stones(Color::White).count(), 2);
        assert_eq!(black.to_move(), Color::White);

        // Rebuild and take white: counts and side to move are identical.
        let mut s = Swap2::<Placing3>::new();
        s.place(mv(7, 7), Color::Black).unwrap();
        s.place(mv(6, 6), Color::Black).unwrap();
        s.place(mv(5, 5), Color::White).unwrap();
        let s = s.finish().unwrap();
        let mut s = s.place_two_more();
        s.place(mv(4, 4), Color::Black).unwrap();
        s.place(mv(3, 3), Color::White).unwrap();
        let s = s.finish().unwrap();
        let white = s.take_white();
        assert_eq!(white.to_move(), Color::White);
    }

    #[test]
    fn finish_rejects_wrong_counts() {
        let mut s = Swap2::<Placing3>::new();
        s.place(mv(0, 0), Color::Black).unwrap();
        s.place(mv(1, 1), Color::White).unwrap();
        assert!(matches!(s.finish(), Err(PlayError::BadOpeningCounts)));

        let mut s = Swap2::<Placing3>::new();
        s.place(mv(0, 0), Color::Black).unwrap();
        s.place(mv(1, 1), Color::Black).unwrap();
        s.place(mv(2, 2), Color::Black).unwrap();
        assert!(matches!(s.finish(), Err(PlayError::BadOpeningCounts)));
    }

    #[test]
    fn place_rejects_occupied_cell() {
        let mut s = Swap2::<Placing3>::new();
        s.place(mv(7, 7), Color::Black).unwrap();
        assert_eq!(s.place(mv(7, 7), Color::White), Err(PlayError::Occupied));
    }

    #[test]
    fn from_position_rejects_overlap_and_bad_counts() {
        let same = mv(7, 7);
        assert_eq!(
            Board::from_position(&[same], &[same], Color::White),
            Err(PositionError::OverlappingColors)
        );

        // 2 Black, 1 White but claiming Black to move.
        assert_eq!(
            Board::from_position(&[mv(0, 0), mv(1, 1)], &[mv(2, 2)], Color::Black),
            Err(PositionError::InvalidCounts)
        );

        // Counts differ by more than one.
        assert_eq!(
            Board::from_position(&[mv(0, 0), mv(1, 1), mv(2, 2)], &[mv(3, 3)], Color::White),
            Err(PositionError::InvalidCounts)
        );
    }

    #[test]
    fn from_position_accepts_empty_board() {
        let b = Board::from_position(&[], &[], Color::Black).unwrap();
        assert_eq!(b.to_move(), Color::Black);
        assert_eq!(b.zobrist(), 0);
    }

    #[test]
    fn opening_and_played_position_share_zobrist_key() {
        let a = mv(7, 7);
        let b = mv(6, 6);
        let w = mv(5, 5);

        // Opening: 2 Black, 1 White, then take a color.
        let mut opening = Swap2::<Placing3>::new();
        opening.place(a, Color::Black).unwrap();
        opening.place(b, Color::Black).unwrap();
        opening.place(w, Color::White).unwrap();
        let opening_board = opening.finish().unwrap().take_black();

        // Same position reached by alternating play: Black, White, Black.
        let mut played = Board::new();
        played.play(a).unwrap();
        played.play(w).unwrap();
        played.play(b).unwrap();

        assert_eq!(
            opening_board.stones(Color::Black),
            played.stones(Color::Black)
        );
        assert_eq!(
            opening_board.stones(Color::White),
            played.stones(Color::White)
        );
        assert_eq!(opening_board.to_move(), played.to_move());
        assert_eq!(opening_board.zobrist(), played.zobrist());
        assert_eq!(
            opening_board.zobrist(),
            zobrist::compute_key(
                &opening_board.stones(Color::Black),
                &opening_board.stones(Color::White),
                opening_board.to_move(),
            )
        );
    }
}
