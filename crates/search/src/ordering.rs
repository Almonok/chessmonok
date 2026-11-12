//! Move ordering: which move the search tries first.
//!
//! Alpha-beta stops searching a node's moves as soon as one of them proves
//! the opponent would never allow the position (a beta cutoff). If that move
//! comes first, the node costs one subtree; if it comes tenth, nine subtrees
//! were searched for nothing. With the best move always first, a search `d`
//! plies deep over `b` moves a node visits about `b^(d/2)` positions instead
//! of `b^d`: the whole art of making alpha-beta fast is guessing the best
//! move before searching it.
//!
//! Each move gets a score, and the moves are sorted by it, highest first:
//!
//! ```text
//! the remembered move (transposition table)   i32::MAX
//! captures and promotions that do not lose    2_000_000 + MVV-LVA
//! first killer, second killer (`killers`)     1_000_000, 999_000
//! other quiet moves (`history`)               8 x history - attacker
//! captures and promotions that lose material  -1_000_000 + MVV-LVA
//! ```
//!
//! MVV-LVA is "most valuable victim, least valuable attacker":
//! `10 x victim + 10 x promotion - attacker`, so taking a queen comes before
//! taking a pawn, and taking it with a pawn before taking it with the queen.
//! A capture is tried before any quiet move because it changes the material
//! balance, which is what most refutations do, unless static exchange
//! evaluation (`see`) shows it loses material once the recaptures are played
//! out: such a capture rarely refutes anything and goes last. Killers come
//! after the good captures: quiet
//! moves that refuted a sibling position. Other quiet moves go last, by how
//! often they caused cutoffs so far; the `- attacker` term only breaks ties,
//! which are common early in a search, when most history scores are still 0.
//! History stays inside `+-HISTORY_LIMIT`, so `8 x history` (at most 131 072)
//! stays below the killers.
//!
//! The sort is stable: moves with equal scores keep the order the move
//! generator gave them, so the search, and the `bench` node count that is its
//! signature, are the same on every run.

use std::cmp::Reverse;

use shakmaty::{Chess, Move, MoveList, Position};

use crate::PackedMove;
use crate::history::{HISTORY_LIMIT, History};
use crate::see::static_exchange;

/// Above every killer and quiet move, below the remembered move.
const CAPTURE_SCORE: i32 = 2_000_000;
const FIRST_KILLER_SCORE: i32 = 1_000_000;
const SECOND_KILLER_SCORE: i32 = 999_000;
/// History is multiplied by this so that the piece only breaks ties.
const HISTORY_WEIGHT: i32 = 8;
const _: () = assert!(HISTORY_WEIGHT * HISTORY_LIMIT < SECOND_KILLER_SCORE);
/// Below every quiet move, however bad its history.
const LOSING_CAPTURE_SCORE: i32 = -1_000_000;
const _: () = assert!(LOSING_CAPTURE_SCORE + 200 < -HISTORY_WEIGHT * HISTORY_LIMIT - 6);

/// Order the moves of `position`, a node of the main search, best guess
/// first.
///
/// The remembered move and the killers are only hints (another position may
/// share the table slot; a killer comes from a sibling): each counts only if
/// it is one of these legal moves, which sorting cannot add to.
pub(crate) fn order_moves(
    moves: &mut MoveList,
    position: &Chess,
    remembered: Option<PackedMove>,
    killers: [Option<Move>; 2],
    history: &History,
) {
    let side_to_move = position.turn();
    // `sort_by_cached_key` scores each move once; `sort_by_key` would score
    // it at every comparison, about eight times per move, and each score
    // reads the killers and the history table. Still a stable sort.
    moves.sort_by_cached_key(|&candidate| {
        let is_remembered = remembered.is_some_and(|packed| PackedMove::new(candidate) == packed);
        Reverse(if is_remembered {
            i32::MAX
        } else if !is_quiet(candidate) {
            let bucket = if static_exchange(position, candidate) >= 0 {
                CAPTURE_SCORE
            } else {
                LOSING_CAPTURE_SCORE
            };
            bucket + material_score(candidate)
        } else if killers[0] == Some(candidate) {
            FIRST_KILLER_SCORE
        } else if killers[1] == Some(candidate) {
            SECOND_KILLER_SCORE
        } else {
            HISTORY_WEIGHT * history.score(side_to_move, candidate) + material_score(candidate)
        })
    });
}

/// A move that changes no material: neither a capture (en passant counts as
/// one) nor a promotion. Only quiet moves become killers.
pub(crate) fn is_quiet(candidate: Move) -> bool {
    !candidate.is_capture() && !candidate.is_promotion()
}

/// Order the moves of a quiescence node: captures (or check evasions) by
/// MVV-LVA alone, since quiescence has no remembered move.
pub(crate) fn order_captures(moves: &mut MoveList) {
    moves.sort_by_key(|&candidate| Reverse(material_score(candidate)));
}

/// MVV-LVA, with promotions counted like a capture of the promoted piece.
/// Roles count from 1 (pawn) to 6 (king).
fn material_score(candidate: Move) -> i32 {
    let victim = candidate.capture().map_or(0, |role| role as i32 * 10);
    let promotion = candidate.promotion().map_or(0, |role| role as i32 * 10);
    victim + promotion - candidate.role() as i32
}

#[cfg(test)]
mod tests {
    use shakmaty::fen::Fen;
    use shakmaty::uci::UciMove;
    use shakmaty::{CastlingMode, Chess, Color, Position, Role, Square};

