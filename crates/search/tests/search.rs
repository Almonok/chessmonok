use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use eval::MaterialEval;
use search::{
    Limits, MATE, SearchResult, SearchStatistics, Searcher, TranspositionTable, position_key,
    time_budget,
};
use shakmaty::fen::Fen;
use shakmaty::uci::UciMove;
use shakmaty::{CastlingMode, Chess, Color, Move, Position};

fn pos(fen: &str) -> Chess {
    fen.parse::<Fen>()
        .unwrap()
        .into_position(CastlingMode::Standard)
        .unwrap()
}

fn uci(chess_move: Move) -> String {
    chess_move.to_uci(CastlingMode::Standard).to_string()
}

fn run(fen: &str, depth: u32) -> SearchResult {
    let limits = Limits {
        depth: Some(depth),
        ..Limits::default()
    };
    Searcher::new(MaterialEval).search(&pos(fen), &limits, &AtomicBool::new(false))
}

/// Like `run`, but reading and writing a table that outlives the search.
fn run_with_table(table: &Arc<TranspositionTable>, fen: &str, depth: u32) -> SearchResult {
    let limits = Limits {
        depth: Some(depth),
        ..Limits::default()
    };
    Searcher::with_table(MaterialEval, Arc::clone(table)).search(
        &pos(fen),
        &limits,
        &AtomicBool::new(false),
    )
}

const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

#[test]
fn finds_mate_in_one() {
    let result = run("6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1", 3);
    assert_eq!(uci(result.best_move.unwrap()), "a1a8");
    assert_eq!(result.score, MATE - 1);
}

/// Depth 5, one ply more than the mate needs: the mating king move is quiet
/// and tried late, so late-move reductions search it at depth 1 first, too
/// shallow to see the rook mate behind it (found from depth 5 on).
#[test]
fn finds_mate_in_two() {
    let result = run("k7/8/2K5/8/8/8/8/7R w - - 0 1", 5);
    assert_eq!(result.score, MATE - 3);
    assert!(["c6b6", "c6c7"].contains(&uci(result.best_move.unwrap()).as_str()));
}

#[test]
fn finds_mate_in_three_with_a_rook() {
    // The attacking side has a rook, so passes are tried all along the line:
    // they must not hide the mate. Depth 8, not 6: every move of the mating
    // plan is a quiet king or rook move, the kind late-move reductions search
    // shallower, so the mate is found two plies later (from depth 8 on).
    let result = run("k7/8/8/3K4/8/8/8/7R w - - 0 1", 8);
    assert_eq!(result.score, MATE - 5);
}

#[test]
fn finds_mate_in_three_in_a_middlegame() {
    let result = run(
        "r1b1kb1r/pppp1ppp/5q2/4n3/3KP3/2N3PN/PPP4P/R1BQ1B1R b kq - 0 1",
        6,
    );
    assert_eq!(result.score, MATE - 5);
    assert_eq!(uci(result.best_move.unwrap()), "f8c5");
}

#[test]
fn never_passes_with_only_pawns_left() {
    // Neither side has a piece, so null-move pruning must not change this
    // search at all: the count is the one measured with `may_pass` switched
    // off. Re-measure it that way whenever the move order changes.
    let result = run("8/pp3k2/2p5/8/8/2P5/PP3K2/8 w - - 0 1", 9);
    assert_eq!(result.nodes, 5_537);
}

#[test]
fn null_move_pruning_halves_a_middlegame_search_at_least() {
    // 3_564_643 nodes at depth 6 before null-move pruning.
    let result = run(
        "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
        6,
    );
    assert!(result.nodes < 3_564_643 / 2, "{} nodes", result.nodes);
}

#[test]
fn takes_a_hanging_queen() {
    let result = run("4k3/8/8/3q4/8/8/8/3RK3 w - - 0 1", 3);
    assert_eq!(uci(result.best_move.unwrap()), "d1d5");
}

#[test]
fn quiescence_sees_the_recapture() {
    let result = run("4k3/8/4p3/3p4/8/8/8/3QK3 w - - 0 1", 1);
    assert_ne!(uci(result.best_move.unwrap()), "d1d5");
}

#[test]
fn stalemated_root_scores_zero() {
    let result = run("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1", 3);
    assert!(result.best_move.is_none());
    assert_eq!(result.score, 0);
}

#[test]
fn mated_root_has_no_move() {
    let result = run("R5k1/5ppp/8/8/8/8/8/6K1 b - - 0 1", 3);
    assert!(result.best_move.is_none());
    assert_eq!(result.score, -MATE);
}

