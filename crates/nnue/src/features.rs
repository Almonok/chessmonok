//! `HalfKP` feature indexing: the Rust twin of almoNnuet's `features.py`
//! (training spec §4). Both must agree bit for bit; `tests/verify.rs` checks
//! it against Python's own output.

use shakmaty::{Board, Color, Piece, Role, Square};

/// 10 piece classes x 64 squares + 1 unused slot.
pub const KING_STRIDE: usize = 641;

/// Black sees the board flipped vertically.
pub fn orient(square: Square, perspective: Color) -> usize {
    match perspective {
        Color::White => square.to_usize(),
        Color::Black => square.to_usize() ^ 0b11_1000, // flip the three rank bits
    }
}

/// 0..=9: twice the piece type, plus one for the enemy's pieces. Kings have
/// no feature.
pub fn piece_index(piece: Piece, perspective: Color) -> usize {
    assert_ne!(piece.role, Role::King, "kings have no feature");
    2 * (piece.role as usize - 1) + usize::from(piece.color != perspective)
}

/// The feature `perspective` sees for `piece` on `square`, its own king on `king`.
pub fn feature_index(piece: Piece, square: Square, king: Square, perspective: Color) -> usize {
    orient(king, perspective) * KING_STRIDE
        + piece_index(piece, perspective) * 64
        + orient(square, perspective)
        + 1
}

/// Every feature `perspective` sees on `board`, sorted ascending.
pub fn active_features(board: &Board, perspective: Color) -> Vec<usize> {
    let king = board
        .king_of(perspective)
        .expect("a legal position has both kings");
    let mut features: Vec<usize> = board
        .iter()
        .filter(|(_, piece)| piece.role != Role::King)
        .map(|(square, piece)| feature_index(piece, square, king, perspective))
        .collect();
    features.sort_unstable();
    features
}