    use super::*;
    use crate::history::History;

    const QUIET_MIDDLEGAME: &str =
        "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10";

    fn position(fen: &str) -> Chess {
        fen.parse::<Fen>()
            .unwrap()
            .into_position(CastlingMode::Standard)
            .unwrap()
    }

    fn parse_move(position: &Chess, uci: &str) -> Move {
        uci.parse::<UciMove>().unwrap().to_move(position).unwrap()
    }

    fn ordered(fen: &str, remembered: Option<Move>, killers: [Option<Move>; 2]) -> MoveList {
        ordered_with_history(fen, remembered, killers, &History::new())
    }

    fn ordered_with_history(
        fen: &str,
        remembered: Option<Move>,
        killers: [Option<Move>; 2],
        history: &History,
    ) -> MoveList {
        let position = position(fen);
        let mut moves = position.legal_moves();
        order_moves(
            &mut moves,
            &position,
            remembered.map(PackedMove::new),
            killers,
            history,
        );
        moves
    }

    #[test]
    fn a_losing_capture_comes_after_every_quiet_move() {
        // The e6 pawn defends d5: Qxd5 loses the queen and Nxd5 the knight,
        // each for a pawn.
        const LOSING_CAPTURES: &str = "4k3/8/4p3/3p4/8/2N5/8/3QK3 w - - 0 1";
        let board = position(LOSING_CAPTURES);
        let losing = [parse_move(&board, "d1d5"), parse_move(&board, "c3d5")];
        let moves = ordered(LOSING_CAPTURES, None, [None, None]);
        let first_losing_index = moves
            .iter()
            .position(|candidate| losing.contains(candidate))
            .unwrap();
        for (index, &candidate) in moves.iter().enumerate() {
            if is_quiet(candidate) {
                assert!(
                    index < first_losing_index,
                    "{candidate:?} after a losing capture"
                );
            }
        }
    }

    #[test]
    fn a_winning_capture_comes_before_the_killers() {
        const WINNING_CAPTURE: &str = "4k3/8/8/3n4/4P3/8/8/4K3 w - - 0 1";
        let board = position(WINNING_CAPTURE);
        let pawn_takes = parse_move(&board, "e4d5");
        let killer = parse_move(&board, "e1d2");
        let moves = ordered(WINNING_CAPTURE, None, [Some(killer), None]);
        assert_eq!(moves[0], pawn_takes);
        assert_eq!(moves[1], killer);
    }

    #[test]
    fn quiet_moves_follow_their_history() {
        let board = position(QUIET_MIDDLEGAME);
        let killer = parse_move(&board, "b2b4");
        let (rarely_good, often_good) = (parse_move(&board, "a3a4"), parse_move(&board, "h2h3"));
        let mut history = History::new();
        history.reward(Color::White, often_good, 5);
        let moves = ordered_with_history(QUIET_MIDDLEGAME, None, [Some(killer), None], &history);
        let index_of = |wanted: Move| {
            moves
                .iter()
                .position(|&candidate| candidate == wanted)
                .unwrap()
        };
        assert!(index_of(killer) < index_of(often_good));
        assert!(index_of(often_good) < index_of(rarely_good));
    }

    #[test]
    fn killers_come_after_safe_captures_and_before_other_quiet_moves() {
        let board = position(QUIET_MIDDLEGAME);
        let killer = parse_move(&board, "a3a4");
        let moves = ordered(QUIET_MIDDLEGAME, None, [Some(killer), None]);
        let killer_index = moves
            .iter()
            .position(|&candidate| candidate == killer)
            .unwrap();
        for (index, &candidate) in moves.iter().enumerate() {
            if is_quiet(candidate) {
                if candidate != killer {
                    assert!(index > killer_index, "{candidate:?} before the killer");
                }
            } else if static_exchange(&board, candidate) >= 0 {
                assert!(index < killer_index, "{candidate:?} after the killer");
            } else {
                assert!(
                    index > killer_index,
                    "losing {candidate:?} before the killer"
                );
            }
        }
    }

    #[test]
    fn a_killer_that_is_not_legal_here_changes_nothing() {
        let elsewhere = Move::Normal {
            role: Role::Queen,
            from: Square::D1,
            capture: None,
            to: Square::D8,
            promotion: None,
        };
        assert_eq!(
            ordered(QUIET_MIDDLEGAME, None, [Some(elsewhere), None]),
            ordered(QUIET_MIDDLEGAME, None, [None, None])
        );
    }

    #[test]
    fn the_remembered_move_stays_first_even_when_it_is_a_killer() {
        let killer = parse_move(&position(QUIET_MIDDLEGAME), "a3a4");
        let moves = ordered(QUIET_MIDDLEGAME, Some(killer), [Some(killer), None]);
        assert_eq!(moves[0], killer);
        assert_eq!(
            moves
                .iter()
                .filter(|&&candidate| candidate == killer)
                .count(),
            1
        );
    }

    #[test]
    fn promotions_and_en_passant_are_not_quiet() {
        let en_passant_position = position("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1");
        let promotion_position = position("4k3/P7/8/8/8/8/8/4K3 w - - 0 1");
        assert!(!is_quiet(parse_move(&en_passant_position, "e5d6")));
        assert!(!is_quiet(parse_move(&promotion_position, "a7a8q")));
        assert!(is_quiet(parse_move(&promotion_position, "e1d2")));
    }
}