#[test]
fn respects_the_depth_limit_and_counts_nodes() {
    let result = run(
        "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3",
        3,
    );
    assert_eq!(result.depth, 3);
    assert!(result.nodes > 0);
    assert_eq!(result.pv.first().copied(), result.best_move);
}

#[test]
fn best_move_is_legal_on_varied_positions() {
    for fen in [
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        "r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1",
    ] {
        let position = pos(fen);
        let best_move = run(fen, 3).best_move.unwrap();
        assert!(position.legal_moves().contains(&best_move), "{fen}");
    }
}

#[test]
fn preset_stop_still_returns_a_move() {
    let position = Chess::default();
    let result =
        Searcher::new(MaterialEval).search(&position, &Limits::default(), &AtomicBool::new(true));
    assert_eq!(result.depth, 1);
    assert!(position.legal_moves().contains(&result.best_move.unwrap()));
}

#[test]
fn movetime_is_respected() {
    let limits = Limits {
        movetime: Some(Duration::from_millis(100)),
        ..Limits::default()
    };
    let start = Instant::now();
    let result =
        Searcher::new(MaterialEval).search(&Chess::default(), &limits, &AtomicBool::new(false));
    assert!(start.elapsed() < Duration::from_millis(600));
    assert!(result.depth >= 1);
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
        &[],
        &limits,
        &AtomicBool::new(false),
        |result| {
            depths.push(result.depth);
        },
    );
    assert_eq!(depths, [1, 2, 3]);
}

fn secs(s: f64) -> Duration {
    Duration::from_secs_f64(s)
}

#[test]
fn budget_uses_movetime_first() {
    let limits = Limits {
        movetime: Some(secs(0.25)),
        wtime: Some(secs(60.0)),
        ..Limits::default()
    };
    assert_eq!(time_budget(&limits, Color::White), Some(secs(0.25)));
}

#[test]
fn budget_is_a_thirtieth_plus_half_the_increment() {
    let limits = Limits {
        wtime: Some(secs(60.0)),
        winc: Some(secs(1.0)),
        btime: Some(secs(30.0)),
        ..Limits::default()
    };
    assert_eq!(time_budget(&limits, Color::White), Some(secs(2.5)));
    assert_eq!(time_budget(&limits, Color::Black), Some(secs(1.0)));
}

#[test]
fn budget_never_exceeds_a_quarter_of_the_clock() {
    let limits = Limits {
        wtime: Some(secs(2.0)),
        winc: Some(secs(10.0)),
        ..Limits::default()
    };
    assert_eq!(time_budget(&limits, Color::White), Some(secs(0.5)));
}

#[test]
fn budget_is_none_without_a_clock() {
    assert_eq!(time_budget(&Limits::default(), Color::White), None);
}

#[test]
fn a_warm_table_saves_most_of_the_work() {
    let table = Arc::new(TranspositionTable::new(16));
    let cold = run_with_table(&table, KIWIPETE, 5);
    let warm = run_with_table(&table, KIWIPETE, 5);
    assert!(
        warm.nodes * 4 < cold.nodes,
        "cold {} nodes, warm {} nodes",
        cold.nodes,
        warm.nodes
    );
    assert!(
        pos(KIWIPETE)
            .legal_moves()
            .contains(&warm.best_move.unwrap())
    );
}

#[test]
fn a_mate_remembered_at_another_ply_keeps_its_true_distance() {
    // The first search stores "mate in 1" for the position after 1.Kb6 Kb8,
    // two plies below its root. The second search starts one ply later
    // (after 1.Kb6), so it meets that position one ply below its root: the
    // mate must now read as 2 plies away for black, not 3.
    // Depth 5: the first search must find the mate, see `finds_mate_in_two`.
    let table = Arc::new(TranspositionTable::new(1));
    let first = run_with_table(&table, "k7/8/2K5/8/8/8/8/7R w - - 0 1", 5);
    assert_eq!(first.score, MATE - 3);
    let later = run_with_table(&table, "k7/8/1K6/8/8/8/8/7R b - - 1 1", 5);
    assert_eq!(later.score, -(MATE - 2));
}

