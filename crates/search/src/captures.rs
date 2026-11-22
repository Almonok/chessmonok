//! The legal captures of a position not in check, for quiescence.
//!
//! Quiescence searches captures only, at every one of its nodes, and those
//! are most of the nodes of a search. shakmaty's `capture_moves` does not
//! generate captures for standard chess: it generates every legal move,
//! quiet ones included, then keeps the captures. On `bench` that threw away
//! work worth 13 % of the search time.
//!
//! A capture lands on a square holding an enemy piece, so it is enough to
//! intersect each piece's attacks with the enemy pieces:
//!
//! ```text
//! knight on f3, enemy pieces on e5 and g5, own pawn on d4
//!
//!   knight_attacks(f3)  = d2 d4 e1 e5 g1 g5 h2 h4
//!   & enemy pieces      =          e5    g5
//!                       = the two captures, nothing else generated
//! ```
//!
//! A capture can still be illegal, which only happens in three ways when
//! the side to move is not in check:
//!
//! - the piece is **pinned** (the only piece between its king and an enemy
//!   rook, bishop or queen) and leaves the line of the pin;
//! - the **king** captures a defended piece;
//! - an **en passant** capture removes two pawns from one rank at once and
//!   uncovers a rook on it, a case shakmaty's `en_passant_moves` handles.
//!
//! The moves come out in exactly the order shakmaty's own generator lists
//! them (en passant, pawns, knights, bishops, rooks, queens, king; squares
//! in increasing order): quiescence sorts captures with a stable sort, so
//! the same order means the same search, node for node.

use shakmaty::attacks::{
    aligned, between, bishop_attacks, king_attacks, knight_attacks, queen_attacks, rook_attacks,
};
use shakmaty::{Bitboard, Board, Chess, File, Move, MoveList, Position, Role, Square};

/// The legal captures of `position`, en passant included and promotions
/// that capture included, in shakmaty's order.
///
/// `position` must not be in check: out of check the legal moves are the
/// evasions, which quiescence generates in full.
pub(crate) fn capture_moves(position: &Chess) -> MoveList {
    debug_assert!(!position.is_check(), "captures out of check are evasions");
    let board = position.board();
    let us = board.by_color(position.turn());
    let enemies = board.by_color(!position.turn());
    let king = board
        .king_of(position.turn())
        .expect("a legal position has both kings");
    let pinned = pinned_pieces(board, enemies, king);

    let mut moves = position.en_passant_moves();
    push_pawn_captures(position, enemies, &mut moves);
    for role in [Role::Knight, Role::Bishop, Role::Rook, Role::Queen] {
        for from in board.by_role(role) & us {
            let reachable = piece_attacks(role, from, board.occupied()) & enemies;
            for to in reachable {
                push_capture(board, role, from, to, None, &mut moves);
            }
        }
    }
    push_king_captures(position, king, enemies, &mut moves);
    moves.retain(|capture| keeps_the_king_safe(*capture, pinned, king));
    moves
}

/// Pawn captures, one diagonal at a time as shakmaty generates them: first
/// towards the a-file, then towards the h-file; on each diagonal the plain
/// captures, then the captures that promote (to a queen, rook, bishop and
/// knight, in that order).
fn push_pawn_captures(position: &Chess, enemies: Bitboard, moves: &mut MoveList) {
    let board = position.board();
    let pawns = board.pawns() & board.by_color(position.turn());
    let forward = position.turn().fold_wb(8, -8);
    // A pawn on the a-file has no capture towards the a-file, and one on the
    // h-file none towards the h-file: without the mask the shift would wrap
    // around to the other side of the board.
    let diagonals = [
        (pawns & !Bitboard::from_file(File::A), forward - 1),
        (pawns & !Bitboard::from_file(File::H), forward + 1),
    ];
    for (capturing_pawns, offset) in diagonals {
        let targets = capturing_pawns.shift(offset) & enemies;
        for to in targets & !Bitboard::BACKRANKS {
            push_capture(board, Role::Pawn, from_square(to, offset), to, None, moves);
        }
        for to in targets & Bitboard::BACKRANKS {
            for promotion in [Role::Queen, Role::Rook, Role::Bishop, Role::Knight] {
                let from = from_square(to, offset);
                push_capture(board, Role::Pawn, from, to, Some(promotion), moves);
            }
        }
    }
}

