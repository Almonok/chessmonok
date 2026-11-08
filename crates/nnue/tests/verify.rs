//! The cross-language oracle: almoNnuet's verify.json for a real net.
//!
//! Needs `CHESSMONOK_NET`, the `.nnue` to verify, absolute or relative to the
//! repo root; its oracle is read from `<stem>.verify.json` beside it.
//!
//! ```text
//! CHESSMONOK_NET=nets/run1-e11.nnue cargo test -p nnue --test verify -- --ignored
//! ```

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use eval::Evaluator;
use nnue::features::active_features;
use nnue::forward::{activate, hidden1, hidden2, output};
use nnue::{Accumulator, L1, Network, Nnue};
use serde::Deserialize;
use shakmaty::fen::Fen;
use shakmaty::{CastlingMode, Chess, Position};

#[derive(Deserialize)]
struct Oracle {
    detailed: Vec<Detailed>,
    bulk: Vec<Bulk>,
}

#[derive(Deserialize)]
struct Detailed {
    fen: String,
    #[serde(rename = "stm_features")]
    side_to_move_features: Vec<usize>,
    #[serde(rename = "nstm_features")]
    not_side_to_move_features: Vec<usize>,
    acc_white: Vec<i16>,
    acc_black: Vec<i16>,
    l1_out: Vec<i8>,
    l2_out: Vec<i8>,
    eval: i32,
}

#[derive(Deserialize)]
struct Bulk {
    fen: String,
    eval: i32,
}

fn net_path() -> PathBuf {
    let raw = std::env::var("CHESSMONOK_NET").expect(
        "set CHESSMONOK_NET to the .nnue to verify, e.g. CHESSMONOK_NET=nets/run1-e11.nnue",
    );
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        // cargo runs tests from crates/nnue; paths are given from the repo root.
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path)
    }
}

fn fixture() -> &'static (Arc<Network>, Oracle) {
    static FIXTURE: OnceLock<(Arc<Network>, Oracle)> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let path = net_path();
        let net = Network::load(&path).unwrap_or_else(|e| panic!("{e}"));
        let json = path.with_extension("verify.json");
        let text = std::fs::read_to_string(&json)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", json.display()));
        let oracle = serde_json::from_str(&text).expect("verify.json matches the schema");
        (Arc::new(net), oracle)
    })
}

fn position(fen: &str) -> Chess {
    fen.parse::<Fen>()
        .unwrap()
        .into_position(CastlingMode::Standard)
        .unwrap()
}

#[test]
#[ignore = "needs CHESSMONOK_NET"]
fn features_match_the_oracle() {
    let (_, oracle) = fixture();
    for e in &oracle.detailed {
        let pos = position(&e.fen);
        let side_to_move = pos.turn();
        assert_eq!(
            active_features(pos.board(), side_to_move),
            e.side_to_move_features,
            "{}",
            e.fen
        );
        assert_eq!(
            active_features(pos.board(), side_to_move.other()),
            e.not_side_to_move_features,
            "{}",
            e.fen
        );
    }
}

fn half(values: &[i16]) -> [i16; L1] {
    values.try_into().expect("256 accumulator values")
}

#[test]
#[ignore = "needs CHESSMONOK_NET"]
fn accumulators_match_the_oracle() {
    let (net, oracle) = fixture();
    for e in &oracle.detailed {
        let acc = Accumulator::refresh(net, &position(&e.fen));
        assert_eq!(acc.white, half(&e.acc_white), "white half of {}", e.fen);
        assert_eq!(acc.black, half(&e.acc_black), "black half of {}", e.fen);
    }
}

#[test]
#[ignore = "needs CHESSMONOK_NET"]
fn hidden_layers_match_the_oracle() {
    let (net, oracle) = fixture();
    for e in &oracle.detailed {
        // Python's accumulators, so a refresh bug cannot hide a layer bug.
        let acc = Accumulator {
            white: half(&e.acc_white),
            black: half(&e.acc_black),
        };
        let x1 = hidden1(net, &activate(&acc, position(&e.fen).turn()));
        assert_eq!(x1.as_slice(), e.l1_out, "l1 of {}", e.fen);
        let x2 = hidden2(net, &x1);
        assert_eq!(x2.as_slice(), e.l2_out, "l2 of {}", e.fen);
        assert_eq!(output(net, &x2), e.eval, "output of {}", e.fen);
    }
}

#[test]
#[ignore = "needs CHESSMONOK_NET"]
fn evals_match_the_oracle() {
    let (net, oracle) = fixture();
    let nnue = Nnue::new(Arc::clone(net));
    let cases = oracle
        .detailed
        .iter()
        .map(|e| (&e.fen, e.eval))
        .chain(oracle.bulk.iter().map(|e| (&e.fen, e.eval)));
    let mut checked = 0;
    for (fen, expected) in cases {
        let pos = position(fen);
        assert_eq!(nnue.evaluate(&nnue.refresh(&pos), &pos), expected, "{fen}");
        checked += 1;
    }
    assert_eq!(checked, 2050);
}
