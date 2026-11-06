//! Null-move pruning: cut a node when even passing keeps us at beta.
//!
//! The side to move passes, and the opponent's two-moves-in-a-row position is
//! searched `REDUCTION` plies shallower with a zero window at beta. If we
//! still reach beta, the node is cut without searching any real move: having
//! the move is almost always worth something, so a real move would reach beta
//! too.
//!
//! That assumption fails in zugzwang, and a pass is meaningless in some
//! positions, hence the guards (checked in `Searcher::null_move_cutoff`): no
//! pass in check, right after a pass, with only pawns and the king left, at a
//! mate-score beta, or when the static evaluation is already below beta.

use shakmaty::{Chess, Position};

/// The shallowest depth at which a pass is tried. Below it the passed position
/// would be searched by quiescence alone: too little is saved to pay for the
/// extra evaluation and search.
pub const MINIMUM_DEPTH: u32 = 3;

/// How many plies shallower than a real move the passed position is searched.
/// Three is the common choice today; a depth-dependent reduction is a tuning
/// step for later, measured on its own.
const REDUCTION: u32 = 3;

/// The depth at which the passed position is searched: one ply for the pass,
/// plus the reduction. Saturating, so a shallow node falls into quiescence.
pub fn reduced_depth(depth: u32) -> u32 {
    depth.saturating_sub(1 + REDUCTION)
}

/// Whether the side to move has a piece other than pawns and its king: the
/// zugzwang guard. Zugzwang is rare once a piece is left to make a harmless
/// waiting move, and common in king-and-pawn endgames.
pub fn has_pieces_besides_pawns(position: &Chess) -> bool {
    let board = position.board();
    let pawns_and_kings = board.pawns() | board.kings();
    (board.by_color(position.turn()) & !pawns_and_kings).any()
}

/// The position after the side to move passes: the other side to move, and
/// any en-passant right gone (it lasts one move only).
///
/// `None` when passing is illegal, which is when the side to move is in check;
/// the search never asks then.
pub fn pass(position: &Chess) -> Option<Chess> {
    position.clone().swap_turn().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position_key;
    use shakmaty::fen::Fen;
    use shakmaty::{CastlingMode, Color};

    fn position(fen: &str) -> Chess {
        fen.parse::<Fen>()
            .unwrap()
            .into_position(CastlingMode::Standard)
            .unwrap()
    }

    #[test]
    fn pawns_and_a_king_are_not_enough_to_pass() {
        assert!(!has_pieces_besides_pawns(&position(
            "8/pp3k2/2p5/8/8/2P5/PP3K2/8 w - - 0 1"
        )));
        assert!(!has_pieces_besides_pawns(&position(
            "4k3/8/8/8/8/8/8/4K3 w - - 0 1"
        )));
    }

    #[test]
    fn one_piece_is_enough_to_pass() {
        assert!(has_pieces_besides_pawns(&position(
            "4k3/8/8/8/8/8/4P3/4K1N1 w - - 0 1"
        )));
    }

    #[test]
    fn only_the_side_to_move_counts() {
        // White has a knight, black only a pawn: black, to move, may not pass.
        assert!(!has_pieces_besides_pawns(&position(
            "4k3/4p3/8/8/8/8/8/4K1N1 b - - 0 1"
        )));
    }

    #[test]
    fn a_pass_flips_the_turn_and_drops_en_passant() {
        // White to move could take e5xf6 en passant; after a pass that right
        // is gone, and the key is that of the same board with black to move.
        let before = position("rnbqkbnr/ppp1p1pp/8/3pPp2/8/8/PPPP1PPP/RNBQKBNR w KQkq f6 0 3");
        let after = pass(&before).unwrap();
        assert_eq!(after.turn(), Color::Black);
        assert!(after.legal_ep_square().is_none());
        assert_eq!(after.board(), before.board());
        assert_eq!(
            position_key(&after),
            position_key(&position(
                "rnbqkbnr/ppp1p1pp/8/3pPp2/8/8/PPPP1PPP/RNBQKBNR b KQkq - 0 3"
            ))
        );
    }

    #[test]
    fn no_pass_out_of_check() {
        assert!(pass(&position("4k3/8/8/8/8/8/8/r3K3 w - - 0 1")).is_none());
    }

    #[test]
    fn the_reduced_depth_saturates_at_quiescence() {
        assert_eq!(reduced_depth(MINIMUM_DEPTH), 0);
        assert_eq!(reduced_depth(1), 0);
        assert_eq!(reduced_depth(8), 4);
    }
}
