//! Late-move reductions: search the moves tried late at a smaller depth.
//!
//! With killer moves and history, a node's refutation is nearly always among
//! its first moves (`bench`: the first move cuts 91 % of the time). A quiet
//! move tried tenth is almost certainly bad, yet proving it bad at full
//! depth costs as much as searching a good move. So late quiet moves are
//! searched a few plies shallower first, with a zero window: the cheap
//! search is enough to show they do not beat the best move so far. A move
//! that surprises, by beating it anyway, is searched again at full depth
//! (`Searcher::search_after_move`): a good move ordered late is slowed down,
//! not lost.
//!
//! How much shallower grows with the remaining depth and the move's place
//! in the order, slowly (logarithms), a formula common to many engines:
//!
//! ```text
//! reduction = floor(0.75 + ln(depth) x ln(move index + 1) / 2.25)
//!
//!   depth 3,  3rd move   -> 1 ply
//!   depth 8,  10th move  -> 2 plies
//!   depth 20, 30th move  -> 5 plies
//! ```
//!
//! capped so that the reduced search keeps at least one ply. Some moves are
//! never reduced, because a shallow search would misjudge them: the first
//! two tried (usually the table move and the best capture or a killer),
//! captures and promotions (they change the material, which a shallow search
//! sees badly), moves that give check, every move when the side to move is
//! in check, and every move of a node with fewer than three plies left.

use std::sync::LazyLock;

use shakmaty::Move;

use crate::ordering::is_quiet;

/// Nodes with fewer plies left are too shallow to reduce: their children
/// are already one ply from quiescence.
const MINIMUM_DEPTH: u32 = 3;
/// The first moves tried are the likeliest refutations: never reduced.
const FIRST_REDUCED_MOVE_INDEX: usize = 2;
/// The table covers depths and move indices up to this; beyond it, the last
/// row or column is used (the formula barely grows that far out).
const TABLE_SIZE: usize = 64;

/// `REDUCTIONS[depth][move_index]`, from the formula, before the cap.
/// Built on first use: `f64::ln` is not a `const fn`, so the table cannot be
/// a compile-time constant, and `LazyLock` computes it once, thread-safely,
/// instead of taking two logarithms at every node.
static REDUCTIONS: LazyLock<[[u32; TABLE_SIZE]; TABLE_SIZE]> = LazyLock::new(|| {
    let mut table = [[0; TABLE_SIZE]; TABLE_SIZE];
    for (depth, row) in table.iter_mut().enumerate().skip(1) {
        for (move_index, reduction) in row.iter_mut().enumerate() {
            *reduction = formula(depth, move_index);
        }
    }
    table
});

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "indices below 64, and a positive result below 64"
)]
fn formula(depth: usize, move_index: usize) -> u32 {
    let move_number = (move_index + 1) as f64;
    (0.75 + (depth as f64).ln() * move_number.ln() / 2.25).floor() as u32
}

/// How many plies shallower than normal to search the move tried at
/// `move_index` (0 for the first) in a node with `remaining_depth` plies
/// left: 0 below `MINIMUM_DEPTH`, and never so much that the reduced search
/// would have less than one ply.
pub(crate) fn late_move_reduction(remaining_depth: u32, move_index: usize) -> u32 {
    if remaining_depth < MINIMUM_DEPTH {
        return 0;
    }
    let row =
        usize::try_from(remaining_depth).map_or(TABLE_SIZE - 1, |depth| depth.min(TABLE_SIZE - 1));
    let column = move_index.min(TABLE_SIZE - 1);
    REDUCTIONS[row][column].min(remaining_depth - 2)
}

/// Whether the move tried at `move_index` may be searched shallower: see
/// the module docs for why each kind of move is kept at full depth.
pub(crate) fn may_reduce(
    remaining_depth: u32,
    move_index: usize,
    chess_move: Move,
    in_check: bool,
    gives_check: bool,
) -> bool {
    remaining_depth >= MINIMUM_DEPTH
        && move_index >= FIRST_REDUCED_MOVE_INDEX
        && is_quiet(chess_move)
        && !in_check
        && !gives_check
}

#[cfg(test)]
mod tests {
    use shakmaty::{Move, Role, Square};

    use super::*;

    fn quiet_knight_move() -> Move {
        Move::Normal {
            role: Role::Knight,
            from: Square::G1,
            capture: None,
            to: Square::F3,
            promotion: None,
        }
    }

    #[test]
    fn the_first_two_moves_are_never_reduced() {
        assert!(!may_reduce(10, 0, quiet_knight_move(), false, false));
        assert!(!may_reduce(10, 1, quiet_knight_move(), false, false));
        assert!(may_reduce(10, 2, quiet_knight_move(), false, false));
    }

    #[test]
    fn shallow_nodes_are_never_reduced() {
        assert!(!may_reduce(2, 10, quiet_knight_move(), false, false));
        assert_eq!(late_move_reduction(1, 10), 0);
        assert_eq!(late_move_reduction(2, 10), 0);
    }

    #[test]
    fn the_formula_gives_the_documented_values() {
        assert_eq!(late_move_reduction(3, 2), 1);
        assert_eq!(late_move_reduction(8, 9), 2);
        assert_eq!(late_move_reduction(20, 29), 5);
    }

    #[test]
    fn reductions_grow_with_depth_and_move_index() {
        for depth in 1..63 {
            for move_index in 0..63 {
                let here = late_move_reduction(depth, move_index);
                assert!(late_move_reduction(depth + 1, move_index) >= here);
                assert!(late_move_reduction(depth, move_index + 1) >= here);
            }
        }
    }

    #[test]
    fn a_reduced_search_keeps_at_least_one_ply() {
        for depth in 3..64 {
            for move_index in 0..300 {
                assert!(late_move_reduction(depth, move_index) <= depth - 2);
            }
        }
    }

    #[test]
    fn move_indices_past_the_table_use_its_last_column() {
        assert_eq!(late_move_reduction(10, 200), late_move_reduction(10, 63));
    }

    #[test]
    fn captures_and_promotions_are_not_reduced() {
        let capture = Move::Normal {
            role: Role::Knight,
            from: Square::F3,
            capture: Some(Role::Pawn),
            to: Square::E5,
            promotion: None,
        };
        let promotion = Move::Normal {
            role: Role::Pawn,
            from: Square::A7,
            capture: None,
            to: Square::A8,
            promotion: Some(Role::Queen),
        };
        assert!(!may_reduce(10, 5, capture, false, false));
        assert!(!may_reduce(10, 5, promotion, false, false));
    }

    #[test]
    fn checking_moves_are_not_reduced() {
        assert!(!may_reduce(10, 5, quiet_knight_move(), false, true));
    }

    #[test]
    fn nothing_is_reduced_in_check() {
        assert!(!may_reduce(10, 5, quiet_knight_move(), true, false));
    }
}
