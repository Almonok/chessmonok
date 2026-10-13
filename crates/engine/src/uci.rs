use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use eval::MaterialEval;
use search::{Limits, MATE, MAX_PLY, SearchResult, Searcher};
use shakmaty::fen::Fen;
use shakmaty::uci::UciMove;
use shakmaty::{CastlingMode, Chess, Move, Position};

type Out<W> = Arc<Mutex<W>>;

pub fn run<R: BufRead, W: Write + Send + 'static>(input: R, output: W) {
    let mut engine = Engine::new(Arc::new(Mutex::new(output)));
    for line in input.lines() {
        let Ok(line) = line else { break };
        if !engine.handle(line.trim()) {
            engine.stop_search();
            return;
        }
    }
    engine.wait();
}

struct Engine<W> {
    out: Out<W>,
    pos: Chess,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    random_mover: bool,
}

impl<W: Write + Send + 'static> Engine<W> {
    fn new(out: Out<W>) -> Self {
        Self {
            out,
            pos: Chess::default(),
            stop: Arc::new(AtomicBool::new(false)),
            worker: None,
            random_mover: false,
        }
    }

    fn handle(&mut self, line: &str) -> bool {
        let mut tokens = line.split_whitespace();
        match tokens.next() {
            Some("uci") => {
                send(&self.out, "id name chessmonok");
                send(&self.out, "id author Theo Quemener");
                send(
                    &self.out,
                    "option name RandomMover type check default false",
                );
                send(&self.out, "uciok");
            }
            Some("isready") => send(&self.out, "readyok"),
            Some("ucinewgame") => {
                self.stop_search();
                self.pos = Chess::default();
            }
            Some("setoption") => self.set_option(&tokens.collect::<Vec<_>>()),
            Some("position") => {
                self.stop_search();
                match parse_position(&tokens.collect::<Vec<_>>()) {
                    Ok(pos) => self.pos = pos,
                    Err(e) => send(&self.out, &format!("info string {e}")),
                }
            }
            Some("go") => {
                self.stop_search();
                self.go(parse_go(&tokens.collect::<Vec<_>>()));
            }
            Some("stop") => self.stop_search(),
            Some("quit") => return false,
            _ => {}
        }
        true
    }

    fn set_option(&mut self, tokens: &[&str]) {
        if let ["name", "RandomMover", "value", value] = tokens {
            self.random_mover = value.eq_ignore_ascii_case("true");
        }
    }

    fn go(&mut self, limits: Limits) {
        self.stop = Arc::new(AtomicBool::new(false));
        let (out, pos, stop, random) = (
            Arc::clone(&self.out),
            self.pos.clone(),
            Arc::clone(&self.stop),
            self.random_mover,
        );
        self.worker = Some(thread::spawn(move || {
            let best = if random {
                random_move(&pos)
            } else {
                let start = Instant::now();
                Searcher::new(MaterialEval)
                    .search_with(&pos, &limits, &stop, |r| {
                        send(&out, &info_line(r, start.elapsed()));
                    })
                    .best_move
            };
            let text = best.map_or_else(
                || "0000".to_string(),
                |m| m.to_uci(CastlingMode::Standard).to_string(),
            );
            send(&out, &format!("bestmove {text}"));
        }));
    }

    fn stop_search(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.wait();
    }

    fn wait(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.join().expect("search thread panicked");
        }
    }
}

fn send<W: Write>(out: &Out<W>, line: &str) {
    let mut out = out.lock().expect("output lock");
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

fn parse_position(tokens: &[&str]) -> Result<Chess, String> {
    let split = tokens
        .iter()
        .position(|t| *t == "moves")
        .unwrap_or(tokens.len());
    let mut pos: Chess = match &tokens[..split] {
        ["startpos"] => Chess::default(),
        ["fen", fen @ ..] => fen
            .join(" ")
            .parse::<Fen>()
            .map_err(|e| e.to_string())?
            .into_position(CastlingMode::Standard)
            .map_err(|e| e.to_string())?,
        _ => return Err("expected startpos or fen".to_string()),
    };
    for text in tokens.get(split + 1..).unwrap_or(&[]) {
        let m = text
            .parse::<UciMove>()
            .map_err(|e| e.to_string())?
            .to_move(&pos)
            .map_err(|e| format!("{text}: {e}"))?;
        pos.play_unchecked(m);
    }
    Ok(pos)
}

fn parse_go(tokens: &[&str]) -> Limits {
    let mut limits = Limits::default();
    for pair in tokens.windows(2) {
        let ms = |s: &str| s.parse::<u64>().ok().map(Duration::from_millis);
        match pair {
            ["depth", d] => limits.depth = d.parse().ok(),
            ["movetime", t] => limits.movetime = ms(t),
            ["wtime", t] => limits.wtime = ms(t),
            ["btime", t] => limits.btime = ms(t),
            ["winc", t] => limits.winc = ms(t),
            ["binc", t] => limits.binc = ms(t),
            _ => {}
        }
    }
    limits
}

fn info_line(r: &SearchResult, elapsed: Duration) -> String {
    let mate_bound = MATE - i32::try_from(MAX_PLY).expect("MAX_PLY fits in i32");
    let score = if r.score.abs() >= mate_bound {
        let moves = (MATE - r.score.abs() + 1) / 2;
        format!("mate {}", if r.score > 0 { moves } else { -moves })
    } else {
        format!("cp {}", r.score)
    };
    let ms = elapsed.as_millis().max(1);
    let nps = u128::from(r.nodes) * 1000 / ms;
    let pv: Vec<String> =
        r.pv.iter()
            .map(|m| m.to_uci(CastlingMode::Standard).to_string())
            .collect();
    format!(
        "info depth {} score {score} nodes {} nps {nps} time {ms} pv {}",
        r.depth,
        r.nodes,
        pv.join(" ")
    )
}

fn random_move(pos: &Chess) -> Option<Move> {
    let moves = pos.legal_moves();
    if moves.is_empty() {
        return None;
    }
    // RandomState gets fresh random keys per instance; the clock on macOS only ticks in µs.
    let seed = RandomState::new().build_hasher().finish();
    Some(moves[(seed % moves.len() as u64) as usize])
}
