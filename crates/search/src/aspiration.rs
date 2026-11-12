//! Aspiration windows: search each iteration in a narrow window around the
//! previous iteration's score.
//!
//! Iterative deepening searches depth 1, 2, 3... and one iteration's score
//! rarely moves far from the last one's. Alpha-beta cuts more the narrower
//! its window `(alpha, beta)`, so the root is searched with a window around
//! the previous score instead of the full one, and only widened if the score
//! lands outside it:
//!
//! ```text
//!   depth 7 scored +35
//!   depth 8, window (35 - 50, 35 + 50) = (-15, 85)
//!
//!       score inside, -15 < s < 85   the exact score: done
//!       score <= -15, a "fail low"   only an upper bound: search again,
//!                                    alpha moved down, the step doubled
//!       score >= 85, a "fail high"   only a lower bound: search again,
//!                                    beta moved up, the step doubled
//! ```
//!
//! A miss costs a second search of the root, but most of it comes back from
//! the transposition table, and the narrow window pays on every iteration
//! that does not miss. Shallow iterations, whose scores still swing, and mate
//! scores, which jump by whole plies, are searched with the full window.

use crate::{INF, is_mate};

/// The first depth searched through a window; shallower scores swing too
/// much from one iteration to the next to be a good guess.
pub(crate) const MINIMUM_DEPTH: u32 = 4;

/// Half the width of the first window, in centipawns. On bench 13, 25 missed
/// 49 times and grew the tree by 5 %; 50 missed 30 times, and searched the
/// fewest nodes at depth 15 of 25, 50 and 100. Tuned later against Elo,
/// like every margin.
const INITIAL_HALF_WIDTH: i32 = 50;

/// The window `(alpha, beta)` the root is searched with, and the step it is
/// widened by after a miss.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AspirationWindow {
    pub(crate) alpha: i32,
    pub(crate) beta: i32,
    step: i32,
}

impl AspirationWindow {
    /// Every score fits: what alpha-beta uses without aspiration.
    pub(crate) const FULL: Self = Self {
        alpha: -INF,
        beta: INF,
        step: INF,
    };

    /// The first window around `previous_score`, or the full window for a
    /// mate score.
    pub(crate) fn around(previous_score: i32) -> Self {
        if is_mate(previous_score) {
            return Self::FULL;
        }
        Self {
            alpha: previous_score - INITIAL_HALF_WIDTH,
            beta: previous_score + INITIAL_HALF_WIDTH,
            step: INITIAL_HALF_WIDTH,
        }
    }

    /// The window to search again with after this one returned `score`, or
    /// `None` when the score is exact. Only the side that missed moves: past
    /// `score`, by a step twice the last, so that a score falling far keeps
    /// the misses few.
    pub(crate) fn widened_after(self, score: i32) -> Option<Self> {
        let step = self.step.saturating_mul(2);
        if score <= self.alpha {
            Some(Self {
                alpha: score.saturating_sub(step).max(-INF),
                step,
                ..self
            })
        } else if score >= self.beta {
            Some(Self {
                beta: score.saturating_add(step).min(INF),
                step,
                ..self
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MATE;

    const HALF: i32 = INITIAL_HALF_WIDTH;

    #[test]
    fn the_first_window_is_centred_on_the_previous_score() {
        let window = AspirationWindow::around(35);
        assert_eq!((window.alpha, window.beta), (35 - HALF, 35 + HALF));
    }

    #[test]
    fn a_score_inside_the_window_is_exact() {
        assert_eq!(AspirationWindow::around(35).widened_after(40), None);
    }

    #[test]
    fn a_fail_low_moves_only_alpha_down_by_a_doubled_step() {
        let first = AspirationWindow::around(35);
        let wider = first.widened_after(first.alpha).unwrap();
        assert_eq!(
            (wider.alpha, wider.beta),
            (first.alpha - 2 * HALF, first.beta)
        );
        let wider_again = wider.widened_after(wider.alpha - 10).unwrap();
        assert_eq!(
            (wider_again.alpha, wider_again.beta),
            (wider.alpha - 10 - 4 * HALF, first.beta)
        );
    }

    #[test]
    fn a_fail_high_moves_only_beta_up_by_a_doubled_step() {
        let first = AspirationWindow::around(35);
        let wider = first.widened_after(first.beta + 10).unwrap();
        assert_eq!(
            (wider.alpha, wider.beta),
            (first.alpha, first.beta + 10 + 2 * HALF)
        );
    }

    #[test]
    fn the_window_never_grows_past_the_full_window() {
        let mut window = AspirationWindow::around(0);
        for _ in 0..20 {
            window = window.widened_after(window.beta).unwrap_or(window);
        }
        assert_eq!(window.beta, INF);
    }

    #[test]
    fn a_mate_score_is_searched_with_the_full_window() {
        assert_eq!(AspirationWindow::around(MATE - 5), AspirationWindow::FULL);
        assert_eq!(AspirationWindow::around(-MATE + 5), AspirationWindow::FULL);
    }
}
