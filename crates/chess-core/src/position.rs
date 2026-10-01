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

use crate::{Bitboard, Color, File, Piece, PieceKind, Rank, Square, zobrist};

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

    /// The single bit standing for one colour's right on one side.
    const fn bit(color: Color, side: CastlingSide) -> u8 {
        1 << (color.index() * 2 + side as usize)
    }

    /// Whether this colour may still castle on this side.
    pub const fn has(self, color: Color, side: CastlingSide) -> bool {
        (self.0 & Self::bit(color, side)) != 0
    }

    /// The same rights, plus this one. Granting a right twice is a no-op.
    pub const fn with(self, color: Color, side: CastlingSide) -> Self {
        Self(self.0 | Self::bit(color, side))
    }

    /// The same rights, minus this one. Revoking a right twice is a no-op.
    pub const fn without(self, color: Color, side: CastlingSide) -> Self {
        Self(self.0 & !Self::bit(color, side))
    }

    /// Both of one colour's rights, revoked at once — what a king move does.
    pub const fn without_color(self, color: Color) -> Self {
        let mask = 0b11 << (color.index() * 2);
        Self(self.0 & !mask)
    }

    /// The `0..16` bit pattern, for indexing the Zobrist key table.
    pub const fn index(self) -> usize {
        self.0 as usize
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
        Self::from_fen(STARTING_FEN).expect("the starting FEN is well-formed")
    }

    /// What is standing on `square`. One array index — see the module docs for
    /// why this is the accessor the whole design bends around.
    pub const fn piece_at(&self, square: Square) -> Option<Piece> {
        self.board[square.index() as usize]
    }

    /// Every piece of this kind, both colours.
    pub const fn by_kind(&self, kind: PieceKind) -> Bitboard {
        self.pieces[kind.index()]
    }

    /// Every piece of this colour, all kinds.
    pub const fn by_color(&self, color: Color) -> Bitboard {
        self.colors[color.index()]
    }

    /// The intersection — e.g. the black knights.
    pub const fn pieces(&self, color: Color, kind: PieceKind) -> Bitboard {
        Bitboard::from_bits(self.by_kind(kind).bits() & self.by_color(color).bits())
    }

    /// Every occupied square.
    pub const fn occupied(&self) -> Bitboard {
        Bitboard::from_bits(self.by_color(Color::White).bits() | self.by_color(Color::Black).bits())
    }

    /// Where this side's king stands.
    ///
    /// Total, not fallible: a `Position` always has exactly one king per side.
    pub fn king_square(&self, color: Color) -> Square {
        self.pieces(color, PieceKind::King)
            .lsb()
            .expect("a Position always has exactly one king per side")
    }

    /// Whose turn it is.
    pub const fn side_to_move(&self) -> Color {
        self.side_to_move
    }

    /// Who may still castle where.
    pub const fn castling(&self) -> CastlingRights {
        self.castling
    }

    /// The en passant target square, if the last move was a double push.
    pub const fn en_passant(&self) -> Option<Square> {
        self.en_passant
    }

    /// Plies since the last capture or pawn move, for the fifty-move rule.
    pub const fn halfmove_clock(&self) -> u8 {
        self.halfmove_clock
    }

    /// The move number, starting at 1 and incrementing after Black moves.
    pub const fn fullmove_number(&self) -> u16 {
        self.fullmove_number
    }

    /// The Zobrist hash of this position.
    pub const fn hash(&self) -> u64 {
        self.hash
    }

    /// A board with no pieces on it and everything else at its default.
    ///
    /// Private, and not `Default`: an empty board breaks the one-king
    /// invariant, so it is only ever a starting point for a FEN parse that is
    /// about to fill it in and validate it. (Arrays only implement `Default` up
    /// to length 32, so the mailbox would have to be written by hand anyway.)
    fn empty() -> Self {
        Self {
            pieces: [Bitboard::EMPTY; 6],
            colors: [Bitboard::EMPTY; 2],
            board: [None; 64],
            side_to_move: Color::White,
            castling: CastlingRights::NONE,
            en_passant: None,
            halfmove_clock: 0,
            fullmove_number: 1,
            // No pieces, White to move, no en passant: the only term left is
            // the castling key, which is XORed in even for `NONE`.
            hash: zobrist::KEYS.castling(CastlingRights::NONE),
        }
    }

    /// Puts `piece` on `square`, which must be empty.
    ///
    /// One of the two functions allowed to touch the board. It must update
    /// **all three** views — the kind board, the colour board and the mailbox —
    /// and XOR the piece's Zobrist key into [`hash`](Position::hash). Keeping
    /// that in exactly two places is what makes the redundancy safe.
    fn put(&mut self, square: Square, piece: Piece) {
        debug_assert!(
            self.piece_at(square).is_none(),
            "put onto an occupied square."
        );
        self.pieces[piece.kind().index()].insert(square);
        self.colors[piece.color().index()].insert(square);
        self.board[square.index() as usize] = Some(piece);
        self.hash ^= zobrist::KEYS.piece(piece, square);
    }

    /// Lifts whatever is on `square` off the board, and returns it.
    ///
    /// The other half of the discipline: same three views, and the same XOR —
    /// which removes the key, because XOR is its own inverse.
    #[expect(dead_code, reason = "called by make/unmake")]
    fn take(&mut self, square: Square) -> Option<Piece> {
        let taken = self.board[square.index() as usize].take()?;
        self.pieces[taken.kind().index()].remove(square);
        self.colors[taken.color().index()].remove(square);
        self.hash ^= zobrist::KEYS.piece(taken, square);
        Some(taken)
    }
}

