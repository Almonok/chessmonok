//! Quiet-move pruning: at a shallow zero-window node, skip the quiet moves
//! that cannot matter.
//!
//! Reverse futility pruning cuts a whole node that is far above beta. These
//! two rules work one move at a time, on nodes a few plies from the leaves,
//! and only on quiet moves (no capture, no promotion), which change no
//! material.
//!
//! **Late-move pruning.** Moves are searched best guess first (`ordering`).
//! Once enough quiet moves have failed at a shallow node, the ones ordered
//! after them, with even less history behind them, almost never succeed:
//!
//! ```text
//! skip the remaining quiet moves once  3 + depth^2  have been searched
//!
//!   depth 1: 4     depth 2: 7     depth 3: 12     depth 4: 19
//! ```
//!
//! **Futility pruning.** A quiet move only shifts the position, and so few
//! plies from the leaves it rarely moves the evaluation by more than a
//! positional margin. When even the evaluation plus that margin stays at or
//! below alpha, the move cannot lift the node above alpha:
//!
//! ```text
//! skip a quiet move when  static evaluation + 100 cp x depth  <=  alpha
//! ```
//!
//! The caller (`Searcher::negamax`) keeps the guards both rules rely on:
//! zero-window nodes only, never the principal variation; never in check,
//! where there is no static evaluation; never a move that gives check, which
//! can win more than its evaluation shows; and only once a move searched at
//! this node scored better than being mated, so a node never ends with every
//! move skipped and no score.

use crate::is_mate;

/// Which rule skipped a quiet move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QuietPruning {
    /// Enough quiet moves were already searched here.
    LateMove,
    /// The evaluation plus a margin cannot reach alpha.
    Futility,
}

/// The deepest node whose quiet moves may be skipped. Four plies is common
/// for a first version; depth and margins are tuned later, each measured
/// alone.
const MAXIMUM_DEPTH: u32 = 4;

/// Centipawns a quiet move is assumed able to gain per ply left.
const FUTILITY_MARGIN_PER_PLY: i32 = 100;

/// How many quiet moves are searched at a node `depth` plies from the
/// leaves before the rest are skipped.
fn late_move_count(depth: u32) -> usize {
    3 + (depth * depth) as usize
}

/// What a quiet move is assumed able to gain with `depth` plies left.
fn futility_margin(depth: u32) -> i32 {
    FUTILITY_MARGIN_PER_PLY * i32::try_from(depth).expect("a depth fits in i32")
}

/// Why the next quiet move at this node may be skipped, or `None` when it
/// must be searched. `quiet_moves_searched` counts the quiet moves already
/// searched at the node, not the ones skipped.
pub(crate) fn skip_reason(
    depth: u32,
    quiet_moves_searched: usize,
    static_evaluation: i32,
    alpha: i32,
) -> Option<QuietPruning> {
    if depth > MAXIMUM_DEPTH {
        return None;
    }
    if quiet_moves_searched >= late_move_count(depth) {
        return Some(QuietPruning::LateMove);
    }
    // A mate-score alpha is only reached by a search, not by a margin.
    let futile = !is_mate(alpha) && static_evaluation + futility_margin(depth) <= alpha;
    futile.then_some(QuietPruning::Futility)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MATE;

    #[test]
    fn late_quiet_moves_are_skipped_once_enough_were_searched() {
        assert_eq!(skip_reason(1, 3, 0, 0), None);
        assert_eq!(skip_reason(1, 4, 0, 0), Some(QuietPruning::LateMove));
        assert_eq!(skip_reason(3, 11, 0, 0), None);
        assert_eq!(skip_reason(3, 12, 0, 0), Some(QuietPruning::LateMove));
    }

    #[test]
    fn a_quiet_move_is_futile_when_the_margin_cannot_reach_alpha() {
        assert_eq!(skip_reason(2, 0, 100, 300), Some(QuietPruning::Futility));
        assert_eq!(skip_reason(2, 0, 101, 300), None);
    }

    #[test]
    fn deep_nodes_keep_every_quiet_move() {
        assert_eq!(skip_reason(MAXIMUM_DEPTH + 1, 100, -10_000, 0), None);
        assert!(skip_reason(MAXIMUM_DEPTH, 100, -10_000, 0).is_some());
    }

    #[test]
    fn a_mate_score_alpha_is_never_futile() {
        assert_eq!(skip_reason(1, 0, 0, MATE - 10), None);
    }
}
