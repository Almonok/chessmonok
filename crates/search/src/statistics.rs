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
//!
//! Principal variation search (PVS) bets on the same order: every move after
//! the first is only asked "is it better than the first?" with a zero window,
//! and searched again with the full window when the answer is yes. The share
//! of zero-window searches that needed that re-search is how often the bet
//! was lost.
//!
//! Late-move reductions bet further: moves tried late are searched shallower
//! first, and at full depth only when the shallow search says they beat the
//! best move so far. The share of reduced searches re-searched that way is
//! how often a late move turned out good.
//!
//! Reverse futility pruning cuts nodes before any move is tried; their count
//! shows how much of the tree it removes. Quiescence skips the captures that
//! static exchange evaluation shows losing; their count shows how many
//! subtrees that saves.
//!
//! Aspiration windows bet that an iteration scores close to the last one;
//! each re-search of the root is a lost bet. Quiet-move pruning skips moves
//! unsearched, counted by the rule that skipped them.

use std::ops::Add;

use crate::quiet_pruning::QuietPruning;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SearchStatistics {
    /// Nodes whose search ended early on a move reaching beta.
    pub cutoffs: u64,
    /// Those of them where that move was the first one tried.
    pub first_move_cutoffs: u64,
    /// Moves after the first, searched with a zero window.
    pub zero_window_searches: u64,
    /// Those of them that beat the first move and were searched again with
    /// the full window.
    pub zero_window_re_searches: u64,
    /// Late moves searched at a reduced depth first.
    pub reduced_searches: u64,
    /// Those of them that beat the best move so far and were searched again
    /// at full depth.
    pub reduced_re_searches: u64,
    /// Nodes cut by reverse futility pruning, on their static evaluation.
    pub reverse_futility_cutoffs: u64,
    /// Captures quiescence did not search because they lose material.
    pub losing_captures_skipped: u64,
    /// Root searches repeated because the score fell outside the
    /// aspiration window.
    pub aspiration_re_searches: u64,
    /// Quiet moves skipped because enough quiet moves were searched first.
    pub late_moves_pruned: u64,
    /// Quiet moves skipped because they could not lift the node to alpha.
    pub futile_moves_pruned: u64,
}

impl SearchStatistics {
    pub(crate) fn record_reverse_futility_cutoff(&mut self) {
        self.reverse_futility_cutoffs += 1;
    }

    pub(crate) fn record_quiet_move_pruned(&mut self, reason: QuietPruning) {
        match reason {
            QuietPruning::LateMove => self.late_moves_pruned += 1,
            QuietPruning::Futility => self.futile_moves_pruned += 1,
        }
    }

    pub(crate) fn record_aspiration_re_search(&mut self) {
        self.aspiration_re_searches += 1;
    }

    pub(crate) fn record_losing_capture_skipped(&mut self) {
        self.losing_captures_skipped += 1;
    }

    pub(crate) fn record_cutoff(&mut self, on_first_move: bool) {
        self.cutoffs += 1;
        if on_first_move {
            self.first_move_cutoffs += 1;
        }
    }

    pub(crate) fn record_zero_window_search(&mut self, re_searched: bool) {
        self.zero_window_searches += 1;
        if re_searched {
            self.zero_window_re_searches += 1;
        }
    }

    pub(crate) fn record_reduced_search(&mut self, re_searched: bool) {
        self.reduced_searches += 1;
        if re_searched {
            self.reduced_re_searches += 1;
        }
    }

    /// The share of cutoffs made by the first move, from 0 to 1; 0 when
    /// there was no cutoff at all.
    pub fn first_move_cutoff_rate(&self) -> f64 {
        share(self.first_move_cutoffs, self.cutoffs)
    }

    /// The share of zero-window searches searched again, from 0 to 1.
    pub fn zero_window_re_search_rate(&self) -> f64 {
        share(self.zero_window_re_searches, self.zero_window_searches)
    }

    /// The share of reduced searches searched again at full depth, from 0
    /// to 1.
    pub fn reduced_re_search_rate(&self) -> f64 {
        share(self.reduced_re_searches, self.reduced_searches)
    }
}

/// Summing the statistics of several searches, as `bench` does.
impl Add for SearchStatistics {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            cutoffs: self.cutoffs + other.cutoffs,
            first_move_cutoffs: self.first_move_cutoffs + other.first_move_cutoffs,
            zero_window_searches: self.zero_window_searches + other.zero_window_searches,
            zero_window_re_searches: self.zero_window_re_searches + other.zero_window_re_searches,
            reduced_searches: self.reduced_searches + other.reduced_searches,
            reduced_re_searches: self.reduced_re_searches + other.reduced_re_searches,
            reverse_futility_cutoffs: self.reverse_futility_cutoffs
                + other.reverse_futility_cutoffs,
            losing_captures_skipped: self.losing_captures_skipped + other.losing_captures_skipped,
            aspiration_re_searches: self.aspiration_re_searches + other.aspiration_re_searches,
            late_moves_pruned: self.late_moves_pruned + other.late_moves_pruned,
            futile_moves_pruned: self.futile_moves_pruned + other.futile_moves_pruned,
        }
    }
}

/// `part / whole` as a fraction, 0 when `whole` is 0.
#[allow(
    clippy::cast_precision_loss,
    reason = "a node count above 2^52 would take years to search"
)]
fn share(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    part as f64 / whole as f64
}
