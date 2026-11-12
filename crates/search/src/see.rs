//! Static exchange evaluation (SEE): what a capture wins once every
//! recapture on its square has been played out.
//!
//! MVV-LVA orders a capture by its victim and its attacker alone, so a queen
//! taking a pawn defended by a pawn looks like a fine capture: it wins a
//! pawn. It also loses the queen. SEE plays the exchange out on the target
//! square, each side recapturing with its least valuable piece, without
//! searching any other move:
//!
//! ```text
//! Qxd5, the d5 pawn defended by the e6 pawn; a white knight also hits d5
//!
//!   capture   balance for White
//!   Qxd5      +100    a pawn won
//!   exd5      -800    the queen lost
//!   Nxd5      -700    the pawn won back; Black has nothing left
//! ```
//!
//! Neither side has to recapture: each may stop when going on would leave it
//! worse off. So the list is resolved backwards, each side picking the better
//! of stopping and capturing. White would rather play Nxd5 (-700) than stop
//! at -800; Black would rather have -700 than let White keep +100. Qxd5 is
//! worth -700: a losing capture.
//!
//! Recaptures by pieces lined up behind one another count (a rook behind a
//! rook on the same file joins in once the first has captured): the attacks
//! are recomputed after every capture with the captured-from square emptied,
//! which uncovers them. Pins and checks are ignored, as in every engine's
//! SEE: it is a cheap estimate for ordering and pruning, not a search.

use shakmaty::{Bitboard, Board, Chess, Color, Move, Position, Role, Square};

/// The most captures one square can see: every piece but the two kings,
/// plus the first capture. A fixed array on the stack, sized for the worst
/// case, saves allocating a `Vec` for each of the many captures SEE scores.
const LONGEST_EXCHANGE: usize = 32;

/// Piece values for exchanges, in centipawns. The king's is far above all
/// the material on the board, so an exchange never trades it for anything.
fn value(role: Role) -> i32 {
    match role {
        Role::Pawn => 100,
        Role::Knight => 320,
        Role::Bishop => 330,
        Role::Rook => 500,
        Role::Queen => 900,
        Role::King => 20_000,
    }
}

/// The material `chess_move` wins for the side playing it, in centipawns,
/// once the exchange it starts on its target square is played out: 0 or more
/// when the move is safe, negative when it loses material.
///
/// Meant for captures and promotions. A castling move changes no material
/// and scores 0.
pub(crate) fn static_exchange(position: &Chess, chess_move: Move) -> i32 {
    if matches!(chess_move, Move::Castle { .. }) {
        return 0;
    }
    let Some(from) = chess_move.from() else {
        return 0;
    };
    let board = position.board();
    let target = chess_move.to();
    let mut occupied = board.occupied().without(from);
    if matches!(chess_move, Move::EnPassant { .. }) {
        // The pawn taken en passant stands beside the target square, not on it.
        occupied.discard(Square::from_coords(target.file(), from.rank()));
    }

    let mut gains = [0; LONGEST_EXCHANGE];
    gains[0] = first_gain(chess_move);
    let mut depth = 0;
    let mut on_target = chess_move.promotion().unwrap_or(chess_move.role());
    let mut side = !position.turn();
    let mut attackers = attackers_of(board, target, occupied);
    while let Some((square, role)) =
        least_valuable_attacker(board, attackers & board.by_color(side))
    {
        occupied.discard(square);
        attackers = attackers_of(board, target, occupied);
        // A king may recapture only on a square nothing attacks any more.
        if role == Role::King && (attackers & board.by_color(!side)).any() {
            break;
        }
        depth += 1;
        gains[depth] = value(on_target) - gains[depth - 1];
        on_target = role;
        side = !side;
    }
    resolve_backwards(&mut gains[..=depth])
}

/// What the move itself wins before any recapture: the captured piece, and
/// for a promotion the promoted piece in place of the pawn.
fn first_gain(chess_move: Move) -> i32 {
    let captured = chess_move.capture().map_or(0, value);
    let promoted = chess_move
        .promotion()
        .map_or(0, |promoted| value(promoted) - value(Role::Pawn));
    captured + promoted
}

