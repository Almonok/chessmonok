use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use shakmaty::uci::UciMove;
use shakmaty::{Chess, Position};

#[derive(Clone, Default)]
struct Sink(Arc<Mutex<Vec<u8>>>);

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn session(input: &str) -> String {
    let sink = Sink::default();
    engine::uci::run(input.as_bytes(), sink.clone());
    String::from_utf8(sink.0.lock().unwrap().clone()).unwrap()
}

fn bestmove(out: &str) -> &str {
    out.lines()
        .find_map(|l| l.strip_prefix("bestmove "))
        .expect("a bestmove line")
}

fn is_legal(pos: &Chess, uci: &str) -> bool {
    uci.parse::<UciMove>().is_ok_and(|m| m.to_move(pos).is_ok())
}

#[test]
fn handshake() {
    let out = session("uci\nisready\nquit\n");
    let id = out.find("id name chessmonok").unwrap();
    let ok = out.find("uciok").unwrap();
    let ready = out.find("readyok").unwrap();
    assert!(id < ok && ok < ready);
    assert!(out.contains("option name RandomMover type check default false"));
    assert!(out.contains("option name Hash type spin default 16 min 1 max 1024"));
}

#[test]
fn searches_with_a_resized_table_and_reports_its_fill() {
    let out = session("setoption name Hash value 1\nposition startpos\ngo depth 4\n");
    assert!(!out.contains("info string Hash"), "{out}");
    let deepest = out
        .lines()
        .find(|line| line.starts_with("info depth 4 "))
        .expect("an info line for depth 4");
    assert!(deepest.contains(" hashfull "), "{deepest}");
    assert!(is_legal(&Chess::default(), bestmove(&out)));
}

#[test]
fn a_hash_that_is_not_a_number_is_reported() {
    let out = session("setoption name Hash value lots\nposition startpos\ngo depth 1\n");
    assert!(
        out.contains("info string Hash: not a number: lots"),
        "{out}"
    );
    assert!(is_legal(&Chess::default(), bestmove(&out)));
}

#[test]
fn searches_to_the_requested_depth() {
    let out = session("position startpos moves e2e4\ngo depth 3\n");
    assert!(out.contains("info depth 3 "));
    let mut pos = Chess::default();
    let e4 = "e2e4".parse::<UciMove>().unwrap().to_move(&pos).unwrap();
    pos.play_unchecked(e4);
    assert!(is_legal(&pos, bestmove(&out)), "{out}");
}

#[test]
fn reports_mate_scores() {
    let out = session("position fen 6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1\ngo depth 3\n");
    assert_eq!(bestmove(&out), "a1a8");
    assert!(out.contains("score mate 1"));
}

#[test]
fn the_moves_played_reach_the_search_as_repetitions() {
    // White, well ahead, went Nb1-d2-b1: playing Nb1-d2 again, its choice
    // without the game, would repeat a position of the game.
    let out = session(
        "position fen 7k/8/8/8/8/8/8/1N1QK3 w - - 0 1 moves b1d2 h8g8 d2b1 g8h8\ngo depth 4\n",
    );
    assert_ne!(bestmove(&out), "b1d2", "{out}");
    let without_the_game = session("position fen 7k/8/8/8/8/8/8/1N1QK3 w - - 4 3\ngo depth 4\n");
    assert_eq!(bestmove(&without_the_game), "b1d2", "{without_the_game}");
}

#[test]
fn mated_position_prints_null_move() {
    let out = session("position fen R5k1/5ppp/8/8/8/8/8/6K1 b - - 0 1\ngo depth 2\n");
    assert_eq!(bestmove(&out), "0000");
}

#[test]
fn stop_ends_an_infinite_search() {
    let out = session("position startpos\ngo infinite\nstop\n");
    assert!(is_legal(&Chess::default(), bestmove(&out)));
}

#[test]
fn illegal_move_keeps_position() {
    let out = session("position startpos moves e2e5\ngo depth 1\n");
    assert!(out.contains("info string"));
    assert!(is_legal(&Chess::default(), bestmove(&out)));
}

#[test]
fn castling_move_is_accepted() {
    let out = session("position fen r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1 moves e1g1\ngo depth 1\n");
    let complaints = out
        .lines()
        .filter(|line| line.starts_with("info string") && !line.starts_with("info string eval "));
    assert_eq!(complaints.count(), 0, "{out}");
}

#[test]
fn random_mover_plays_a_legal_move_without_searching() {
    let out = session("setoption name RandomMover value true\nposition startpos\ngo depth 5\n");
    assert!(!out.contains("info depth"));
    assert!(is_legal(&Chess::default(), bestmove(&out)));
}

#[test]
fn short_clock_returns_quickly() {
    let start = Instant::now();
    let out = session("position startpos\ngo wtime 1000 btime 1000\n");
    assert!(start.elapsed() < Duration::from_millis(800));
    assert!(is_legal(&Chess::default(), bestmove(&out)));
}

#[test]
fn ucinewgame_resets_the_position() {
    let out = session("position startpos moves e2e4\nucinewgame\ngo depth 1\n");
    assert!(is_legal(&Chess::default(), bestmove(&out)));
}

