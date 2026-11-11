//! History: how often each quiet move caused a cutoff, anywhere in the search.
//!
//! Killer moves remember the last two refutations at one ply. History
//! remembers across the whole tree: one score per side to move, from-square
//! and to-square (castling counts as the king moving onto its rook). When a
//! quiet move causes a beta cutoff with `d` plies left to search, it is
//! rewarded, and every quiet move tried before it at that node, which failed
//! to cut, is penalised by the same amount:
//!
//! ```text
//! bonus = min(d x d, 1024)
//! ```
//!
//! A deep cutoff saves a big subtree, so it counts more. Scores are kept
//! inside `+-HISTORY_LIMIT` by "gravity" rather than by clipping:
//!
//! ```text
//! score += bonus - score x |bonus| / HISTORY_LIMIT
//! ```
//!
//! The further a score is from zero, the more each update pulls it back, so
//! it can approach the limit but never cross it, and old knowledge fades as
//! new cutoffs arrive instead of piling up at a ceiling where every move
//! looks the same.

use shakmaty::{Color, Move};

/// The bound the scores stay inside; ordering scales them below the killers.
pub(crate) const HISTORY_LIMIT: i32 = 16_384;

/// The largest single update. Without a cap, a cutoff at depth 30 would move
/// a score by most of its range at once.
const MAXIMUM_BONUS: i32 = 1_024;

const SQUARES: usize = 64;

/// One score per side to move, from-square and to-square.
#[derive(Clone, Debug)]
pub(crate) struct History {
    /// One 64 x 64 table per side, 32 KiB in all, on the heap: built on the
    /// stack first, an array this size would risk overflowing a thread's
    /// stack (clippy's `large_stack_arrays`), so `vec!` builds it in place.
    scores: Box<[[[i32; SQUARES]; SQUARES]]>,
}

impl History {
    /// Every score at zero: nothing has caused a cutoff yet.
    pub(crate) fn new() -> Self {
        Self {
            scores: vec![[[0; SQUARES]; SQUARES]; 2].into_boxed_slice(),
        }
    }

    pub(crate) fn score(&self, side_to_move: Color, quiet_move: Move) -> i32 {
        let (from, to) = squares(quiet_move);
        self.scores[side_to_move as usize][from][to]
    }

    /// `m` caused a cutoff with `remaining_depth` plies left.
    pub(crate) fn reward(&mut self, side_to_move: Color, quiet_move: Move, remaining_depth: u32) {
        self.update(side_to_move, quiet_move, bonus(remaining_depth));
    }

    /// `m` was tried before the move that cut, and did not cut.
    pub(crate) fn penalise(&mut self, side_to_move: Color, quiet_move: Move, remaining_depth: u32) {
        self.update(side_to_move, quiet_move, -bonus(remaining_depth));
    }

    fn update(&mut self, side_to_move: Color, quiet_move: Move, bonus: i32) {
        let (from, to) = squares(quiet_move);
        let score = &mut self.scores[side_to_move as usize][from][to];
        *score += bonus - *score * bonus.abs() / HISTORY_LIMIT;
    }
}

fn bonus(remaining_depth: u32) -> i32 {
    i32::try_from(remaining_depth.saturating_mul(remaining_depth))
        .unwrap_or(i32::MAX)
        .min(MAXIMUM_BONUS)
}

/// The from- and to-square of a move, as indices. Every move of standard
/// chess has a from-square; only a drop (crazyhouse) has none.
fn squares(quiet_move: Move) -> (usize, usize) {
    let from = quiet_move.from().expect("standard chess has no drops");
    (usize::from(from), usize::from(quiet_move.to()))
}

#[cfg(test)]
mod tests {
    use shakmaty::{Color, Move, Role, Square};

    use super::*;

    fn knight_move() -> Move {
        Move::Normal {
            role: Role::Knight,
            from: Square::G1,
            capture: None,
            to: Square::F3,
            promotion: None,
        }
    }

    #[test]
    fn a_reward_raises_the_score_and_a_penalty_lowers_it() {
        let mut history = History::new();
        history.reward(Color::White, knight_move(), 4);
        assert!(history.score(Color::White, knight_move()) > 0);
        let mut history = History::new();
        history.penalise(Color::White, knight_move(), 4);
        assert!(history.score(Color::White, knight_move()) < 0);
    }

    #[test]
    fn the_sides_have_separate_scores() {
        let mut history = History::new();
        history.reward(Color::White, knight_move(), 4);
        assert_eq!(history.score(Color::Black, knight_move()), 0);
    }

    #[test]
    fn history_stays_within_its_limit() {
        let mut history = History::new();
        for _ in 0..10_000 {
            history.reward(Color::White, knight_move(), 30);
            assert!(history.score(Color::White, knight_move()) <= HISTORY_LIMIT);
        }
        for _ in 0..10_000 {
            history.penalise(Color::White, knight_move(), 30);
            assert!(history.score(Color::White, knight_move()) >= -HISTORY_LIMIT);
        }
    }

    #[test]
    fn deeper_cutoffs_earn_more() {
        let mut shallow = History::new();
        shallow.reward(Color::White, knight_move(), 2);
        let mut deep = History::new();
        deep.reward(Color::White, knight_move(), 6);
        assert!(
            deep.score(Color::White, knight_move()) > shallow.score(Color::White, knight_move())
        );
    }
}
