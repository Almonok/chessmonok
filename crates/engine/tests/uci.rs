use std::io::Write;
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
    assert!(!out.contains("info string"), "{out}");
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
