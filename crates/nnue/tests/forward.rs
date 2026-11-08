//! The forward pass on nets that need no file: properties that hold for any
//! weights. `tests/verify.rs` checks exact values against the real net.

use nnue::forward::{activate, evaluate};
use nnue::{Accumulator, L1, Network, QA};
use shakmaty::fen::Fen;
use shakmaty::{CastlingMode, Chess, Color, Position};

fn position(fen: &str) -> Chess {
    fen.parse::<Fen>()
        .unwrap()
        .into_position(CastlingMode::Standard)
        .unwrap()
}

#[test]
fn activation_clamps_and_puts_the_side_to_move_first() {
    let mut accumulator = Accumulator {
        white: [0; L1],
        black: [0; L1],
    };
    accumulator.white[0] = -5;
    accumulator.white[1] = 300;
    accumulator.white[2] = 50;
    accumulator.black[0] = 7;
    let input = activate(&accumulator, Color::Black);
    assert_eq!(input[0], 7, "black (to move) comes first");
    assert_eq!(&input[L1..L1 + 3], &[0, QA as i8, 50]);
}

#[test]
fn colour_mirrored_positions_evaluate_the_same() {
    // HalfKP sees the board from each side's own point of view, so flipping
    // the board, swapping the colours and swapping the side to move gives
    // the net exactly the same inputs.
    let network = Network::random(11);
    let evaluate_fen = |fen: &str| {
        let position = position(fen);
        evaluate(
            &network,
            &Accumulator::refresh(&network, &position),
            position.turn(),
        )
    };
    for (fen, mirrored) in [
        (
            "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3",
            "rnbqkb1r/pppp1ppp/5n2/4p3/4P3/2N5/PPPP1PPP/R1BQKBNR b KQkq - 2 3",
        ),
        (
            "8/5k2/3p4/8/2P5/8/1K6/8 b - - 0 40",
            "8/1k6/8/2p5/8/3P4/5K2/8 w - - 0 40",
        ),
    ] {
        assert_eq!(evaluate_fen(fen), evaluate_fen(mirrored), "{fen}");
    }
}
