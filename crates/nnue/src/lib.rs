//! `HalfKP` NNUE inference for chessmonok (design spec 2026-10-13 §4).
//!
//! [`Network`] is the quantised net almoNnuet's `export.py` writes;
//! [`Accumulator`] is the per-ply state the search keeps; [`forward`] turns
//! one into a score. [`Nnue`] plugs them into the search through
//! [`eval::Evaluator`].

mod accumulator;
pub mod features;
pub mod forward;
mod network;

use std::sync::Arc;

use eval::{DirtyPieces, Evaluator};
use shakmaty::{Chess, Position};

pub use accumulator::Accumulator;
pub use network::{
    ARCH, FEATURE_SET, FILE_SIZE, L0, L1, L2, L3, LoadError, Network, OUT_SCALE, QA, QB, fnv1a32,
};

/// The NNUE evaluator. Cloning shares the network.
#[derive(Clone, Debug)]
pub struct Nnue {
    net: Arc<Network>,
}

impl Nnue {
    pub fn new(net: Arc<Network>) -> Self {
        Self { net }
    }
}

impl Evaluator for Nnue {
    type State = Accumulator;

    fn refresh(&self, pos: &Chess) -> Accumulator {
        Accumulator::refresh(&self.net, pos)
    }

    fn update(&self, parent: &Accumulator, after: &Chess, dirty: &DirtyPieces) -> Accumulator {
        parent.update(&self.net, after, dirty)
    }

    fn evaluate(&self, state: &Accumulator, pos: &Chess) -> i32 {
        forward::evaluate(&self.net, state, pos.turn())
    }
}
