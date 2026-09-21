//! A chess position: [`CastlingRights`], the [`Position`] container, and FEN.
//!
//! Exercise 02, parts 3 and 4 — see `docs/exercises/02-pieces-and-position.md`.
//! The spec is `tests/position.rs` and `tests/fen.rs`.
//!
//! # The shape of this type
//!
//! A position is stored **twice**, and the redundancy is deliberate.
//!
//! - *Piece-centric*: one [`Bitboard`] per kind and per colour. "Where are all
//!   the white rooks?" is a field access, and "where can this bishop go?" is set
//!   algebra on the boards from exercise 01. This is what makes movegen fast.
//! - *Square-centric*: a 64-entry mailbox, `board[square] = piece`. "What is
//!   standing on e4?" is one index. This is what makes make/unmake fast, and it
//!   is the decisive one: every move made during search must name the piece it
//!   captured, to fill in the undo record and the dirty-piece list the NNUE
//!   accumulator is patched from. That is the hottest question in the engine,
//!   and without a mailbox it is a six-board scan.
//!
//! Two views of one board only work under a discipline: exactly one place is
//! allowed to mutate them, and it updates both together. That place is
//! [`Position::put`] and [`Position::take`], which are private, and
//! `tests/position.rs` checks after every operation that the views still tell
//! the same story.
//!
//! # The one-king invariant
//!
//! A `Position` always has exactly one king per side, enforced where positions
//! are built — which, until make/unmake exists, means [`Position::from_fen`].
//!
//! That is what lets [`Position::king_square`] return a plain [`Square`] rather
//! than an `Option`. Every NNUE feature index is keyed on the king square:
//!
//! ```text
//! halfkp_idx = piece_square + (piece_idx + king_square * 10) * 64
//! ```
//!
//! so a fallible accessor would put an unwrap on that path forever. Same trick
//! as [`Square`]'s `0..64` invariant: validate at the boundary, be total
//! everywhere inside. Note what it costs — there is no way to build a kingless
//! `Position`, and no public board editor, because a caller who could delete a
//! king would turn `king_square` into a panic.

use std::fmt;
use std::str::FromStr;

use crate::{Bitboard, Color, Piece, PieceKind, Square};

/// Which side of the board a castle happens on.
///
/// Named for the piece that starts on that wing, not for the direction — which
/// is why this is an enum and not a `bool`.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum CastlingSide {
    King,
    Queen,
}

/// Who may still castle where: four booleans packed into a `u8`.
///
/// Packed rather than `[[bool; 2]; 2]` because it is part of the Zobrist key —
/// the key table is a `[u64; 16]` indexed by [`CastlingRights::index`], so the
/// four flags have to collapse to one small number.
///
/// The bit layout is yours to choose; the only thing the tests pin down is that
/// the sixteen combinations produce the sixteen distinct indices `0..16`, and
/// that `NONE` is `0`. Deriving it from `color.index()` and the side keeps
/// [`has`](CastlingRights::has) and friends arithmetic rather than a four-arm
/// match.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CastlingRights(u8);

impl CastlingRights {
    /// Nobody may castle.
    pub const NONE: Self = Self(0);

    /// Both sides may castle both ways — the starting position's `KQkq`.
    pub const ALL: Self = Self(0b1111);

    /// Whether this colour may still castle on this side.
    pub const fn has(self, color: Color, side: CastlingSide) -> bool {
        // Hint: one bit test. Work out the bit from `color` and `side` in a
        // private helper, so `with`, `without` and this all agree by
        // construction rather than by three separate hand-written layouts.
        let _ = (color, side);
        todo!()
    }

    /// The same rights, plus this one. Granting a right twice is a no-op.
    pub const fn with(self, color: Color, side: CastlingSide) -> Self {
        let _ = (color, side);
        todo!()
    }

    /// The same rights, minus this one. Revoking a right twice is a no-op.
    pub const fn without(self, color: Color, side: CastlingSide) -> Self {
        let _ = (color, side);
        todo!()
    }

    /// Both of one colour's rights, revoked at once — what a king move does.
    pub const fn without_color(self, color: Color) -> Self {
        let _ = color;
        todo!()
    }

    /// The `0..16` bit pattern, for indexing the Zobrist key table.
    pub const fn index(self) -> usize {
        todo!()
    }
}

