//! The quantised forward pass: from an accumulator to centipawns.
//!
//! `tests/verify.rs` checks every layer against almoNnuet's own integer
//! forward pass, to the exact value.
//!
//! ```text
//! x0 = clamp([accumulator of the side to move | accumulator of the other side], 0, QA)   i8 x 512
//! x1 = clamp((l1_weight @ x0 + l1_bias) >> 6, 0, QA)  i8 x 32
//! x2 = clamp((l2_weight @ x1 + l2_bias) >> 6, 0, QA)  i8 x 32
//! cp = (out_weight @ x2 + out_bias) * OUT_SCALE / (QA * QB), toward zero
//! ```
//!
//! # Why the activations are `i8`
//!
//! An activation is never negative, yet it is stored as a signed byte. The
//! reason is the instruction the compiler can pick for `weights · input`.
//! ARM's `sdot` multiplies 16 pairs of bytes and adds them into four 32-bit
//! sums in one instruction, but only when *both* operands are signed. With
//! `u8` activations against `i8` weights, the compiler has no such
//! instruction: it widens every byte to 16 bits first, then multiplies 4
//! pairs per instruction (`smlal`), about eight times the work for the
//! first hidden layer. Clamping to `[0, QA]` with `QA = 127` keeps every
//! activation inside `i8`, so the change of type changes no value.

use shakmaty::Color;

use crate::{Accumulator, L1, L2, L3, Network, OUT_SCALE, QA, QB};

/// Dividing by `QB` (64) is a right shift by 6. An arithmetic shift floors
/// negative numbers, exactly like almoNnuet's numpy `>>`.
const WEIGHT_SHIFT: u32 = 6;
const _: () = assert!(1 << WEIGHT_SHIFT == QB);

/// Clamp both halves to `[0, QA]` and concatenate them, side to move first.
///
/// One loop per half, not one loop over all `2 * L1` outputs choosing its
/// half at each index: with that choice inside the loop, the compiler
/// fetched every vector lane on its own, from an address picked per lane.
/// Over one contiguous half it loads, clamps and narrows whole vectors.
pub fn activate(accumulator: &Accumulator, side_to_move: Color) -> [i8; 2 * L1] {
    let mut activations = [0; 2 * L1];
    let (side_to_move_activations, other_side_activations) = activations.split_at_mut(L1);
    clip_half(side_to_move_activations, accumulator.half(side_to_move));
    clip_half(
        other_side_activations,
        accumulator.half(side_to_move.other()),
    );
    activations
}

/// Clamp one accumulator half into `activations`, element by element.
fn clip_half(activations: &mut [i8], half: &[i16; L1]) {
    for (activation, &value) in activations.iter_mut().zip(half) {
        *activation = clipped(i32::from(value));
    }
}

/// The first hidden layer: `clamp((l1_weight @ input + l1_bias) >> 6, 0, QA)`.
pub fn hidden1(net: &Network, input: &[i8; 2 * L1]) -> [i8; L2] {
    dense_layer(&net.l1_weight, &net.l1_bias, input)
}

/// The second hidden layer: the same computation over `L2` inputs.
pub fn hidden2(net: &Network, input: &[i8; L2]) -> [i8; L3] {
    dense_layer(&net.l2_weight, &net.l2_bias, input)
}

/// Centipawns from the side to move:
/// `(out_weight @ input + out_bias) * OUT_SCALE / (QA * QB)`, truncated
/// toward zero.
pub fn output(net: &Network, input: &[i8; L3]) -> i32 {
    let sum = net.out_bias
        + input
            .iter()
            .zip(&net.out_weight)
            .map(|(&value, &weight)| i32::from(value) * i32::from(weight))
            .sum::<i32>();
    // In i64: `sum * 600` can exceed i32 with a real net's weights. Integer
    // division truncates toward zero, as almoNnuet's reference does.
    (i64::from(sum) * i64::from(OUT_SCALE) / i64::from(QA * QB)) as i32
}

/// One fully connected layer with a clipped activation: for each output,
/// `clamp((weights row · input + bias) >> 6, 0, QA)`.
///
/// `INPUTS` and `OUTPUTS` are const generics: the sizes are part of the
/// types, so the compiler checks that the weights, the biases and the input
/// fit together, and can unroll the loops for these exact sizes.
fn dense_layer<const INPUTS: usize, const OUTPUTS: usize>(
    weights: &[i8],
    biases: &[i32; OUTPUTS],
    input: &[i8; INPUTS],
) -> [i8; OUTPUTS] {
    std::array::from_fn(|output_index| {
        let row = &weights[output_index * INPUTS..(output_index + 1) * INPUTS];
        let sum = biases[output_index]
            + row
                .iter()
                .zip(input)
                .map(|(&weight, &value)| i32::from(weight) * i32::from(value))
                .sum::<i32>();
        clipped(sum >> WEIGHT_SHIFT)
    })
}

/// The clipped `ReLU` activation: negative becomes 0, above `QA` becomes `QA`.
fn clipped(value: i32) -> i8 {
    i8::try_from(value.clamp(0, QA)).expect("clamped into 0..=127")
}

/// The whole pass, from an accumulator to centipawns for `side_to_move`.
pub fn evaluate(net: &Network, accumulator: &Accumulator, side_to_move: Color) -> i32 {
    output(
        net,
        &hidden2(net, &hidden1(net, &activate(accumulator, side_to_move))),
    )
}
