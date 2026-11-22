//! `bench [depth]`: search a fixed set of positions to a fixed depth.
//!
//! Two numbers come out. **Nodes per second** measure speed. The **node
//! total** is a signature of the search's behaviour: the search is
//! deterministic at a fixed depth, so a change that should only make the
//! engine faster (SIMD, a better data layout) must leave the node total
//! exactly unchanged. If it moves, the change also changed what the engine
//! searches, and needs an Elo test, not just a stopwatch.
//!
//! A third number, the **first-move cutoff rate** (see
//! `search::SearchStatistics`), measures move ordering: a change meant to
//! order moves better must raise it.
//!
//! Each position is searched with a fresh transposition table of the
//! session's `Hash` size. At the default size, bench's searches barely fill
//! it; `setoption name Hash value 1` before `bench` puts the table under the
//! pressure of a long game, which is where a change to how it keeps entries
//! shows (in its hit rate, and in the node total).

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use eval::{Evaluator, MaterialEval};
use search::{Limits, SearchStatistics, Searcher, TranspositionTable};
use shakmaty::fen::Fen;
use shakmaty::{CastlingMode, Chess};

use crate::evaluation::Evaluation;

pub const DEFAULT_DEPTH: u32 = 6;

/// A spread of positions: openings, tactical middlegames, endgames.
const POSITIONS: [&str; 8] = [
    // The start position.
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    // An open game after 1. e4 e5 2. Nf3 Nc6.
    "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3",
    // "Kiwipete": every kind of move at once, a classic perft position.
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    // A sharp middlegame with a promotion threat (perft position 5).
    "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
    // A quiet, symmetrical middlegame (perft position 6).
    "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
    // A rook endgame with pawns (perft position 3).
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    // A back-rank mate in one.
    "6k1/5ppp/8/8/8/8/5PPP/3R2K1 w - - 0 1",
    // King and pawn against king.
    "4k3/8/8/8/8/8/4P3/4K3 w - - 0 1",
];

#[derive(Debug)]
pub struct BenchResult {
    pub nodes: u64,
    pub statistics: SearchStatistics,
    pub elapsed: Duration,
}

impl BenchResult {
    pub fn nodes_per_second(&self) -> u64 {
        let milliseconds = self.elapsed.as_millis().max(1);
        u64::try_from(u128::from(self.nodes) * 1000 / milliseconds).unwrap_or(u64::MAX)
    }
}

/// Search every bench position to `depth` with `evaluation`; `None` when the
/// evaluation is broken.
pub fn run_bench(
    evaluation: &Evaluation,
    depth: u32,
    hash_megabytes: usize,
) -> Option<BenchResult> {
    run_bench_on(evaluation, depth, hash_megabytes, &POSITIONS)
}

/// The same as [`run_bench`], on the given positions (FENs).
fn run_bench_on(
    evaluation: &Evaluation,
    depth: u32,
    hash_megabytes: usize,
    positions: &[&str],
) -> Option<BenchResult> {
    let started = Instant::now();
    let (nodes, statistics) = match evaluation {
        Evaluation::Material => search_all(&MaterialEval, depth, hash_megabytes, positions),
        Evaluation::Nnue { evaluator, .. } => {
            search_all(evaluator, depth, hash_megabytes, positions)
        }
        Evaluation::Broken { .. } => return None,
    };
    Some(BenchResult {
        nodes,
        statistics,
        elapsed: started.elapsed(),
    })
}

/// Each position is searched by a fresh `Searcher` with its own empty
/// transposition table, so no position's search depends on the one before
/// it, and the node total does not depend on the order of the positions.
fn search_all<E: Evaluator + Clone>(
    evaluator: &E,
    depth: u32,
    hash_megabytes: usize,
    positions: &[&str],
) -> (u64, SearchStatistics) {
    let limits = Limits {
        depth: Some(depth),
        ..Limits::default()
    };
    let never_stop = AtomicBool::new(false);
    positions
        .iter()
        .map(|fen| {
            let table = Arc::new(TranspositionTable::new(hash_megabytes));
            Searcher::with_table(evaluator.clone(), table).search(
                &bench_position(fen),
                &limits,
                &never_stop,
            )
        })
        .fold(
            (0, SearchStatistics::default()),
            |(nodes, statistics), result| (nodes + result.nodes, statistics + result.statistics),
        )
}

fn bench_position(fen: &str) -> Chess {
    fen.parse::<Fen>()
        .expect("bench FENs are valid")
        .into_position(CastlingMode::Standard)
        .expect("bench positions are legal")
}

#[cfg(test)]
mod tests {
    use super::*;
    use search::DEFAULT_HASH_MEGABYTES;

    use nnue::{Network, Nnue};

    /// A quiet position on purpose. A random net knows nothing about
    /// material, so it cannot prune the capture search the way a real
    /// evaluation does: on the tactical bench positions its searches grow
    /// astronomically. One quiet ending is enough to show the net is used.
    const KING_AND_PAWN: [&str; 1] = ["4k3/8/8/8/8/8/4P3/4K3 w - - 0 1"];

    #[test]
    fn a_loaded_net_changes_the_search() {
        let net = Evaluation::Nnue {
            evaluator: Nnue::new(Arc::new(Network::random(5))),
            file: "random".to_string(),
        };
        let with_net = run_bench_on(&net, 3, DEFAULT_HASH_MEGABYTES, &KING_AND_PAWN).unwrap();
        let with_material = run_bench_on(
            &Evaluation::Material,
            3,
            DEFAULT_HASH_MEGABYTES,
            &KING_AND_PAWN,
        )
        .unwrap();
        assert_ne!(with_net.nodes, with_material.nodes);
    }

    #[test]
    fn a_broken_evaluation_cannot_bench() {
        let broken = Evaluation::Broken {
            reason: "missing".to_string(),
        };
        assert!(run_bench_on(&broken, 3, DEFAULT_HASH_MEGABYTES, &KING_AND_PAWN).is_none());
    }
}
