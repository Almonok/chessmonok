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
    fn update(&self, parent: &Self::State, after: &Chess, dirty: &DirtyPieces) -> Self::State;
    /// Centipawns from the side to move.
    fn evaluate(&self, state: &Self::State, pos: &Chess) -> i32;
}
