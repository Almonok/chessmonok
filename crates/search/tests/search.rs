use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use eval::MaterialEval;
use search::{Limits, MATE, SearchResult, Searcher, time_budget};
use shakmaty::fen::Fen;
use shakmaty::{CastlingMode, Chess, Color, Move, Position};

fn pos(fen: &str) -> Chess {
    fen.parse::<Fen>()
        .unwrap()
        .into_position(CastlingMode::Standard)
        .unwrap()
}

fn uci(m: Move) -> String {
    m.to_uci(CastlingMode::Standard).to_string()
}

fn run(fen: &str, depth: u32) -> SearchResult {
    let limits = Limits {
        depth: Some(depth),
        ..Limits::default()
    };
    Searcher::new(MaterialEval).search(&pos(fen), &limits, &AtomicBool::new(false))
}

#[test]
fn finds_mate_in_one() {
    let r = run("6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1", 3);
    assert_eq!(uci(r.best_move.unwrap()), "a1a8");
    assert_eq!(r.score, MATE - 1);
}

#[test]
fn finds_mate_in_two() {
    let r = run("k7/8/2K5/8/8/8/8/7R w - - 0 1", 4);
    assert_eq!(r.score, MATE - 3);
    assert!(["c6b6", "c6c7"].contains(&uci(r.best_move.unwrap()).as_str()));
}

#[test]
fn takes_a_hanging_queen() {
    let r = run("4k3/8/8/3q4/8/8/8/3RK3 w - - 0 1", 3);
    assert_eq!(uci(r.best_move.unwrap()), "d1d5");
}

#[test]
fn quiescence_sees_the_recapture() {
    let r = run("4k3/8/4p3/3p4/8/8/8/3QK3 w - - 0 1", 1);
    assert_ne!(uci(r.best_move.unwrap()), "d1d5");
}

#[test]
fn stalemated_root_scores_zero() {
    let r = run("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1", 3);
    assert!(r.best_move.is_none());
    assert_eq!(r.score, 0);
}

#[test]
fn mated_root_has_no_move() {
    let r = run("R5k1/5ppp/8/8/8/8/8/6K1 b - - 0 1", 3);
    assert!(r.best_move.is_none());
    assert_eq!(r.score, -MATE);
}

#[test]
fn respects_the_depth_limit_and_counts_nodes() {
    let r = run(
        "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3",
        3,
    );
    assert_eq!(r.depth, 3);
    assert!(r.nodes > 0);
    assert_eq!(r.pv.first().copied(), r.best_move);
}

#[test]
fn best_move_is_legal_on_varied_positions() {
    for fen in [
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        "r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1",
    ] {
        let p = pos(fen);
        let m = run(fen, 3).best_move.unwrap();
        assert!(p.legal_moves().contains(&m), "{fen}");
    }
}

#[test]
fn preset_stop_still_returns_a_move() {
    let p = Chess::default();
    let r = Searcher::new(MaterialEval).search(&p, &Limits::default(), &AtomicBool::new(true));
    assert_eq!(r.depth, 1);
    assert!(p.legal_moves().contains(&r.best_move.unwrap()));
}

#[test]
fn movetime_is_respected() {
    let limits = Limits {
        movetime: Some(Duration::from_millis(100)),
        ..Limits::default()
    };
    let start = Instant::now();
    let r = Searcher::new(MaterialEval).search(&Chess::default(), &limits, &AtomicBool::new(false));
    assert!(start.elapsed() < Duration::from_millis(600));
    assert!(r.depth >= 1);
}

#[test]
fn on_depth_reports_every_iteration() {
    let mut depths = Vec::new();
    let limits = Limits {
        depth: Some(3),
        ..Limits::default()
    };
    Searcher::new(MaterialEval).search_with(
        &Chess::default(),
        &limits,
        &AtomicBool::new(false),
        |r| {
            depths.push(r.depth);
        },
    );
    assert_eq!(depths, [1, 2, 3]);
}

fn secs(s: f64) -> Duration {
    Duration::from_secs_f64(s)
}

#[test]
fn budget_uses_movetime_first() {
    let l = Limits {
        movetime: Some(secs(0.25)),
        wtime: Some(secs(60.0)),
        ..Limits::default()
    };
    assert_eq!(time_budget(&l, Color::White), Some(secs(0.25)));
}

#[test]
fn budget_is_a_thirtieth_plus_half_the_increment() {
    let l = Limits {
        wtime: Some(secs(60.0)),
        winc: Some(secs(1.0)),
        btime: Some(secs(30.0)),
        ..Limits::default()
    };
    assert_eq!(time_budget(&l, Color::White), Some(secs(2.5)));
    assert_eq!(time_budget(&l, Color::Black), Some(secs(1.0)));
}

#[test]
fn budget_never_exceeds_a_quarter_of_the_clock() {
    let l = Limits {
        wtime: Some(secs(2.0)),
        winc: Some(secs(10.0)),
        ..Limits::default()
    };
    assert_eq!(time_budget(&l, Color::White), Some(secs(0.5)));
}

#[test]
fn budget_is_none_without_a_clock() {
    assert_eq!(time_budget(&Limits::default(), Color::White), None);
}