/// A chess position: the board, and everything else FEN records about it.
///
/// Every field is private. A `Position` is built by parsing a FEN and, later,
/// mutated by `make_move` — both of which preserve the one-king invariant the
/// module docs describe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Position {
    /// Where each kind of piece stands, both colours together. Six boards and
    /// not twelve because movegen mostly wants *the diagonal sliders* —
    /// `by_kind(Bishop) | by_kind(Queen)` — which is a two-way OR here and a
    /// four-way one with twelve boards. It is also 64 bytes rather than 96:
    /// exactly one cache line, next to a mailbox that is exactly one more.
    pieces: [Bitboard; 6],
    /// Where each colour's pieces stand, all kinds together.
    colors: [Bitboard; 2],
    /// The mailbox: what is standing on each square, or nothing.
    board: [Option<Piece>; 64],
    side_to_move: Color,
    castling: CastlingRights,
    /// The square *behind* a pawn that just double-pushed — where a capturing
    /// pawn would land, not where the pawn itself is.
    en_passant: Option<Square>,
    halfmove_clock: u8,
    fullmove_number: u16,
    /// The Zobrist hash, kept current by [`put`](Position::put) and
    /// [`take`](Position::take) rather than computed on demand: the
    /// transposition table probes it constantly, and a scan over 32 pieces per
    /// probe would throw away the whole saving.
    hash: u64,
}

impl Position {
    /// The position at the start of a game.
    pub fn starting() -> Self {
        todo!()
    }

    /// What is standing on `square`. One array index — see the module docs for
    /// why this is the accessor the whole design bends around.
    pub const fn piece_at(&self, square: Square) -> Option<Piece> {
        let _ = square;
        todo!()
    }

    /// Every piece of this kind, both colours.
    pub const fn by_kind(&self, kind: PieceKind) -> Bitboard {
        let _ = kind;
        todo!()
    }

    /// Every piece of this colour, all kinds.
    pub const fn by_color(&self, color: Color) -> Bitboard {
        let _ = color;
        todo!()
    }

    /// The intersection — e.g. the black knights.
    pub const fn pieces(&self, color: Color, kind: PieceKind) -> Bitboard {
        // Hint: `const fn` still cannot call trait methods, so this is
        // `Bitboard::from_bits(a.bits() & b.bits())` rather than `a & b`,
        // exactly as in exercise 01.
        let _ = (color, kind);
        todo!()
    }

    /// Every occupied square.
    pub const fn occupied(&self) -> Bitboard {
        todo!()
    }

    /// Where this side's king stands.
    ///
    /// Total, not fallible: a `Position` always has exactly one king per side.
    pub fn king_square(&self, color: Color) -> Square {
        // Hint: the king board's `lsb()`. The `expect` is the invariant being
        // cashed in — say so in the message.
        let _ = color;
        todo!()
    }

    /// Whose turn it is.
    pub const fn side_to_move(&self) -> Color {
        todo!()
    }

    /// Who may still castle where.
    pub const fn castling(&self) -> CastlingRights {
        todo!()
    }

    /// The en passant target square, if the last move was a double push.
    pub const fn en_passant(&self) -> Option<Square> {
        todo!()
    }

    /// Plies since the last capture or pawn move, for the fifty-move rule.
    pub const fn halfmove_clock(&self) -> u8 {
        todo!()
    }

    /// The move number, starting at 1 and incrementing after Black moves.
    pub const fn fullmove_number(&self) -> u16 {
        todo!()
    }

    /// The Zobrist hash of this position.
    pub const fn hash(&self) -> u64 {
        todo!()
    }

    /// A board with no pieces on it and everything else at its default.
    ///
    /// Private, and not `Default`: an empty board breaks the one-king
    /// invariant, so it is only ever a starting point for a FEN parse that is
    /// about to fill it in and validate it. (Arrays only implement `Default` up
    /// to length 32, so the mailbox would have to be written by hand anyway.)
    fn empty() -> Self {
        todo!()
    }

    /// Puts `piece` on `square`, which must be empty.
    ///
    /// One of the two functions allowed to touch the board. It must update
    /// **all three** views — the kind board, the colour board and the mailbox —
    /// and XOR the piece's Zobrist key into [`hash`](Position::hash). Keeping
    /// that in exactly two places is what makes the redundancy safe.
    fn put(&mut self, square: Square, piece: Piece) {
        // TODO: `self.hash ^= zobrist::KEYS.piece(piece, square);` once
        // `zobrist.rs` exists (exercise 02, part 2).
        let _ = (square, piece);
        todo!()
    }