/// The king's captures of pieces nothing defends.
fn push_king_captures(position: &Chess, king: Square, enemies: Bitboard, moves: &mut MoveList) {
    let board = position.board();
    for to in king_attacks(king) & enemies {
        let defended = board
            .attacks_to(to, !position.turn(), board.occupied())
            .any();
        if !defended {
            push_capture(board, Role::King, king, to, None, moves);
        }
    }
}

fn push_capture(
    board: &Board,
    role: Role,
    from: Square,
    to: Square,
    promotion: Option<Role>,
    moves: &mut MoveList,
) {
    moves.push(Move::Normal {
        role,
        from,
        capture: board.role_at(to),
        to,
        promotion,
    });
}

/// The square a pawn captured from, given where it landed and the shift
/// that took it there.
fn from_square(to: Square, offset: i32) -> Square {
    to.offset(-offset)
        .expect("a pawn capture comes from a square on the board")
}

fn piece_attacks(role: Role, from: Square, occupied: Bitboard) -> Bitboard {
    match role {
        Role::Knight => knight_attacks(from),
        Role::Bishop => bishop_attacks(from, occupied),
        Role::Rook => rook_attacks(from, occupied),
        Role::Queen => queen_attacks(from, occupied),
        Role::Pawn | Role::King => unreachable!("pawns and the king have their own generators"),
    }
}

/// The pieces that stand alone between `king` and an enemy rook, bishop or
/// queen aiming at it along an empty-board line: moving one off that line
/// would expose the king. (Enemy pieces can be on the list too; only ours
/// ever move here.)
fn pinned_pieces(board: &Board, enemies: Bitboard, king: Square) -> Bitboard {
    let snipers = (rook_attacks(king, Bitboard::EMPTY) & board.rooks_and_queens())
        | (bishop_attacks(king, Bitboard::EMPTY) & board.bishops_and_queens());
    let mut pinned = Bitboard::EMPTY;
    for sniper in snipers & enemies {
        let in_between = between(king, sniper) & board.occupied();
        if !in_between.more_than_one() {
            pinned |= in_between;
        }
    }
    pinned
}

/// Whether `capture` leaves the king out of check. Only a pinned piece can
/// expose it here, by leaving the line between its king and the pinner;
/// moving along that line (capturing the pinner included) is safe. The king
/// never stands on a pin line itself, and en passant was already checked.
fn keeps_the_king_safe(capture: Move, pinned: Bitboard, king: Square) -> bool {
    match capture {
        Move::Normal { from, to, .. } => !pinned.contains(from) || aligned(from, to, king),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use shakmaty::fen::Fen;
    use shakmaty::{CastlingMode, Chess, Position};

    use super::capture_moves;

    /// Positions full of what makes captures hard: pins, en passant (one
    /// that uncovers a rook on the rank), promotions that capture, kings
    /// next to defended pieces. Kiwipete and positions 3, 4 and 5 of the
    /// standard perft suite, plus a pinned en passant.
    const POSITIONS: [&str; 6] = [
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
        "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
        "8/8/8/K2pP2r/8/8/8/7k w - d6 0 1",
    ];

    /// Every position up to `depth` plies from `position`.
    fn visit(position: &Chess, depth: u32, check: &mut impl FnMut(&Chess)) {
        check(position);
        if depth == 0 {
            return;
        }
        for chess_move in position.legal_moves() {
            let mut child = position.clone();
            child.play_unchecked(chess_move);
            visit(&child, depth - 1, check);
        }
    }

    #[test]
    fn the_captures_are_shakmatys_in_shakmatys_order() {
        let mut compared = 0;
        for fen in POSITIONS {
            let position: Chess = fen
                .parse::<Fen>()
                .unwrap()
                .into_position(CastlingMode::Standard)
                .unwrap();
            visit(&position, 3, &mut |position| {
                if position.is_check() {
                    return;
                }
                assert_eq!(
                    capture_moves(position),
                    position.capture_moves(),
                    "{}",
                    Fen::from_position(position, shakmaty::EnPassantMode::Legal)
                );
                compared += 1;
            });
        }
        // Enough positions that every kind of capture shows up many times.
        assert!(compared > 100_000, "only {compared} positions compared");
    }
}
