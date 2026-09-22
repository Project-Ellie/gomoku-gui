//! `MoveSet`: public set-of-moves bitset, logical stride-15 indexing.
//! Slice 3. Deliberately separate from the internal stride-16 `Bitboard`.

/// A legal Gomoku cell: `row * 15 + col`, always in `0..=224`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Move(u8);

impl Move {
    /// Create a move from row and column; returns `None` if off the board.
    pub fn new(row: u8, col: u8) -> Option<Move> {
        if row < 15 && col < 15 {
            Some(Move(row * 15 + col))
        } else {
            None
        }
    }

    /// Row index, `0..=14`.
    pub fn row(self) -> u8 {
        self.0 / 15
    }

    /// Column index, `0..=14`.
    pub fn col(self) -> u8 {
        self.0 % 15
    }

    /// Logical stride-15 index, `0..=224`.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A set of moves as a bitset over logical indices (stride 15).
///
/// 225 bits in 4 `u64` words; the top 31 bits of word 3 can never be
/// set, because the only way in is `insert(Move)` and a `Move` is
/// always < 225. Value semantics, `Copy` — a set you can pass around
/// like a number. Deliberately separate from the internal stride-16
/// `Bitboard`: this type speaks the PUBLIC logical vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MoveSet([u64; 4]);

impl MoveSet {
    /// The empty set.
    pub const EMPTY: MoveSet = MoveSet([0; 4]);

    /// Insert `mv` into the set.
    pub fn insert(&mut self, mv: Move) {
        let i = mv.index();
        self.0[i / 64] |= 1 << (i % 64);
    }

    /// True if `mv` is in the set.
    pub fn contains(&self, mv: Move) -> bool {
        let i = mv.index();
        self.0[i / 64] & (1 << (i % 64)) != 0
    }

    /// Number of moves in the set.
    pub fn len(&self) -> u32 {
        self.0.iter().map(|w| w.count_ones()).sum()
    }

    /// True if the set contains no moves.
    pub fn is_empty(&self) -> bool {
        self.0 == [0; 4]
    }

    /// Iterate over the moves in the set.
    pub fn iter(&self) -> impl Iterator<Item = Move> + '_ {
        self.0.iter().enumerate().flat_map(|(w, &word)| {
            (0..64).filter_map(move |bit| {
                if word >> bit & 1 == 1 {
                    // Invariant: only bits < 225 can be set, so the
                    // index always fits a u8 and is a valid cell.
                    Some(Move((w * 64 + bit) as u8))
                } else {
                    None
                }
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_off_board_cells() {
        assert_eq!(Move::new(0, 15), None);
        assert_eq!(Move::new(15, 0), None);
        assert_eq!(Move::new(7, 3), Some(Move(108)));
    }

    #[test]
    fn index_row_col_are_consistent() {
        let mv = Move::new(7, 3).unwrap();
        assert_eq!(mv.index(), 7 * 15 + 3); // 108
        assert_eq!(mv.row(), 7);
        assert_eq!(mv.col(), 3);
    }

    #[test]
    fn moveset_roundtrip_across_word_boundaries() {
        let mut set = MoveSet::EMPTY;
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);

        // indices 0, 63, 64, 108, 224 — one per interesting seam
        let moves = [
            Move::new(0, 0).unwrap(),   //   0
            Move::new(4, 3).unwrap(),   //  63
            Move::new(4, 4).unwrap(),   //  64
            Move::new(7, 3).unwrap(),   // 108
            Move::new(14, 14).unwrap(), // 224
        ];
        for &mv in &moves {
            set.insert(mv);
        }
        assert!(!set.is_empty());
        assert_eq!(set.len(), 5);
        for &mv in &moves {
            assert!(set.contains(mv), "{mv:?} must be in the set");
        }
        assert!(!set.contains(Move::new(7, 7).unwrap()));

        let mut collected: Vec<usize> = set.iter().map(|m| m.index()).collect();
        collected.sort_unstable();
        assert_eq!(collected, vec![0, 63, 64, 108, 224]);
    }

    #[test]
    fn insert_is_idempotent() {
        let mut set = MoveSet::EMPTY;
        let mv = Move::new(7, 7).unwrap();
        set.insert(mv);
        set.insert(mv);
        assert_eq!(set.len(), 1);
    }
}
