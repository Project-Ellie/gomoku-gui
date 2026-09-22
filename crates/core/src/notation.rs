//! Display labels for board points, in tournament notation.

use engine::Move;

const COLUMN_LETTERS: [u8; 15] = *b"ABCDEFGHIJKLMNO";

/// The display label of a point: columns A to O, rows 1 to 15 counted from
/// the bottom, so the centre of the board is `H8`.
pub fn label(point: Move) -> String {
    // Move::col() is at most 14, so the index is always inside the array.
    let letter = COLUMN_LETTERS[point.col() as usize] as char;
    let number = 15 - point.row();
    format!("{letter}{number}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn point(row: u8, col: u8) -> Move {
        Move::new(row, col).expect("row and col are inside the board")
    }

    #[test]
    fn centre_of_the_board_is_h8() {
        assert_eq!(label(point(7, 7)), "H8");
    }

    #[test]
    fn every_column_has_its_own_letter() {
        assert_eq!(label(point(0, 0)), "A15");
        assert_eq!(label(point(0, 8)), "I15");
        assert_eq!(label(point(0, 14)), "O15");
    }

    #[test]
    fn rows_count_from_the_bottom() {
        assert_eq!(label(point(0, 0)), "A15");
        assert_eq!(label(point(14, 0)), "A1");
        assert_eq!(label(point(14, 14)), "O1");
    }

    #[test]
    fn all_labels_are_unique() {
        let mut seen = HashSet::new();
        for row in 0..15u8 {
            for col in 0..15u8 {
                assert!(seen.insert(label(point(row, col))));
            }
        }
        assert_eq!(seen.len(), 225);
    }
}
