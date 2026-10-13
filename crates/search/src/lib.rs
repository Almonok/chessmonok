//! Placeholder alpha-beta for chessmonok: replaced piece by piece in the S exercises.

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use eval::{Evaluator, dirty_pieces};
use shakmaty::{Chess, Color, Move, MoveList, Position};

pub const MATE: i32 = 30_000;
pub const MAX_PLY: usize = 128;
const INF: i32 = 32_000;
const MAX_DEPTH: u32 = 64;

#[derive(Clone, Debug, Default)]
pub struct Limits {
    pub depth: Option<u32>,
    pub movetime: Option<Duration>,
    pub wtime: Option<Duration>,
    pub btime: Option<Duration>,
    pub winc: Option<Duration>,
    pub binc: Option<Duration>,
}

#[derive(Clone, Debug)]
pub struct SearchResult {
    pub best_move: Option<Move>,
    pub score: i32,
    pub depth: u32,
    pub nodes: u64,
    pub pv: Vec<Move>,
}

/// `movetime`, else a thirtieth of the clock plus half the increment, capped at a quarter.
pub fn time_budget(limits: &Limits, turn: Color) -> Option<Duration> {
    if limits.movetime.is_some() {
        return limits.movetime;
    }
    let (time, inc) = match turn {
        Color::White => (limits.wtime, limits.winc),
        Color::Black => (limits.btime, limits.binc),
    };
    let time = time?;
    Some((time / 30 + inc.unwrap_or_default() / 2).min(time / 4))
}

pub struct Searcher<E: Evaluator> {
    eval: E,
    states: Vec<E::State>,
    pv: Vec<Vec<Move>>,
    nodes: u64,
    deadline: Option<Instant>,
    can_abort: bool,
    aborted: bool,
}

impl<E: Evaluator> fmt::Debug for Searcher<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Searcher")
            .field("nodes", &self.nodes)
            .finish_non_exhaustive()
    }
}

impl<E: Evaluator> Searcher<E> {
    pub fn new(eval: E) -> Self {
        Self {
            eval,
            states: Vec::new(),
            pv: Vec::new(),
            nodes: 0,
            deadline: None,
            can_abort: false,
            aborted: false,
        }
    }

    pub fn search(&mut self, pos: &Chess, limits: &Limits, stop: &AtomicBool) -> SearchResult {
        self.search_with(pos, limits, stop, |_| {})
    }

    pub fn search_with(
        &mut self,
        pos: &Chess,
        limits: &Limits,
        stop: &AtomicBool,
        mut on_depth: impl FnMut(&SearchResult),
    ) -> SearchResult {
        self.deadline = time_budget(limits, pos.turn()).map(|budget| Instant::now() + budget);
        self.nodes = 0;
        self.states = vec![self.eval.refresh(pos); MAX_PLY + 1];
        self.pv = vec![Vec::new(); MAX_PLY + 1];
        let mut best = SearchResult {
            best_move: None,
            score: 0,
            depth: 0,
            nodes: 0,
            pv: Vec::new(),
        };
        if pos.legal_moves().is_empty() {
            best.score = if pos.is_check() { -MATE } else { 0 };
            return best;
        }
        for depth in 1..=limits.depth.unwrap_or(MAX_DEPTH) {
            self.can_abort = depth > 1;
            self.aborted = false;
            let score = self.negamax(pos, depth, 0, -INF, INF, stop);
            if self.aborted {
                break;
            }
            best = SearchResult {
                best_move: self.pv[0].first().copied(),
                score,
                depth,
                nodes: self.nodes,
                pv: self.pv[0].clone(),
            };
            on_depth(&best);
            let out_of_time = self.deadline.is_some_and(|d| Instant::now() >= d);
            if is_mate(score) || out_of_time || stop.load(Ordering::Relaxed) {
                break;
            }
        }
        best.nodes = self.nodes;
        best
    }