/// Every piece of either colour still on the board that attacks `target`,
/// seen through the `occupied` squares only.
fn attackers_of(board: &Board, target: Square, occupied: Bitboard) -> Bitboard {
    (board.attacks_to(target, Color::White, occupied)
        | board.attacks_to(target, Color::Black, occupied))
        & occupied
}

/// The cheapest piece among `attackers`, with its square.
fn least_valuable_attacker(board: &Board, attackers: Bitboard) -> Option<(Square, Role)> {
    Role::ALL.into_iter().find_map(|role| {
        (attackers & board.by_role(role))
            .first()
            .map(|square| (square, role))
    })
}

/// `gains[i]` is what the side making capture `i` has won once it is made,
/// if nothing follows. Walking back from the last capture, the side to play
/// at each step keeps the better of stopping (`-gains[i - 1]` for it) and
/// going on (`gains[i]`): what is left in `gains[0]` is the exchange's value.
fn resolve_backwards(gains: &mut [i32]) -> i32 {
    for index in (1..gains.len()).rev() {
        gains[index - 1] = -((-gains[index - 1]).max(gains[index]));
    }
    gains[0]
}

#[cfg(test)]
mod tests {
    use shakmaty::fen::Fen;
    use shakmaty::uci::UciMove;
    use shakmaty::{CastlingMode, Chess};

    use super::*;

    fn exchange(fen: &str, uci: &str) -> i32 {
        let position: Chess = fen
            .parse::<Fen>()
            .unwrap()
            .into_position(CastlingMode::Standard)
            .unwrap();
        let chess_move = uci.parse::<UciMove>().unwrap().to_move(&position).unwrap();
        static_exchange(&position, chess_move)
    }

    #[test]
    fn an_undefended_piece_is_won_whole() {
        assert_eq!(exchange("4k3/8/8/3n4/4P3/8/8/4K3 w - - 0 1", "e4d5"), 320);
    }

    #[test]
    fn a_queen_taking_a_defended_pawn_loses_the_queen() {
        assert_eq!(exchange("4k3/8/4p3/3p4/8/8/8/3QK3 w - - 0 1", "d1d5"), -800);
    }

    #[test]
    fn the_module_example_is_worth_minus_seven_hundred() {
        assert_eq!(
            exchange("4k3/8/4p3/3p4/8/2N5/8/3QK3 w - - 0 1", "d1d5"),
            -700
        );
    }

    #[test]
    fn a_rook_behind_a_rook_joins_the_exchange() {
        // Rxd5 Rxd5 Rxd5: a pawn won. Without the d1 rook, -400.
        assert_eq!(
            exchange("3r2k1/8/8/3p4/8/8/3R4/3R2K1 w - - 0 1", "d2d5"),
            100
        );
        assert_eq!(
            exchange("3r2k1/8/8/3p4/8/8/3R4/6K1 w - - 0 1", "d2d5"),
            -400
        );
    }

    #[test]
    fn a_king_never_recaptures_onto_an_attacked_square() {
        // The f1 rook still sees f7 once f2 has captured: the king may not take.
        assert_eq!(exchange("6k1/5p2/8/8/8/8/5R2/5RK1 w - - 0 1", "f2f7"), 100);
        // With nothing behind, the king takes the rook back.
        assert_eq!(exchange("6k1/5p2/8/8/8/8/5R2/6K1 w - - 0 1", "f2f7"), -400);
    }

    #[test]
    fn en_passant_wins_the_pawn_beside_the_target() {
        assert_eq!(exchange("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1", "e5d6"), 100);
    }

    #[test]
    fn a_promotion_on_a_defended_square_loses_the_new_queen() {
        assert_eq!(exchange("1r2k3/P7/8/8/8/8/8/4K3 w - - 0 1", "a7a8q"), -100);
        assert_eq!(exchange("4k3/P7/8/8/8/8/8/4K3 w - - 0 1", "a7a8q"), 800);
    }
}
