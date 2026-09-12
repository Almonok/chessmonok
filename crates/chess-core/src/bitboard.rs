//! A set of squares packed into a single `u64`.
//!
//! Bit `n` of the wrapped integer stands for square `n` in the rank-major
//! numbering [`Square`] defines: bit 0 is a1, bit 63 is h8. That equivalence is
//! the whole point — a set of squares becomes one machine word, so "every
//! square this rook attacks" or "every square White occupies" is a value the
//! CPU manipulates in a single instruction.

use std::fmt;
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not};

use crate::{File, Rank, Square};

/// A set of squares, one bit per square.
///
/// Unlike [`Square`], this type has no invariant to protect: all 2^64 bit
/// patterns are meaningful sets, so [`Bitboard::from_bits`] can accept any
/// `u64` and the set operations can never produce an invalid value.
///
/// Every operation on a `Bitboard` is pure: it reads the set and returns a new
/// one, never mutating in place except through the explicit `&mut self`
/// methods. Computing a board and discarding it is therefore always dead code,
/// so the whole type is `#[must_use]` rather than each method separately.
#[must_use]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Bitboard(u64);

impl Bitboard {
    /// The empty set — no squares occupied.
    pub const EMPTY: Self = Self(0);

    /// Every square on the board.
    pub const FULL: Self = Self(u64::MAX);

    /// The set containing exactly one square.
    pub const fn from_square(square: Square) -> Self {
        Self(1u64 << square.index())
    }

    /// Wraps a raw bit pattern, where bit `n` means square `n`.
    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }

    /// The raw bit pattern.
    pub const fn bits(self) -> u64 {
        self.0
    }

    /// Whether `square` is in the set.
    pub const fn contains(self, square: Square) -> bool {
        self.0 & Self::from_square(square).0 != 0
    }

    /// Whether the set has no squares at all.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// How many squares are in the set.
    ///
    /// This compiles to a single `popcount` instruction, which is why material
    /// counting and mobility scoring can afford to call it in a hot loop.
    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }

    /// Adds `square` to the set. A no-op if it is already there.
    pub const fn insert(&mut self, square: Square) {
        self.0 |= Self::from_square(square).0;
    }

    /// Removes `square` from the set. A no-op if it was not there.
    pub const fn remove(&mut self, square: Square) {
        self.0 &= !Self::from_square(square).0;
    }

    /// Adds `square` if absent, removes it if present.
    ///
    /// Toggling is how make/unmake will move a piece: the same call that lifts
    /// it off one square puts it back, so undo needs no separate code path.
    pub const fn toggle(&mut self, square: Square) {
        self.0 ^= Self::from_square(square).0;
    }

    /// The lowest-numbered square in the set, or `None` if it is empty.
    ///
    /// "Lowest-numbered" means closest to a1 in the rank-major order, so
    /// scanning a board this way walks a1, b1, ... h1, a2, ... h8.
    pub const fn lsb(self) -> Option<Square> {
        if self.is_empty() {
            None
        } else {
            // `trailing_zeros` counts the zeros below the lowest set bit, which
            // *is* that bit's position — and a bit position is a square index.
            Square::from_index(self.0.trailing_zeros() as u8)
        }
    }

    /// Removes and returns the lowest-numbered square, or `None` if empty.
    ///
    /// This is the workhorse of move generation: a loop of `pop_lsb` visits
    /// every square in a set exactly once, doing no work for the empty ones.
    pub const fn pop_lsb(&mut self) -> Option<Square> {
        match self.lsb() {
            Some(square) => {
                // Subtracting one flips the lowest set bit to zero and turns
                // every zero below it into a one; AND-ing that back against the
                // original therefore clears exactly the lowest set bit.
                self.0 &= self.0 - 1;
                Some(square)
            }
            None => None,
        }
    }

    /// Every square on the given file.
    pub const fn file(file: File) -> Self {
        Self(0x0101_0101_0101_0101u64 << file.index())
    }

    /// Every square on the given rank.
    pub const fn rank(rank: Rank) -> Self {
        Self(0xFFu64 << (rank.index() * 8))
    }
    /// Every square in the set, moved one rank towards rank 8.
    /// Squares on rank 8 fall off the board and are dropped.
    pub const fn north(self) -> Self {
        Self(self.0 << 8)
    }

    /// Every square in the set, moved one rank towards rank 1.
    /// Squares on rank 1 fall off the board and are dropped.
    pub const fn south(self) -> Self {
        Self(self.0 >> 8)
    }

    /// Every square in the set, moved one file towards the h-file.
    /// Squares on the h-file fall off the board and are dropped.
    pub const fn east(self) -> Self {
        Self((self.0 & !Self::file(File::H).0) << 1)
    }

    /// Every square in the set, moved one file towards the a-file.
    /// Squares on the a-file fall off the board and are dropped.
    pub const fn west(self) -> Self {
        Self((self.0 & !Self::file(File::A).0) >> 1)
    }

    /// One rank towards rank 8 and one file towards the h-file.
    pub const fn north_east(self) -> Self {
        Self((self.0 & !Self::file(File::H).0) << 9)
    }

    /// One rank towards rank 8 and one file towards the a-file.
    pub const fn north_west(self) -> Self {
        Self((self.0 & !Self::file(File::A).0) << 7)
    }

    /// One rank towards rank 1 and one file towards the h-file.
    pub const fn south_east(self) -> Self {
        Self((self.0 & !Self::file(File::H).0) >> 7)
    }

    /// One rank towards rank 1 and one file towards the a-file.
    pub const fn south_west(self) -> Self {
        Self((self.0 & !Self::file(File::A).0) >> 9)
    }
}