    /// Lifts whatever is on `square` off the board, and returns it.
    ///
    /// The other half of the discipline: same three views, and the same XOR —
    /// which removes the key, because XOR is its own inverse.
    fn take(&mut self, square: Square) -> Option<Piece> {
        let _ = square;
        todo!()
    }
}

/// Returned when a string is not a well-formed FEN.
///
/// An enum and not a string because these end up in front of a human, via UCI
/// or a test failure, and "invalid FEN" names nothing. Follows the `thiserror`
/// pattern from [`ParseSquareError`](crate::square), in its multi-variant form.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FenError {
    /// A FEN has exactly six space-separated fields.
    #[error("a FEN has 6 fields, found {found}")]
    WrongFieldCount { found: usize },

    /// The placement field has exactly eight `/`-separated ranks.
    #[error("the placement field has 8 ranks, found {found}")]
    WrongRankCount { found: usize },

    /// Pieces and digits in one rank must add up to exactly eight squares.
    #[error("rank {rank} describes {found} squares, expected 8")]
    RankIsNotEightSquares { rank: u8, found: usize },

    /// Not a FEN piece letter, and not a `1..=8` run length either.
    #[error("`{0}` is not a piece letter or a run length")]
    UnknownPiece(char),

    /// The side to move is `w` or `b`, lowercase.
    #[error("`{0}` is not a side to move (expected `w` or `b`)")]
    BadSideToMove(String),

    /// Castling rights are `-`, or some of `KQkq` with no repeats.
    #[error("`{0}` is not a castling rights field (expected `-` or some of `KQkq`)")]
    BadCastlingRights(String),

    /// The en passant field is `-` or a square in algebraic notation.
    #[error("`{0}` is not an en passant target (expected `-` or e.g. `e3`)")]
    BadEnPassant(String),

    /// The halfmove clock is a non-negative integer.
    #[error("`{0}` is not a halfmove clock")]
    BadHalfmoveClock(String),

    /// The fullmove number is a non-negative integer.
    #[error("`{0}` is not a fullmove number")]
    BadFullmoveNumber(String),

    /// The invariant `king_square` depends on: exactly one king per side.
    #[error("a position has exactly one {color:?} king, found {found}")]
    WrongKingCount { color: Color, found: u32 },
}

impl Position {
    /// Parses a position from Forsyth–Edwards Notation.
    ///
    /// Six space-separated fields: piece placement, side to move, castling
    /// rights, en passant target, halfmove clock, fullmove number.
    ///
    /// ```text
    /// rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
    /// ```
    ///
    /// The placement field reads **rank 8 first**, which is the opposite of our
    /// index order — a1 is 0, so a1 is in the *last* group. Within a rank it is
    /// file a to h, which does match. That reversal is the single most common
    /// bug in a FEN parser; write down which rank you are on before you write
    /// the loop.
    ///
    /// This is the only way into a `Position`, so it is also the only place the
    /// one-king invariant has to be enforced.
    pub fn from_fen(fen: &str) -> Result<Self, FenError> {
        // Hint: `split_whitespace()` for the fields, `split('/')` for the
        // ranks, `chars()` within a rank, `parse::<u8>()` for the clocks — and
        // `?` to get the errors out, which is the first real use of it in the
        // crate.
        let _ = fen;
        todo!()
    }

    /// Writes this position as a FEN.
    ///
    /// The inverse of [`from_fen`](Position::from_fen), which is what makes the
    /// round-trip test meaningful. Runs of empty squares are written as one
    /// digit, never as repeated `1`s.
    #[must_use]
    pub fn to_fen(&self) -> String {
        todo!()
    }
}

impl FromStr for Position {
    type Err = FenError;

    fn from_str(fen: &str) -> Result<Self, Self::Err> {
        Self::from_fen(fen)
    }
}

impl fmt::Display for Position {
    /// Writes the FEN — not a pretty board.
    ///
    /// Tempting to reuse [`Bitboard`]'s 8×8 grid, but `Display` and [`FromStr`]
    /// being inverses is worth more: it makes `position.to_string().parse()` a
    /// meaningful test, and it is what UCI needs. The grid is a stretch goal,
    /// under its own name.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_fen())
    }
}
