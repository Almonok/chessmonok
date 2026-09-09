//! Board coordinates: [`File`], [`Rank`] and the [`Square`] they combine into.
//!
//! Squares are numbered `a1 = 0` through `h8 = 63`, rank-major, so that
//! `index = rank * 8 + file`. Every bitboard in the crate uses this numbering:
//! bit `n` of a `u64` refers to square `n`.

use std::fmt;
use std::str::FromStr;

/// A board column, `a` through `h`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum File {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
}

impl File {
    /// The file's offset from the a-file, `0..8`.
    pub const fn index(self) -> u8 {
        self as u8
    }

    /// Builds a file from its offset, or `None` if `index >= 8`.
    pub const fn from_index(index: u8) -> Option<Self> {
        match index {
            0 => Some(Self::A),
            1 => Some(Self::B),
            2 => Some(Self::C),
            3 => Some(Self::D),
            4 => Some(Self::E),
            5 => Some(Self::F),
            6 => Some(Self::G),
            7 => Some(Self::H),
            _ => None,
        }
    }

    /// The file's letter as it appears in algebraic notation.
    pub const fn to_char(self) -> char {
        (b'a' + self as u8) as char
    }

    /// Reads a file from its algebraic letter. Uppercase is rejected.
    pub const fn from_char(letter: char) -> Option<Self> {
        match letter {
            'a'..='h' => Self::from_index(letter as u8 - b'a'),
            _ => None,
        }
    }
}

/// A board row, rank 1 through rank 8.
///
/// Ranks are spelled out rather than numbered so that call sites read as
/// `Rank::Fourth` instead of `Rank::new(3)`, keeping the off-by-one between
/// "rank 4" and "index 3" inside this module.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Rank {
    First,
    Second,
    Third,
    Fourth,
    Fifth,
    Sixth,
    Seventh,
    Eighth,
}

impl Rank {
    /// The rank's offset from rank 1, `0..8`.
    pub const fn index(self) -> u8 {
        self as u8
    }

    /// Builds a rank from its offset, or `None` if `index >= 8`.
    pub const fn from_index(index: u8) -> Option<Self> {
        match index {
            0 => Some(Self::First),
            1 => Some(Self::Second),
            2 => Some(Self::Third),
            3 => Some(Self::Fourth),
            4 => Some(Self::Fifth),
            5 => Some(Self::Sixth),
            6 => Some(Self::Seventh),
            7 => Some(Self::Eighth),
            _ => None,
        }
    }

    /// The rank's digit as it appears in algebraic notation.
    pub const fn to_char(self) -> char {
        (b'1' + self as u8) as char
    }

    /// Reads a rank from its algebraic digit.
    pub const fn from_char(digit: char) -> Option<Self> {
        match digit {
            '1'..='8' => Self::from_index(digit as u8 - b'1'),
            _ => None,
        }
    }
}

/// One of the 64 squares of the board.
///
/// # Invariant
///
/// The wrapped index is always in `0..64`. Every constructor upholds it, and
/// the field is private so no caller can break it — which is what makes
/// [`Square::file`] and [`Square::rank`] total functions rather than fallible
/// ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Square(u8);

impl Square {
    /// The a1 square, index 0 — the origin of the numbering scheme.
    pub const A1: Self = Self(0);
    /// The h8 square, index 63 — the last square.
    pub const H8: Self = Self(63);

    /// Combines a file and a rank into the square where they meet.
    pub const fn new(file: File, rank: Rank) -> Self {
        Self(rank as u8 * 8 + file as u8)
    }

    /// Builds a square from its `0..64` index, or `None` if out of range.
    pub const fn from_index(index: u8) -> Option<Self> {
        if index < 64 { Some(Self(index)) } else { None }
    }

    /// The square's `0..64` index.
    pub const fn index(self) -> u8 {
        self.0
    }

    /// The file this square sits on.
    pub const fn file(self) -> File {
        match File::from_index(self.0 % 8) {
            Some(file) => file,
            // Unreachable by the type invariant: `self.0 < 64`, so `self.0 % 8 < 8`.
            None => unreachable!(),
        }
    }

    /// The rank this square sits on.
    pub const fn rank(self) -> Rank {
        match Rank::from_index(self.0 / 8) {
            Some(rank) => rank,
            // Unreachable by the type invariant: `self.0 < 64`, so `self.0 / 8 < 8`.
            None => unreachable!(),
        }
    }
}

impl fmt::Display for Square {
    /// Writes the square in algebraic notation, e.g. `e4`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.file().to_char(), self.rank().to_char())
    }
}

/// Returned when a string is not a square in algebraic notation.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("`{0}` is not a square in algebraic notation (expected e.g. `e4`)")]
pub struct ParseSquareError(pub String);

impl FromStr for Square {
    type Err = ParseSquareError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut chars = text.chars();

        // Exactly two characters: a file letter, a rank digit, then nothing.
        // The trailing `None` is what rejects "e44".
        let coordinates = match (chars.next(), chars.next(), chars.next()) {
            (Some(file), Some(rank), None) => File::from_char(file).zip(Rank::from_char(rank)),
            _ => None,
        };

        coordinates
            .map(|(file, rank)| Self::new(file, rank))
            .ok_or_else(|| ParseSquareError(text.to_owned()))
    }
}