// --- Set algebra ------------------------------------------------------------
//
// Implementing the operator traits lets movegen read as set theory —
// `rook_attacks & !friendly_pieces` — instead of a wall of method calls.
// Each one is the matching `u64` operation, rewrapped.

impl BitAnd for Bitboard {
    type Output = Self;

    /// Intersection: squares in both sets.
    fn bitand(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
}

impl BitOr for Bitboard {
    type Output = Self;

    /// Union: squares in either set.
    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl BitXor for Bitboard {
    type Output = Self;

    /// Symmetric difference: squares in exactly one of the sets.
    fn bitxor(self, other: Self) -> Self {
        Self(self.0 ^ other.0)
    }
}

impl Not for Bitboard {
    type Output = Self;

    /// Complement: every square *not* in the set.
    fn not(self) -> Self {
        Self(!self.0)
    }
}

impl BitAndAssign for Bitboard {
    fn bitand_assign(&mut self, other: Self) {
        self.0 &= other.0;
    }
}

impl BitOrAssign for Bitboard {
    fn bitor_assign(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

impl BitXorAssign for Bitboard {
    fn bitxor_assign(&mut self, other: Self) {
        self.0 ^= other.0;
    }
}

// --- Iteration --------------------------------------------------------------

/// Iterator over the squares of a [`Bitboard`], lowest-numbered first.
///
/// Returned by [`Bitboard::into_iter`]. It is a separate type rather than
/// `Bitboard` implementing [`Iterator`] itself, so that a board does not
/// silently inherit the whole iterator API — in particular [`Iterator::count`],
/// which would shadow the meaning of [`Bitboard::count`].
#[derive(Clone, Debug)]
pub struct Squares(Bitboard);

impl Iterator for Squares {
    type Item = Square;

    fn next(&mut self) -> Option<Square> {
        self.0.pop_lsb()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.0.count() as usize;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for Squares {}

impl IntoIterator for Bitboard {
    type Item = Square;
    type IntoIter = Squares;

    fn into_iter(self) -> Squares {
        Squares(self)
    }
}

impl FromIterator<Square> for Bitboard {
    fn from_iter<I: IntoIterator<Item = Square>>(squares: I) -> Self {
        let mut board = Self::EMPTY;
        for square in squares {
            board.insert(square);
        }
        board
    }
}

// --- Display ----------------------------------------------------------------

impl fmt::Display for Bitboard {
    /// Draws the board as an 8x8 grid, rank 8 on top and the a-file on the
    /// left — the orientation a player sees, not the order the bits sit in.
    ///
    /// Occupied squares print as `X`, empty ones as `.`. There is no trailing
    /// newline, so `println!("{board}")` ends the grid exactly once.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for rank in (0..8u8).rev() {
            if rank != 7 {
                writeln!(f)?;
            }

            for file in 0..8u8 {
                if file != 0 {
                    write!(f, " ")?;
                }

                let square = Square::from_index(rank * 8 + file).expect("rank and file are 0..8");
                write!(f, "{}", if self.contains(square) { 'X' } else { '.' })?;
            }
        }

        Ok(())
    }
}
