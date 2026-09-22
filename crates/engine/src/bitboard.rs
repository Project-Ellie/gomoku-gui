//! Stride-16 bitboard over `[u64; 4]` — internal only (`pub(crate)`).
//! Slice 3. Invariant: padding bits (column 15 of each row, bits 240-255)
//! are always zero; complements must immediately AND with `VALID`.
use std::ops::{BitAnd, BitOr, Not};

pub(crate) const fn idx(r: u8, c: u8) -> usize {
    r as usize * 16 + c as usize
}

/// 256 bits, of which 240 are addressable cells (15 rows x 16 stride)
/// and 225 are real board cells. Column 15 of every row plus bits
/// 240-255 are padding - the invariant says they are ALWAYS zero.
///
/// NewType over `[u64; 4]`: `Copy` (32 bytes - cheaper than a
/// reference), and the wrapper keeps bitboard math from mixing with
/// plain ntegers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Bitboard(pub(crate) [u64; 4]);

impl Bitboard {
    pub(crate) const EMPTY: Bitboard = Bitboard([0; 4]);

    pub(crate) fn with_bit(self, idx: usize) -> Bitboard {
        let mut w = self.0;
        w[idx / 64] |= 1 << (idx % 64);
        Bitboard(w)
    }

    pub(crate) fn without_bit(self, idx: usize) -> Bitboard {
        let mut w = self.0;
        w[idx / 64] &= !(1 << (idx % 64));
        Bitboard(w)
    }

    pub(crate) fn test(&self, idx: usize) -> bool {
        self.0[idx / 64] & (1 << (idx % 64)) != 0
    }

    /// Iterate the indices of all set bits, lowest first — the classic
    /// `bits & (bits - 1)` lowest-bit walk, word by word. Walks
    /// whatever bits are set, padding included; masking dirty
    /// complements is the caller's business (the padding invariant).
    pub(crate) fn iter_set_bits(self) -> impl Iterator<Item = usize> {
        self.0.into_iter().enumerate().flat_map(|(w, mut bits)| {
            std::iter::from_fn(move || {
                if bits == 0 {
                    return None;
                }
                let bit = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                Some(w * 64 + bit)
            })
        })
    }

    // The three methods below are what slice 4's staged win detection
    // consumes — `count`/`is_zero` to inspect results, `shr` as the
    // shift-AND primitive. Nothing in the crate uses them yet, hence the
    // narrow allows; drop them when `win.rs` lands.
    #[allow(dead_code)]
    pub(crate) fn is_zero(&self) -> bool {
        self.0 == [0; 4]
    }

    #[allow(dead_code)]
    pub(crate) fn count(&self) -> u32 {
        self.0.iter().map(|w| w.count_ones()).sum()
    }

    /// Logical right shift of the whole 256-bit value by `s`,
    /// `0 < s < 64`. Bits shifted past the top are lost (fine: they
    /// are padding or beyond).
    #[allow(dead_code)]
    pub(crate) fn shr(&self, s: u32) -> Bitboard {
        // THE TRAP: `x << 64` on u64 is a panic in debug builds and
        // SILENT GARBAGE in release (the hardware masks the shift
        // amount to 6 bits, so `<< 64` behaves like `<< 0`). Our
        // callers only ever pass 1, 15, 16, 17, 2*s of those — never
        // 0 or ≥64. debug_assert enforces it loudly in tests, free in
        // release. (Slice 4's staged AND exists precisely to keep
        // every shift under 64 — ch. 13.)
        debug_assert!(s > 0 && s < 64);
        let w = self.0;
        Bitboard([
            // Each output word keeps its own bits moved down by s, and
            // its TOP s bits come from the NEXT word's bottom s bits —
            // `w[k+1] << (64 - s)` is the carry bridge across the seam.
            (w[0] >> s) | (w[1] << (64 - s)),
            (w[1] >> s) | (w[2] << (64 - s)),
            (w[2] >> s) | (w[3] << (64 - s)),
            w[3] >> s,
        ])
    }
}

impl BitAnd for Bitboard {
    type Output = Bitboard;

    fn bitand(self, rhs: Bitboard) -> Bitboard {
        Bitboard([
            self.0[0] & rhs.0[0],
            self.0[1] & rhs.0[1],
            self.0[2] & rhs.0[2],
            self.0[3] & rhs.0[3],
        ])
    }
}

impl BitOr for Bitboard {
    type Output = Bitboard;

    fn bitor(self, rhs: Bitboard) -> Bitboard {
        Bitboard([
            self.0[0] | rhs.0[0],
            self.0[1] | rhs.0[1],
            self.0[2] | rhs.0[2],
            self.0[3] | rhs.0[3],
        ])
    }
}