#[test]
fn a_cleared_table_searches_like_a_fresh_one() {
    let table = Arc::new(TranspositionTable::new(1));
    let fresh = run_with_table(&table, KIWIPETE, 4);
    run_with_table(&table, KIWIPETE, 4);
    table.clear();
    let after_clear = run_with_table(&table, KIWIPETE, 4);
    assert_eq!(fresh.nodes, after_clear.nodes);
    assert_eq!(fresh.best_move, after_clear.best_move);
}

/// The position after `moves` from `fen`, and the keys of the positions
/// before it, oldest first: what the UCI layer hands the search.
fn game_after(fen: &str, moves: &[&str]) -> (Chess, Vec<u64>) {
    let mut position = pos(fen);
    let mut earlier_keys = Vec::new();
    for text in moves {
        let chess_move = text.parse::<UciMove>().unwrap().to_move(&position).unwrap();
        earlier_keys.push(position_key(&position));
        position.play_unchecked(chess_move);
    }
    (position, earlier_keys)
}

fn run_game(position: &Chess, earlier_keys: &[u64], depth: u32) -> SearchResult {
    let limits = Limits {
        depth: Some(depth),
        ..Limits::default()
    };
    Searcher::new(MaterialEval).search_with(
        position,
        earlier_keys,
        &limits,
        &AtomicBool::new(false),
        |_| {},
    )
}

/// White, a queen up, shuffled Kc1-b1-c1 while black went Kh8-g8. Black to
/// move: Kg8-h8 brings back the game's first position.
const SHUFFLE_FEN: &str = "7k/8/8/8/8/8/8/2KQ4 w - - 0 1";
const SHUFFLE_MOVES: [&str; 3] = ["c1b1", "h8g8", "b1c1"];

#[test]
fn the_losing_side_takes_a_repetition() {
    let (position, earlier_keys) = game_after(SHUFFLE_FEN, &SHUFFLE_MOVES);
    let result = run_game(&position, &earlier_keys, 4);
    assert_eq!(uci(result.best_move.unwrap()), "g8h8");
    assert_eq!(result.score, 0);
}

#[test]
fn without_the_game_the_same_move_is_no_draw() {
    let (position, _) = game_after(SHUFFLE_FEN, &SHUFFLE_MOVES);
    let result = run_game(&position, &[], 4);
    assert!(result.score < -500, "score {}", result.score);
}

#[test]
fn the_winning_side_avoids_a_repetition() {
    // White, a queen and a knight up, went Nb1-d2-b1 while black went
    // Kh8-g8-h8. Nb1-d2 is the move white picks among its equally winning
    // ones, and here it repeats the game's second position: white must play
    // something else and keep its winning score.
    let (position, earlier_keys) = game_after(
        "7k/8/8/8/8/8/8/1N1QK3 w - - 0 1",
        &["b1d2", "h8g8", "d2b1", "g8h8"],
    );
    let result = run_game(&position, &earlier_keys, 4);
    assert_ne!(uci(result.best_move.unwrap()), "b1d2");
    assert!(result.score > 500, "score {}", result.score);
    let without_the_game = run_game(&position, &[], 4);
    assert_eq!(uci(without_the_game.best_move.unwrap()), "b1d2");
}

#[test]
fn a_remembered_mate_does_not_end_the_search_before_it_is_proven_shortest() {
    // The second search finds the first one's mate in the table at depth 1
    // or 2. Stopping there trusts a mate the iteration could not have seen
    // whole: in a lichess game, such a remembered mate in 6 was played over a
    // mate in 3, walked back to an earlier position, and the game was drawn
    // by repetition.
    let table = Arc::new(TranspositionTable::new(16));
    let mate_in_three = "k7/8/8/3K4/8/8/8/7R w - - 0 1";
    run_with_table(&table, mate_in_three, 8);
    let again = run_with_table(&table, mate_in_three, 8);
    assert_eq!(again.score, MATE - 5);
    assert!(again.depth >= 5, "stopped at depth {}", again.depth);
}

/// A quiet, symmetrical middlegame (perft position 6): many quiet moves, so
/// the order of quiet moves shows in the node count.
const QUIET_MIDDLEGAME: &str =
    "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10";

/// The quiet middlegame at depth 6 with the material evaluation, before
/// killer moves and history, when quiet moves were ordered by piece alone.
///
/// A historical measurement, not a target: with killer moves the count fell
/// to 223,479, with history to 220,605 (2026-11-11). Re-measure these when
/// another change to the search moves them.
const BEFORE_QUIET_ORDERING_MIDDLEGAME_NODES: u64 = 282_981;

