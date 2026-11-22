//! The refresh cache ("Finny table"): cheap accumulator rebuilds after a
//! king move.
//!
//! In `HalfKP` every feature is "this piece on this square, *with my king on
//! that square*". When a king moves, every feature of its side changes, and
//! that half of the accumulator is rebuilt: the bias plus one row per piece
//! on the board, about thirty rows. On `bench` those rebuilds were 8 % of the
//! search time.
//!
//! But the search keeps putting kings back on the same few squares, and
//! the board around them has changed little since. So the cache keeps, for
//! each perspective and each square of its king, the last half computed
//! there and the board it was computed for. A rebuild starts from that half
//! and applies only the differences between the two boards:
//!
//! ```text
//!   white king steps to g1; the cache's (White, g1) entry was made
//!   when the board differed by a knight (f3 -> e5) and a pawn taken on d5
//!
//!   full rebuild:   bias + ~30 rows
//!   from the cache: entry - row(N f3) + row(N e5) - row(p d5)    3 rows
//! ```
//!
//! An entry always holds the exact half for its board, so it is correct for
//! any later position: the differences make it so, however many there are.
//! Each searcher owns its cache (it is written on every king move), so
//! several search threads never share one.

use shakmaty::{Board, Color, Piece, Square};

use crate::accumulator::{add_row, subtract_row};
use crate::features::feature_index;
use crate::{L1, Network};

/// One perspective's half for one square of its king, and the board it is
/// exact for.
#[derive(Clone, Debug)]
struct CacheEntry {
    half: [i16; L1],
    board: Board,
}

/// One entry per perspective and square of that perspective's king.
#[derive(Clone, Debug)]
pub struct RefreshCache {
    entries: Box<[CacheEntry]>,
}

/// Every piece that has features: kings have none.
const PIECES_WITH_FEATURES: [Piece; 10] = [
    Color::White.pawn(),
    Color::White.knight(),
    Color::White.bishop(),
    Color::White.rook(),
    Color::White.queen(),
    Color::Black.pawn(),
    Color::Black.knight(),
    Color::Black.bishop(),
    Color::Black.rook(),
    Color::Black.queen(),
];

impl RefreshCache {
    /// Every entry starts as the empty board, whose half is just the bias:
    /// exact, like every entry after it.
    pub fn new(net: &Network) -> Self {
        let empty = CacheEntry {
            half: net.ft_bias,
            board: Board::empty(),
        };
        Self {
            entries: vec![empty; 2 * 64].into_boxed_slice(),
        }
    }

    /// Write into `half` the `perspective` half for `board`, from the entry
    /// of its king's square, which is brought up to `board` on the way.
    pub fn refresh_half(
        &mut self,
        net: &Network,
        board: &Board,
        perspective: Color,
        half: &mut [i16; L1],
    ) {
        let king = board
            .king_of(perspective)
            .expect("a legal position has both kings");
        let entry = &mut self.entries[perspective as usize * 64 + usize::from(king)];
        for piece in PIECES_WITH_FEATURES {
            let before = entry.board.by_piece(piece);
            let now = board.by_piece(piece);
            let row = |square: Square| net.ft_row(feature_index(piece, square, king, perspective));
            for gone in before & !now {
                subtract_row(&mut entry.half, row(gone));
            }
            for arrived in now & !before {
                add_row(&mut entry.half, row(arrived));
            }
        }
        entry.board = board.clone();
        *half = entry.half;
    }
}
