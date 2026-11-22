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
mod refresh_cache;

use std::sync::Arc;

use eval::{DirtyPieces, Evaluator};
use shakmaty::{Chess, Position};

pub use accumulator::Accumulator;
pub use network::{
    ARCH, FEATURE_SET, FILE_SIZE, L0, L1, L2, L3, LoadError, Network, OUT_SCALE, QA, QB, fnv1a32,
};
pub use refresh_cache::RefreshCache;

/// The NNUE evaluator. Cloning shares the network and copies the refresh
/// cache, which each searcher then keeps up to date on its own.
#[derive(Clone, Debug)]
pub struct Nnue {
    net: Arc<Network>,
    refresh_cache: RefreshCache,
}

impl Nnue {
    pub fn new(net: Arc<Network>) -> Self {
        let refresh_cache = RefreshCache::new(&net);
        Self { net, refresh_cache }
    }
}

impl Evaluator for Nnue {
    type State = Accumulator;

    fn refresh(&self, pos: &Chess) -> Accumulator {
        Accumulator::refresh(&self.net, pos)
    }

    fn update(
        &mut self,
        parent: &Accumulator,
        child: &mut Accumulator,
        after: &Chess,
        dirty: &DirtyPieces,
    ) {
        parent.update(&self.net, after, dirty, child, &mut self.refresh_cache);
    }

    fn evaluate(&self, state: &Accumulator, pos: &Chess) -> i32 {
        forward::evaluate(&self.net, state, pos.turn())
    }
}
