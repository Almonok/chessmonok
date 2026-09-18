//! Who owns a piece ([`Color`]), what it does ([`PieceKind`]), and the
//! [`Piece`] that pairs them.
//!
//! All three are `#[must_use]` for the same reason [`Bitboard`] is: every
//! operation on them is a pure conversion, so computing one and discarding it
//! is always dead code.
//!
//! [`Bitboard`]: crate::Bitboard

/// Which player a piece belongs to.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Color {
    White,
    Black,
}

impl Color {
    /// The colour's `0..2` offset, for indexing colour-keyed arrays.
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The other colour.
    pub const fn flip(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }
}

impl std::ops::Not for Color {
    type Output = Self;

    /// The other colour, so that movegen can say `!side_to_move`.
    fn not(self) -> Self {
        self.flip()
    }
}

/// What a piece does, independent of whose it is.
///
/// Ordered pawn-to-king because that order is baked into [`Piece`]'s layout and
/// into the Zobrist key table; reordering these variants renumbers both.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PieceKind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

impl PieceKind {
    /// The kind's `0..6` offset, for indexing kind-keyed arrays.
    pub const fn index(self) -> usize {
        // Same trick as `Color::index`.
        self as usize
    }

    /// Builds a kind from its offset, or `None` if `index >= 6`.
    pub const fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::Pawn),
            1 => Some(Self::Knight),
            2 => Some(Self::Bishop),
            3 => Some(Self::Rook),
            4 => Some(Self::Queen),
            5 => Some(Self::King),
            _ => None,
        }
    }

    /// The lowercase FEN letter. Knight is `n`, because `k` is the king's.
    pub const fn to_char(self) -> char {
        match self {
            Self::Pawn => 'p',
            Self::Knight => 'n',
            Self::Bishop => 'b',
            Self::Rook => 'r',
            Self::Queen => 'q',
            Self::King => 'k',
        }
    }
}

/// A piece, with its colour.
///
/// # Layout
///
/// Twelve variants, deliberately: the six White pieces in [`PieceKind`] order,
/// then the six Black ones, so that `index() == color.index() * 6 + kind.index()`.
///
/// That buys two things. [`Piece::color`] and [`Piece::kind`] become a division
/// and a remainder instead of a twelve-arm match. And because a `#[repr(u8)]`
/// enum with twelve variants only ever holds `0..12`, the compiler can use one
/// of the 244 unused bit patterns as the `None` of an `Option<Piece>` — a
/// *niche* — so `Option<Piece>` is one byte and the 64-square mailbox in
/// `Position` is exactly one cache line.
///
/// A `{ color, kind }` struct would cost three bytes per square instead of one.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Piece {
    WhitePawn,
    WhiteKnight,
    WhiteBishop,
    WhiteRook,
    WhiteQueen,
    WhiteKing,
    BlackPawn,
    BlackKnight,
    BlackBishop,
    BlackRook,
    BlackQueen,
    BlackKing,
}

impl Piece {
    /// The piece of this colour and kind.
    pub const fn new(color: Color, kind: PieceKind) -> Self {
        // Compute the index the layout promises, then turn it back into a
        // variant with the private helper below.
        match Self::from_index(color.index() * 6 + kind.index()) {
            Some(piece) => piece,
            // Unreachable by the layout: `color < 2` and `kind < 6`, so the sum is `< 12`.
            None => unreachable!(),
        }
    }

    /// The piece's `0..12` offset.
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Whose piece this is.
    pub const fn color(self) -> Color {
        // The first six indices are White's. No need for a `Color::from_index`.
        if self.index() < 6 {
            Color::White
        } else {
            Color::Black
        }
    }

    /// What this piece does.
    pub const fn kind(self) -> PieceKind {
        // `PieceKind::from_index` returns an `Option`, but the layout guarantees
        // the remainder is in range — so this is the `match ... None =>
        // unreachable!()` pattern from `Square::file` in `square.rs`.
        match PieceKind::from_index(self.index() % 6) {
            Some(kind) => kind,
            // Unreachable by the layout: anything `% 6` is `< 6`.
            None => unreachable!(),
        }
    }

    /// The FEN letter: uppercase for White, lowercase for Black.
    pub const fn to_char(self) -> char {
        // The kind already knows its lowercase letter. White's is the same
        // letter, uppercased — and `u8` arithmetic on ASCII is const-friendly
        // where `char::to_ascii_uppercase` is not.
        let lower = self.kind().to_char();
        match self.color() {
            Color::White => (lower as u8 - b'a' + b'A') as char,
            Color::Black => lower,
        }
    }

    /// Reads a piece from its FEN letter, or `None` if it is not one.
    pub const fn from_char(letter: char) -> Option<Self> {
        // Case picks the colour, the letter picks the kind. Rejecting anything
        // else is the whole job — digits and `/` are FEN structure, not pieces.
        let (color, lower) = match letter {
            'A'..='Z' => (Color::White, (letter as u8 - b'A' + b'a') as char),
            'a'..='z' => (Color::Black, letter),
            _ => return None,
        };
        let kind = match lower {
            'p' => PieceKind::Pawn,
            'n' => PieceKind::Knight,
            'b' => PieceKind::Bishop,
            'r' => PieceKind::Rook,
            'q' => PieceKind::Queen,
            'k' => PieceKind::King,
            _ => return None,
        };
        Some(Self::new(color, kind))
    }

    /// Builds a piece from its `0..12` offset, or `None` if out of range.
    ///
    /// Private: the offset is a layout detail, and callers who want a specific
    /// piece should say so with [`Piece::new`].
    const fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::WhitePawn),
            1 => Some(Self::WhiteKnight),
            2 => Some(Self::WhiteBishop),
            3 => Some(Self::WhiteRook),
            4 => Some(Self::WhiteQueen),
            5 => Some(Self::WhiteKing),
            6 => Some(Self::BlackPawn),
            7 => Some(Self::BlackKnight),
            8 => Some(Self::BlackBishop),
            9 => Some(Self::BlackRook),
            10 => Some(Self::BlackQueen),
            11 => Some(Self::BlackKing),
            _ => None,
        }
    }
}