#[test]
fn random_mover_varies_its_moves() {
    let moves: std::collections::HashSet<String> = (0..100)
        .map(|_| {
            let out =
                session("setoption name RandomMover value true\nposition startpos\ngo depth 1\n");
            bestmove(&out).to_string()
        })
        .collect();
    assert!(
        moves.len() >= 8,
        "only {} distinct moves: {moves:?}",
        moves.len()
    );
}

/// A random net written to a temporary file whose name contains a space.
/// The file (21 MB) is deleted when this value is dropped.
struct TemporaryNet {
    path: PathBuf,
}

impl TemporaryNet {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("chessmonok {name} {}.nnue", std::process::id()));
        std::fs::write(&path, nnue::Network::random(5).to_bytes()).unwrap();
        TemporaryNet { path }
    }

    fn path(&self) -> String {
        self.path.display().to_string()
    }
}

impl Drop for TemporaryNet {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[test]
fn temporary_nets_are_deleted_when_dropped() {
    let net = TemporaryNet::new("dropped");
    let path = net.path.clone();
    assert!(path.exists());
    drop(net);
    assert!(!path.exists());
}

#[test]
fn the_handshake_lists_eval_file() {
    let out = session("uci\nquit\n");
    assert!(
        out.contains("option name EvalFile type string default <empty>"),
        "{out}"
    );
}

#[test]
fn a_valid_eval_file_is_loaded_and_used() {
    let net = TemporaryNet::new("valid");
    let out = session(&format!(
        "setoption name EvalFile value {}\nposition startpos\ngo depth 2\n",
        net.path()
    ));
    assert!(
        out.contains(&format!("info string loaded {}", net.path())),
        "{out}"
    );
    assert!(out.contains("info string eval nnue"), "{out}");
    assert!(is_legal(&Chess::default(), bestmove(&out)), "{out}");
}

#[test]
fn eval_file_paths_may_contain_spaces() {
    let net = TemporaryNet::new("with  two spaces");
    let out = session(&format!("setoption name EvalFile value {}\n", net.path()));
    assert!(out.contains("info string loaded"), "{out}");
}

#[test]
fn a_broken_eval_file_stops_the_engine_from_searching() {
    let out =
        session("setoption name EvalFile value /no/such/net.nnue\nposition startpos\ngo depth 2\n");
    assert!(out.contains("info string EvalFile error:"), "{out}");
    assert!(
        out.contains("info string cannot search: EvalFile error:"),
        "{out}"
    );
    assert_eq!(bestmove(&out), "0000");
    assert!(!out.contains("info depth"), "no search may run: {out}");
}

#[test]
fn extra_spaces_before_the_value_are_ignored() {
    let net = TemporaryNet::new("extra space");
    let out = session(&format!("setoption name EvalFile value   {}\n", net.path()));
    assert!(out.contains("info string loaded"), "{out}");
}

#[test]
fn the_advertised_default_means_material() {
    // The handshake says `default <empty>`; a GUI may send that back.
    let out = session("setoption name EvalFile value <empty>\nposition startpos\ngo depth 2\n");
    assert!(!out.contains("EvalFile error"), "{out}");
    assert!(out.contains("info string eval material"), "{out}");
}

#[test]
fn an_empty_eval_file_means_material() {
    let out = session(
        "setoption name EvalFile value /no/such/net.nnue\nsetoption name EvalFile value\nposition startpos\ngo depth 2\n",
    );
    assert!(out.contains("info string eval material"), "{out}");
    assert!(is_legal(&Chess::default(), bestmove(&out)), "{out}");
}

/// The node total of a `bench` run.
fn bench_nodes(out: &str) -> u64 {
    let line = out
        .lines()
        .find(|line| line.starts_with("info string bench nodes "))
        .unwrap_or_else(|| panic!("no bench line in {out}"));
    line.split_whitespace().nth(4).unwrap().parse().unwrap()
}

#[test]
fn bench_reports_nodes_and_is_deterministic() {
    let first = bench_nodes(&session("bench 3\n"));
    let second = bench_nodes(&session("bench 3\n"));
    assert!(first > 0);
    assert_eq!(first, second, "the node count is a signature of the search");
}

#[test]
fn bench_reports_first_move_cutoffs() {
    let out = session("bench 3\n");
    let line = out
        .lines()
        .find(|line| line.starts_with("info string bench cutoffs "))
        .unwrap_or_else(|| panic!("no cutoff line in {out}"));
    assert!(line.contains(" first-move "), "{line}");
}

#[test]
fn bench_reports_zero_window_re_searches() {
    let out = session("bench 3\n");
    let line = out
        .lines()
        .find(|line| line.starts_with("info string bench zero-window "))
        .unwrap_or_else(|| panic!("no zero-window line in {out}"));
    assert!(line.contains(" re-searched "), "{line}");
}

#[test]
fn bench_reports_reductions() {
    let out = session("bench 3\n");
    assert!(
        out.lines()
            .any(|line| line.starts_with("info string bench reduced ")),
        "{out}"
    );
}

#[test]
fn bench_reports_reverse_futility_cutoffs() {
    let out = session("bench 3\n");
    assert!(
        out.lines()
            .any(|line| line.starts_with("info string bench reverse-futility cutoffs ")),
        "{out}"
    );
}
