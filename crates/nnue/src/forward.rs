//! The quantised forward pass: exercise N1, part 2. See
//! `docs/exercises/n1-refresh-and-forward.md`; the spec is
//! `hidden_layers_match_the_oracle` and `evals_match_the_oracle` in
//! `tests/verify.rs`.
//!
//! ```text
//! x0 = clamp([acc_stm | acc_nstm], 0, QA)            u8 x 512
//! x1 = clamp((l1_weight @ x0 + l1_bias) >> 6, 0, QA)  u8 x 32
//! x2 = clamp((l2_weight @ x1 + l2_bias) >> 6, 0, QA)  u8 x 32
//! cp = (out_weight @ x2 + out_bias) * OUT_SCALE / (QA * QB), toward zero
//! ```

use shakmaty::Color;

use crate::{Accumulator, L1, L2, L3, Network};

/// Clamp both halves to `[0, QA]` and concatenate them, side to move first.
pub fn activate(acc: &Accumulator, stm: Color) -> [u8; 2 * L1] {
    let _ = (acc, stm);
    todo!()
}

/// The first hidden layer: `clamp((l1_weight @ x0 + l1_bias) >> 6, 0, QA)`.
pub fn hidden1(net: &Network, x0: &[u8; 2 * L1]) -> [u8; L2] {
    let _ = (net, x0);
    todo!()
}

/// The second hidden layer: the same computation over `L2` inputs.
pub fn hidden2(net: &Network, x1: &[u8; L2]) -> [u8; L3] {
    let _ = (net, x1);
    todo!()
}

/// Centipawns from the side to move:
/// `(out_weight @ x2 + out_bias) * OUT_SCALE / (QA * QB)`, truncated toward
/// zero.
pub fn output(net: &Network, x2: &[u8; L3]) -> i32 {
    let _ = (net, x2);
    todo!()
}

/// The whole pass, from an accumulator to centipawns for `stm`.
pub fn evaluate(net: &Network, acc: &Accumulator, stm: Color) -> i32 {
    output(net, &hidden2(net, &hidden1(net, &activate(acc, stm))))
}