/// Killer moves and history together cut 22 % of that search. The test asks
/// for at least 15 %: enough margin that an unrelated change to the search
/// (an extension, a pruning rule) does not turn it red, while a search that
/// orders quiet moves by piece alone again fails it.
#[test]
fn quiet_move_ordering_shrinks_the_middlegame_search() {
    let nodes = run(QUIET_MIDDLEGAME, 6).nodes;
    let ceiling = BEFORE_QUIET_ORDERING_MIDDLEGAME_NODES * 85 / 100;
    assert!(nodes < ceiling, "{nodes} nodes, ceiling {ceiling}");
}

#[test]
fn killers_and_history_do_not_leak_between_searches() {
    let table = Arc::new(TranspositionTable::new(16));
    let limits = Limits {
        depth: Some(5),
        ..Limits::default()
    };
    let mut searcher = Searcher::with_table(MaterialEval, Arc::clone(&table));
    let first = searcher.search(&pos(QUIET_MIDDLEGAME), &limits, &AtomicBool::new(false));
    table.clear();
    let second = searcher.search(&pos(QUIET_MIDDLEGAME), &limits, &AtomicBool::new(false));
    assert_eq!(first.nodes, second.nodes);
}

#[test]
fn cutoff_statistics_are_counted() {
    let result = run(QUIET_MIDDLEGAME, 5);
    assert!(result.statistics.cutoffs > 0);
    assert!(result.statistics.first_move_cutoffs <= result.statistics.cutoffs);
    let rate = result.statistics.first_move_cutoff_rate();
    assert!(rate > 0.0 && rate <= 1.0, "{rate}");
}

#[test]
fn no_cutoffs_means_a_zero_rate() {
    assert!(SearchStatistics::default().first_move_cutoff_rate() == 0.0);
}

#[test]
fn zero_window_searches_are_counted() {
    let statistics = run(QUIET_MIDDLEGAME, 5).statistics;
    assert!(statistics.zero_window_searches > 0);
    assert!(statistics.zero_window_re_searches <= statistics.zero_window_searches);
}

/// A smoke test: with zero-window searches and their re-searches, the
/// reported line must still be a sequence of legal moves. It does not force
/// a re-search that fails low at a PV node; `search_after_move` keeps the
/// first move's line then because only a score above alpha updates the PV.
#[test]
fn the_principal_variation_is_a_legal_line() {
    let result = run(KIWIPETE, 5);
    assert!(!result.pv.is_empty());
    let mut position = pos(KIWIPETE);
    for &chess_move in &result.pv {
        assert!(
            position.legal_moves().contains(&chess_move),
            "{}",
            uci(chess_move)
        );
        position.play_unchecked(chess_move);
    }
}

#[test]
fn late_moves_are_reduced_and_some_re_searched() {
    let statistics = run(QUIET_MIDDLEGAME, 6).statistics;
    assert!(statistics.reduced_searches > 0);
    assert!(statistics.reduced_re_searches <= statistics.reduced_searches);
}

#[test]
fn reverse_futility_pruning_cuts_nodes_far_above_beta() {
    // Kiwipete is full of captures: many lines win material outright, and
    // the side that won it is cut on its evaluation a few plies further.
    let statistics = run(KIWIPETE, 5).statistics;
    assert!(statistics.reverse_futility_cutoffs > 0);
}

#[test]
fn quiescence_skips_captures_that_lose_material() {
    // Kiwipete's queens, rooks and minor pieces face many defended pawns.
    let statistics = run(KIWIPETE, 5).statistics;
    assert!(statistics.losing_captures_skipped > 0);
}

#[test]
fn shallow_nodes_skip_late_and_futile_quiet_moves() {
    let statistics = run(QUIET_MIDDLEGAME, 6).statistics;
    assert!(statistics.late_moves_pruned > 0);
    assert!(statistics.futile_moves_pruned > 0);
}

/// A smoke test: a search cut short by time, with reduced searches and
/// re-searches in flight, still returns a legal move. It cannot choose where
/// the stop lands; `search_after_move` returns 0 after every child search
/// that was aborted, and `search_with` drops the unfinished iteration.
#[test]
fn a_stopped_search_still_returns_a_legal_move() {
    let limits = Limits {
        movetime: Some(Duration::from_millis(20)),
        ..Limits::default()
    };
    let position = pos(QUIET_MIDDLEGAME);
    let result = Searcher::new(MaterialEval).search(&position, &limits, &AtomicBool::new(false));
    assert!(position.legal_moves().contains(&result.best_move.unwrap()));
}
