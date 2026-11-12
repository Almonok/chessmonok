//! Reverse futility pruning: cut a shallow node whose static evaluation is
//! already far above beta.
//!
//! A node a few plies from the leaves, whose evaluation before any move is
//! well above beta, almost always stays above beta once searched: in so few
//! plies the opponent rarely wins back more than a modest margin. So the
//! node is cut on its evaluation alone, without generating a move.
//!
//! "Futility" pruning skips a move that cannot lift a node up to alpha;
//! this is the reverse: the opponent's moves cannot bring the node down to
//! beta. The margin is what the opponent could still win back, and grows
//! with the plies left to do it:
//!
//! ```text
//! cut when  static evaluation - MARGIN_PER_PLY x depth  >=  beta
//!
//!   depth 1:  evaluation at least beta +  80 cp
//!   depth 6:  evaluation at least beta + 480 cp
//! ```
//!
//! Beyond `MAXIMUM_DEPTH` the margin would have to be so wide that the cut
//! hardly ever happens, and a mistake there costs a deep search, so deeper
//! nodes are always searched. The other guards are checked by the caller
//! (`Searcher::reverse_futility_cutoff`): only zero-window nodes, where the
//! search only asks "at least beta?", never the principal variation; never
//! in check, where the static evaluation ignores that the king must be
//! saved; and never at a mate-score beta, which only a search can prove.

use crate::is_mate;

/// The deepest node that may be cut. Six plies is common for a first
/// version; the depth and the margin are tuned later, each measured alone.
const MAXIMUM_DEPTH: u32 = 6;

/// Centipawns the opponent is assumed able to win back per ply left.
const MARGIN_PER_PLY: i32 = 80;

/// What the opponent could still win back from a node with `depth` plies
/// left.
fn margin(depth: u32) -> i32 {
    MARGIN_PER_PLY * i32::try_from(depth).expect("a depth fits in i32")
}

/// The score that ends a node with `depth` plies left and this static
/// evaluation, or `None` when it must be searched.
///
/// The score is the evaluation minus the margin, the lowest value the cut
/// assumes, rather than the evaluation itself: it is still at least beta,
/// so the cut holds, but it claims no more than what was assumed. A parent
/// that keeps it as its best score is then not misled by an optimistic one.
pub(crate) fn cutoff_score(depth: u32, static_evaluation: i32, beta: i32) -> Option<i32> {
    if depth > MAXIMUM_DEPTH || is_mate(beta) {
        return None;
    }
    let lowest_expected = static_evaluation - margin(depth);
    (lowest_expected >= beta).then_some(lowest_expected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MATE;

    #[test]
    fn a_node_far_enough_above_beta_is_cut_at_the_lowest_expected_score() {
        assert_eq!(cutoff_score(1, 180, 100), Some(100));
        assert_eq!(cutoff_score(3, 400, 100), Some(160));
    }

    #[test]
    fn a_node_within_the_margin_is_searched() {
        assert_eq!(cutoff_score(1, 179, 100), None);
        assert_eq!(cutoff_score(3, 339, 100), None);
    }

    #[test]
    fn the_margin_grows_with_the_depth_left() {
        for depth in 1..MAXIMUM_DEPTH {
            assert!(margin(depth + 1) > margin(depth));
        }
    }

    #[test]
    fn deep_nodes_are_always_searched() {
        assert_eq!(cutoff_score(MAXIMUM_DEPTH, 10_000, 0), Some(10_000 - 480));
        assert_eq!(cutoff_score(MAXIMUM_DEPTH + 1, 10_000, 0), None);
    }

    #[test]
    fn a_mate_score_beta_is_never_cut() {
        // Being mated in 5 is beta: a good evaluation does not prove the
        // mate is escaped.
        assert_eq!(cutoff_score(2, 300, -MATE + 5), None);
        assert_eq!(cutoff_score(2, MATE, MATE - 5), None);
    }
}
