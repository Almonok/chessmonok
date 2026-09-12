//! Board representation, move generation and position rules for chessmonok.

mod bitboard;
mod square;

pub use bitboard::{Bitboard, Squares};
pub use square::{File, Rank, Square};
