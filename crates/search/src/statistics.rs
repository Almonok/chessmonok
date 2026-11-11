//! How well the search orders its moves, measured while it runs.
//!
//! Alpha-beta stops searching a node's moves as soon as one of them proves
//! the node is already too good for the opponent (a beta cutoff). The sooner
//! that move comes, the fewer subtrees are searched for nothing, so the share
//! of cutoffs made by the very first move tried is a direct measure of move
//! ordering:
//!
//! ```text
//! first-move cutoff rate = cutoffs by the first move / all cutoffs
//! ```
//!
//! It answers in seconds, from one `bench` run, what an Elo match answers in
//! twenty minutes: whether a change to the order helps. Only the main search
//! counts; quiescence, table and null-move cutoffs try no ordered move.

use std::ops::Add;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SearchStatistics {
    /// Nodes whose search ended early on a move reaching beta.
    pub cutoffs: u64,
    /// Those of them where that move was the first one tried.
    pub first_move_cutoffs: u64,
}

impl SearchStatistics {
    pub(crate) fn record_cutoff(&mut self, on_first_move: bool) {
        self.cutoffs += 1;
        if on_first_move {
            self.first_move_cutoffs += 1;
        }
    }

    /// The share of cutoffs made by the first move, from 0 to 1; 0 when
    /// there was no cutoff at all.
    #[allow(
        clippy::cast_precision_loss,
        reason = "a node count above 2^52 would take years to search"
    )]
    pub fn first_move_cutoff_rate(&self) -> f64 {
        if self.cutoffs == 0 {
            return 0.0;
        }
        self.first_move_cutoffs as f64 / self.cutoffs as f64
    }
}

/// Summing the statistics of several searches, as `bench` does.
impl Add for SearchStatistics {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            cutoffs: self.cutoffs + other.cutoffs,
            first_move_cutoffs: self.first_move_cutoffs + other.first_move_cutoffs,
        }
    }
}