    fn negamax(
        &mut self,
        pos: &Chess,
        depth: u32,
        ply: usize,
        mut alpha: i32,
        beta: i32,
        stop: &AtomicBool,
    ) -> i32 {
        self.pv[ply].clear();
        if depth == 0 || ply >= MAX_PLY {
            return self.quiesce(pos, ply, alpha, beta, stop);
        }
        if self.should_stop(stop) {
            self.aborted = true;
            return 0;
        }
        self.nodes += 1;
        if ply > 0 && (pos.halfmoves() >= 100 || pos.is_insufficient_material()) {
            return 0;
        }
        let mut moves = pos.legal_moves();
        if moves.is_empty() {
            return if pos.is_check() { mated(ply) } else { 0 };
        }
        order(&mut moves);
        let mut best = -INF;
        for &m in &moves {
            let child = self.play(pos, m, ply);
            let score = -self.negamax(&child, depth - 1, ply + 1, -beta, -alpha, stop);
            if self.aborted {
                return 0;
            }
            if score > best {
                best = score;
                if score > alpha {
                    alpha = score;
                    self.set_pv(ply, m);
                }
            }
            if alpha >= beta {
                break;
            }
        }
        best
    }

    fn quiesce(
        &mut self,
        pos: &Chess,
        ply: usize,
        mut alpha: i32,
        beta: i32,
        stop: &AtomicBool,
    ) -> i32 {
        self.pv[ply].clear();
        if self.should_stop(stop) {
            self.aborted = true;
            return 0;
        }
        self.nodes += 1;
        if ply >= MAX_PLY {
            return self.eval.evaluate(&self.states[ply], pos);
        }
        let in_check = pos.is_check();
        let mut moves = if in_check {
            pos.legal_moves()
        } else {
            pos.capture_moves()
        };
        if in_check && moves.is_empty() {
            return mated(ply);
        }
        let mut best = -INF;
        if !in_check {
            let stand = self.eval.evaluate(&self.states[ply], pos);
            if stand >= beta {
                return stand;
            }
            alpha = alpha.max(stand);
            best = stand;
        }
        order(&mut moves);
        for &m in &moves {
            let child = self.play(pos, m, ply);
            let score = -self.quiesce(&child, ply + 1, -beta, -alpha, stop);
            if self.aborted {
                return 0;
            }
            if score > best {
                best = score;
                if score > alpha {
                    alpha = score;
                    self.set_pv(ply, m);
                }
            }
            if alpha >= beta {
                break;
            }
        }
        best
    }

    fn play(&mut self, pos: &Chess, m: Move, ply: usize) -> Chess {
        let dirty = dirty_pieces(pos, m);
        let mut child = pos.clone();
        child.play_unchecked(m);
        self.states[ply + 1] = self.eval.update(&self.states[ply], &child, &dirty);
        child
    }

    fn set_pv(&mut self, ply: usize, m: Move) {
        let mut line = vec![m];
        line.extend_from_slice(&self.pv[ply + 1]);
        self.pv[ply] = line;
    }

    fn should_stop(&self, stop: &AtomicBool) -> bool {
        self.can_abort
            && self.nodes % 1024 == 0
            && (stop.load(Ordering::Relaxed) || self.deadline.is_some_and(|d| Instant::now() >= d))
    }
}

fn mated(ply: usize) -> i32 {
    -MATE + i32::try_from(ply).expect("ply fits in i32")
}

fn is_mate(score: i32) -> bool {
    score.abs() >= MATE - i32::try_from(MAX_PLY).expect("MAX_PLY fits in i32")
}

fn order(moves: &mut MoveList) {
    moves.sort_by_key(|m| {
        let victim = m.capture().map_or(0, |r| r as i32 * 10);
        let promotion = m.promotion().map_or(0, |r| r as i32 * 10);
        -(victim + promotion - m.role() as i32)
    });
}
