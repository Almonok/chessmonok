use shakmaty::{Chess, Position};

use crate::{DirtyPieces, Evaluator};

const VALUES: [i32; 6] = [100, 320, 330, 500, 900, 0];

/// Placeholder until the NNUE: centipawn material from the side to move.
#[derive(Clone, Copy, Debug, Default)]
pub struct MaterialEval;

impl Evaluator for MaterialEval {
    type State = ();

    fn refresh(&self, _: &Chess) {}

    fn update(&self, (): &(), (): &mut (), _: &Chess, _: &DirtyPieces) {}

    fn evaluate(&self, (): &(), pos: &Chess) -> i32 {
        let us = pos.turn();
        pos.board()
            .iter()
            .map(|(_, p)| {
                let value = VALUES[p.role as usize - 1];
                if p.color == us { value } else { -value }
            })
            .sum()
    }
}
