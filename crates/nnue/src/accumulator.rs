//! The accumulator: exercises N1 (refresh) and N2 (update). See
//! `docs/exercises/n1-refresh-and-forward.md` and
//! `docs/exercises/n2-incremental-update.md`; the specs are
//! `tests/verify.rs` and `tests/incremental.rs`.

use eval::DirtyPieces;
use shakmaty::{Board, Chess, Color};

use crate::{L1, Network};

/// The feature transformer's output for both perspectives: `ft_bias` plus the
/// `ft_row` of every feature that perspective sees.
///
/// Indexed by colour, never by side to move, so a move never swaps the
/// halves; `forward::activate` puts the side to move first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Accumulator {
    pub white: [i16; L1],
    pub black: [i16; L1],
}

impl Accumulator {
    /// One perspective's half.
    pub fn half(&self, persp: Color) -> &[i16; L1] {
        match persp {
            Color::White => &self.white,
            Color::Black => &self.black,
        }
    }

    /// One perspective's half, mutably.
    pub fn half_mut(&mut self, persp: Color) -> &mut [i16; L1] {
        match persp {
            Color::White => &mut self.white,
            Color::Black => &mut self.black,
        }
    }

    /// N1: both halves from scratch.
    pub fn refresh(net: &Network, pos: &Chess) -> Self {
        // Hint: start from zeros and let refresh_half fill each perspective.
        let _ = (net, pos);
        todo!()
    }

    /// N1: recompute `persp`'s half from scratch: the bias plus one row per
    /// feature in `features::active_features(board, persp)`. N2 calls it when
    /// `persp`'s own king moved.
    pub fn refresh_half(&mut self, net: &Network, board: &Board, persp: Color) {
        let _ = (self, net, board, persp);
        todo!()
    }

    /// N2: the accumulator after the move whose disturbed pieces are `dirty`,
    /// computed from this one (the accumulator before the move). `after` is
    /// the position after the move: it knows where the kings now stand.
    #[must_use]
    pub fn update(&self, net: &Network, after: &Chess, dirty: &DirtyPieces) -> Self {
        // Hint: per perspective, a king among the dirty pieces of that colour
        // means refresh_half; otherwise, skipping kings (which have no row),
        // subtract the row of every piece that left a square and add the row
        // of every piece that arrived.
        let _ = (self, net, after, dirty);
        todo!()
    }
}