impl Not for Bitboard {
    type Output = Bitboard;

    fn not(self) -> Bitboard {
        Bitboard([!self.0[0], !self.0[1], !self.0[2], !self.0[3]])
    }
}

/// Observe the bit index: 15 is left in the bit order but right-most in the visual representation
/// Similar reasoning holds for the position of the 0x0000 in the bottom u64
pub(crate) const VALID: Bitboard = Bitboard([
    0x7FFF_7FFF_7FFF_7FFF,
    0x7FFF_7FFF_7FFF_7FFF,
    0x7FFF_7FFF_7FFF_7FFF,
    0x0000_7FFF_7FFF_7FFF,
]);

#[cfg(test)]
pub(crate) fn assert_clean(b: &Bitboard) {
    assert!(
        (*b & !VALID).is_zero(),
        "padding invariant violated: {b:?} has bits outside the 225 real cells"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_test_clear_on_corners_and_edges() {
        let bb = Bitboard::EMPTY;
        assert!(bb.is_zero());

        let bb = bb.with_bit(idx(0, 0));
        assert!(bb.test(idx(0, 0)));
        assert!(!bb.test(idx(1, 0)));
        assert_eq!(bb.count(), 1);

        let bb = bb
            .with_bit(idx(0, 14))
            .with_bit(idx(14, 0))
            .with_bit(idx(14, 14));
        assert_eq!(bb.count(), 4);

        let bb = bb.without_bit(idx(0, 0));
        assert!(!bb.test(idx(0, 0)));
        assert_eq!(bb.count(), 3);
    }

    #[test]
    fn bits_crossing_a_word_boundary_land_correctly() {
        let bb = Bitboard::EMPTY.with_bit(idx(3, 14)).with_bit(idx(4, 0));
        assert!(bb.test(idx(3, 14)));
        assert!(bb.test(idx(4, 0)));
        assert!(!bb.test(idx(4, 1)));
        assert_eq!(bb.count(), 2);
    }

    #[test]
    fn operators_make_bitboard_math_readable() {
        let a = Bitboard::EMPTY.with_bit(idx(7, 7)).with_bit(idx(7, 8));
        let b = Bitboard::EMPTY.with_bit(idx(7, 8)).with_bit(idx(7, 9));

        assert_eq!((a & b).count(), 1);
        assert_eq!((a | b).count(), 3);

        assert!(!(!a).test(idx(7, 7)));
        assert!((!a).test(idx(0, 0)));
        assert!((!a).test(idx(0, 15)));
    }

    #[test]
    fn valid_has_225_bits_and_clean_padding() {
        assert_eq!(VALID.count(), 225);
        assert!((VALID & !VALID).is_zero());
        assert_clean(&VALID);
    }

    #[test]
    #[should_panic]
    fn assert_clean_catches_dirty_padding() {
        let dirty = Bitboard::EMPTY.with_bit(idx(7, 15));
        assert_clean(&dirty);
    }

    #[test]
    fn shr_moves_stones_down_by_the_shift_amount() {
        // Stone at (2,3) = idx 35. shr(s) SUBTRACTS s from every bit
        // index — it is a true >> on the 256-bit value. (Win detection
        // does not care which way the shift goes: b & b.shr(s) pairs
        // stones at i and i+s either way, and slice 4 relies on exactly
        // that. What matters is that the four DIRECTIONS are uniform.)
        let bb = Bitboard::EMPTY.with_bit(idx(2, 3));
        assert!(bb.shr(1).test(idx(2, 2))); //  35−1  = 34 = (2,2)  ←
        assert!(bb.shr(16).test(idx(1, 3))); // 35−16 = 19 = (1,3)  ↑
        assert!(bb.shr(15).test(idx(1, 4))); // 35−15 = 20 = (1,4)  ↗
        assert!(bb.shr(17).test(idx(1, 2))); // 35−17 = 18 = (1,2)  ↖
        assert_eq!(bb.shr(1).count(), 1); // shifts duplicate nothing
    }

    #[test]
    fn shr_carries_bits_across_word_boundaries() {
        // idx 77 lives in word 1 (bits 64–127); 77−17 = 60 lands in
        // word 0. This is the case a naive per-word `>>` gets wrong.
        let bb = Bitboard::EMPTY.with_bit(idx(4, 13)); // 4*16+13 = 77
        let shifted = bb.shr(17);
        assert!(shifted.test(idx(3, 12))); // 60 = 3*16+12
        assert_eq!(shifted.count(), 1);
    }
}