/// Returned when a string is not a well-formed FEN.
///
/// An enum and not a string because these end up in front of a human, via UCI
/// or a test failure, and "invalid FEN" names nothing.
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

const STARTING_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// The castling letters in the order FEN writes them.
const CASTLING_LETTERS: [(char, Color, CastlingSide); 4] = [
    ('K', Color::White, CastlingSide::King),
    ('Q', Color::White, CastlingSide::Queen),
    ('k', Color::Black, CastlingSide::King),
    ('q', Color::Black, CastlingSide::Queen),
];

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
        let fields: Vec<&str> = fen.split_whitespace().collect();
        let [placement, side, castling, en_passant, halfmove, fullmove] = fields[..] else {
            return Err(FenError::WrongFieldCount {
                found: fields.len(),
            });
        };

        let mut position = Self::empty();
        position.parse_placement(placement)?;
        position.parse_side_to_move(side)?;
        position.parse_castling(castling)?;
        position.parse_en_passant(en_passant)?;
        position.halfmove_clock = halfmove
            .parse()
            .map_err(|_| FenError::BadHalfmoveClock(halfmove.to_string()))?;
        position.fullmove_number = fullmove
            .parse()
            .map_err(|_| FenError::BadFullmoveNumber(fullmove.to_string()))?;

        for color in [Color::White, Color::Black] {
            let found = position.pieces(color, PieceKind::King).count();
            if found != 1 {
                return Err(FenError::WrongKingCount { color, found });
            }
        }
        Ok(position)
    }

    fn parse_placement(&mut self, placement: &str) -> Result<(), FenError> {
        let ranks: Vec<&str> = placement.split('/').collect();
        if ranks.len() != 8 {
            return Err(FenError::WrongRankCount { found: ranks.len() });
        }

        for (i, text) in ranks.iter().enumerate() {
            // FEN lists rank 8 first.
            let rank_index = 7 - i as u8;
            let rank = Rank::from_index(rank_index).expect("there are 8 ranks");
            let overflow = |found| FenError::RankIsNotEightSquares {
                rank: rank_index + 1,
                found,
            };

            let mut file: usize = 0;
            for c in text.chars() {
                if let Some(n) = c.to_digit(10) {
                    if !(1..=8).contains(&n) {
                        return Err(FenError::UnknownPiece(c));
                    }
                    file += n as usize;
                    if file > 8 {
                        return Err(overflow(file));
                    }
                } else {
                    let piece = Piece::from_char(c).ok_or(FenError::UnknownPiece(c))?;
                    if file >= 8 {
                        return Err(overflow(file + 1));
                    }
                    let square =
                        Square::new(File::from_index(file as u8).expect("checked above"), rank);
                    self.put(square, piece);
                    file += 1;
                }
            }
            if file != 8 {
                return Err(overflow(file));
            }
        }
        Ok(())
    }

    fn parse_side_to_move(&mut self, side: &str) -> Result<(), FenError> {
        self.side_to_move = match side {
            "w" => Color::White,
            "b" => Color::Black,
            _ => return Err(FenError::BadSideToMove(side.to_string())),
        };
        if self.side_to_move == Color::Black {
            self.hash ^= zobrist::KEYS.black_to_move();
        }
        Ok(())
    }

    fn parse_castling(&mut self, castling: &str) -> Result<(), FenError> {
        let bad = || FenError::BadCastlingRights(castling.to_string());

        let mut rights = CastlingRights::NONE;
        if castling != "-" {
            for c in castling.chars() {
                let &(_, color, side) = CASTLING_LETTERS
                    .iter()
                    .find(|(letter, _, _)| *letter == c)
                    .ok_or_else(bad)?;
                if rights.has(color, side) {
                    return Err(bad());
                }
                rights = rights.with(color, side);
            }
        }

        self.hash ^= zobrist::KEYS.castling(self.castling);
        self.castling = rights;
        self.hash ^= zobrist::KEYS.castling(self.castling);
        Ok(())
    }

    fn parse_en_passant(&mut self, en_passant: &str) -> Result<(), FenError> {
        if en_passant == "-" {
            return Ok(());
        }
        let square: Square = en_passant
            .parse()
            .map_err(|_| FenError::BadEnPassant(en_passant.to_string()))?;
        self.en_passant = Some(square);
        self.hash ^= zobrist::KEYS.en_passant(square.file());
        Ok(())
    }

    /// Writes this position as a FEN.
    ///
    /// The inverse of [`from_fen`](Position::from_fen), which is what makes the
    /// round-trip test meaningful. Runs of empty squares are written as one
    /// digit, never as repeated `1`s.
    #[must_use]
    pub fn to_fen(&self) -> String {
        let mut fen = String::new();

        for rank_index in (0..8u8).rev() {
            let rank = Rank::from_index(rank_index).expect("rank index in 0..8");
            let mut empty = 0;
            for file_index in 0..8u8 {
                let file = File::from_index(file_index).expect("file index in 0..8");
                match self.piece_at(Square::new(file, rank)) {
                    Some(piece) => {
                        if empty > 0 {
                            fen.push(char::from_digit(empty, 10).expect("at most 8"));
                            empty = 0;
                        }
                        fen.push(piece.to_char());
                    }
                    None => empty += 1,
                }
            }
            if empty > 0 {
                fen.push(char::from_digit(empty, 10).expect("at most 8"));
            }
            if rank_index > 0 {
                fen.push('/');
            }
        }

        let side = match self.side_to_move {
            Color::White => 'w',
            Color::Black => 'b',
        };

        let mut castling: String = CASTLING_LETTERS
            .iter()
            .filter(|&&(_, color, side)| self.castling.has(color, side))
            .map(|&(letter, _, _)| letter)
            .collect();
        if castling.is_empty() {
            castling.push('-');
        }

        let en_passant = self
            .en_passant
            .map_or_else(|| "-".to_string(), |square| square.to_string());

        format!(
            "{fen} {side} {castling} {en_passant} {} {}",
            self.halfmove_clock, self.fullmove_number
        )
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
