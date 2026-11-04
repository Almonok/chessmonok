//! The 64-bit key that names a position in the transposition table.
//!
//! The table needs a cheap, fixed-size name for "this position", so that two
//! move orders reaching the same position (`1.Nf3 Nc6 2.Nc3` and
//! `1.Nc3 Nc6 2.Nf3`) look up the same entry. Comparing whole boards would be
//! far too slow, so the key is a **Zobrist hash**: one random 64-bit number
//! per (piece, square), per castling right, per en-passant file and for "white
//! to move", and the key of a position is the XOR of the numbers for
//! everything true in it.
//!
//! XOR is what makes it cheap. XOR is its own inverse, so a move updates the
//! key with a handful of XORs instead of a rescan of the board:
//!
//! ```text
//! key(after Nf3) = key(before) ^ N@g1 ^ N@f3 ^ white_to_move
//! ```
//!
//! The random numbers are shakmaty's (`Zobrist64`), so the key is the same on
//! every run and every machine.
//!
//! **En passant counts only when it is possible.** After `e2e4` the FEN has an
//! en-passant square, but if no black pawn can take, the position is the same
//! as one without it, and must share its key. `EnPassantMode::Legal` hashes
//! the en-passant file only when a legal en-passant capture exists.
//!
//! The halfmove clock and the move number are not in the key: they do not
//! change which moves are good, except near the 50-move rule, and putting
//! them in would stop almost every transposition from matching.

use shakmaty::zobrist::Zobrist64;
use shakmaty::{Chess, EnPassantMode, Move, Position};

/// The key of `position`, computed from scratch (about 40 XORs).
pub fn position_key(position: &Chess) -> u64 {
    position.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0
}

/// The key of `child`, the position after `played` from `parent`, whose key
/// is `parent_key`.
///
/// shakmaty updates the key incrementally for ordinary moves, and declines
/// (returns `None`) for the moves that touch castling rights or en passant:
/// king and rook moves from their home squares, double pawn pushes, moves out
/// of a position with an en-passant square. Those are rare, so recomputing
/// from scratch for them costs little and keeps the hard cases in one,
/// well-tested place.
pub fn key_after(parent: &Chess, parent_key: u64, played: Move, child: &Chess) -> u64 {
    parent
        .update_zobrist_hash(Zobrist64(parent_key), played, EnPassantMode::Legal)
        .map_or_else(|| position_key(child), |key: Zobrist64| key.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shakmaty::CastlingMode;
    use shakmaty::fen::Fen;

    fn position(fen: &str) -> Chess {
        fen.parse::<Fen>()
            .unwrap()
            .into_position(CastlingMode::Standard)
            .unwrap()
    }

    /// Walk every line `depth` plies deep, checking at each step that the
    /// incremental key equals the key computed from scratch.
    fn check_every_line(parent: &Chess, parent_key: u64, depth: u32) {
        if depth == 0 {
            return;
        }
        for played in parent.legal_moves() {
            let mut child = parent.clone();
            child.play_unchecked(played);
            let incremental = key_after(parent, parent_key, played, &child);
            assert_eq!(
                incremental,
                position_key(&child),
                "after {played} from {}",
                Fen::from_position(parent, EnPassantMode::Legal)
            );
            check_every_line(&child, incremental, depth - 1);
        }
    }

    #[test]
    fn the_incremental_key_matches_the_key_from_scratch() {
        for fen in [
            // Castling on both sides, and pins.
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            // En passant available at once (d5xe6).
            "rnbqkbnr/ppp1p1pp/8/3pPp2/8/8/PPPP1PPP/RNBQKBNR w KQkq f6 0 3",
            // Promotions with and without capture.
            "r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1",
            // Double pushes next to enemy pawns: en passant appears.
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        ] {
            let start = position(fen);
            check_every_line(&start, position_key(&start), 3);
        }
    }

    #[test]
    fn transposed_move_orders_share_a_key() {
        let play = |moves: &[&str]| {
            let mut position = Chess::default();
            for text in moves {
                let played = text
                    .parse::<shakmaty::uci::UciMove>()
                    .unwrap()
                    .to_move(&position)
                    .unwrap();
                position.play_unchecked(played);
            }
            position_key(&position)
        };
        assert_eq!(
            play(&["g1f3", "b8c6", "b1c3"]),
            play(&["b1c3", "b8c6", "g1f3"])
        );
    }

    #[test]
    fn an_impossible_en_passant_does_not_change_the_key() {
        // After 1.e4 no black pawn can take on e3: the same position as the
        // FEN without an en-passant square.
        let with_square = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1";
        let without = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
        assert_eq!(
            position_key(&position(with_square)),
            position_key(&position(without))
        );
    }
}
