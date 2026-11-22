//! Evaluation seam for chessmonok.

mod dirty;
mod material;

use shakmaty::Chess;

pub use dirty::{DirtyPiece, DirtyPieces, dirty_pieces};
pub use material::MaterialEval;

/// A position evaluator with per-ply state the search keeps up to date.
pub trait Evaluator {
    type State: Clone;
    fn refresh(&self, pos: &Chess) -> Self::State;
    /// Write into `child` the state after the move whose disturbed pieces
    /// are `dirty`, computed from `parent`, the state before it. `after` is
    /// the position after the move.
    ///
    /// `child` is overwritten whatever it held. Writing in place, rather
    /// than returning a new state, lets the search keep one slot per ply and
    /// fill it directly: an NNUE state is a kilobyte, and returning it by
    /// value cost a copy at every node.
    ///
    /// `&mut self`: an evaluator may keep caches of its own up to date (the
    /// NNUE's refresh cache), so each searcher owns its evaluator.
    fn update(
        &mut self,
        parent: &Self::State,
        child: &mut Self::State,
        after: &Chess,
        dirty: &DirtyPieces,
    );
    /// Centipawns from the side to move.
    fn evaluate(&self, state: &Self::State, pos: &Chess) -> i32;
}
