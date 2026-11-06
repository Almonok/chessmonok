//! Alpha-beta search for chessmonok.
//!
//! Iterative deepening runs a negamax alpha-beta search to depth 1, 2, 3...
//! until the time or depth limit, with a capture-only quiescence search at
//! the leaves. A transposition table (`transposition`), shared across
//! searches, remembers each node's result: it orders the remembered best move
//! first and ends the search of a node that was already searched deep enough.
//! Null-move pruning (`null_move`) cuts nodes where even passing the turn
//! keeps the side to move at beta. A position that repeats an earlier one, in
//! the game or in the searched line, scores as a draw (`repetition`).

mod null_move;
mod position_key;
mod repetition;
mod transposition;

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use eval::{Evaluator, dirty_pieces};
use shakmaty::{Chess, Color, Move, MoveList, Position};

pub use position_key::{key_after, position_key};
pub use transposition::{
    Bound, DEFAULT_HASH_MEGABYTES, MAX_HASH_MEGABYTES, PackedMove, TableEntry, TranspositionTable,
    score_from_table, score_to_table,
};

pub const MATE: i32 = 30_000;
pub const MAX_PLY: usize = 128;
const INF: i32 = 32_000;
const MAX_DEPTH: u32 = 64;

/// Scores at least this far from zero are mates: the search never goes deeper
/// than `MAX_PLY`, so a mate is never further than that from `MATE`.
#[allow(
    clippy::cast_possible_wrap,
    reason = "MAX_PLY is 128, far below i32::MAX"
)]
const MATE_BOUND: i32 = MATE - MAX_PLY as i32;

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
    /// The Zobrist key of the position at each ply of the current line, kept
    /// up to date move by move like `states`.
    keys: Vec<u64>,
    /// The keys of the game's positions before the root, oldest first: the
    /// search line can repeat them too.
    game_keys: Vec<u64>,
    /// Whether the move into each ply of the current line was a pass: a pass
    /// is never answered by another pass.
    passed: Vec<bool>,
    pv: Vec<Vec<Move>>,
    /// Shared, not owned: the engine keeps one table across `go` commands so
    /// each search starts from what the previous ones learnt.
    table: Arc<TranspositionTable>,
    nodes: u64,
    deadline: Option<Instant>,
    can_abort: bool,
    aborted: bool,
}

impl<E: Evaluator> fmt::Debug for Searcher<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Searcher")
            .field("nodes", &self.nodes)
            .field("table", &self.table)
            .finish_non_exhaustive()
    }
}

impl<E: Evaluator> Searcher<E> {
    /// A searcher with its own, empty table of the default size.
    pub fn new(eval: E) -> Self {
        Self::with_table(
            eval,
            Arc::new(TranspositionTable::new(DEFAULT_HASH_MEGABYTES)),
        )
    }

    /// A searcher that reads and writes `table`, which outlives it.
    pub fn with_table(eval: E, table: Arc<TranspositionTable>) -> Self {
        Self {
            eval,
            states: Vec::new(),
            keys: Vec::new(),
            game_keys: Vec::new(),
            passed: Vec::new(),
            pv: Vec::new(),
            table,
            nodes: 0,
            deadline: None,
            can_abort: false,
            aborted: false,
        }
    }

    /// Search `pos` as if the game started there: no earlier position can be
    /// repeated.
    pub fn search(&mut self, pos: &Chess, limits: &Limits, stop: &AtomicBool) -> SearchResult {
        self.search_with(pos, &[], limits, stop, |_| {})
    }

    /// Search `pos`, reached through the positions whose keys are
    /// `game_keys` (oldest first, `pos` itself excluded), calling `on_depth`
    /// after each finished iteration.
    pub fn search_with(
        &mut self,
        pos: &Chess,
        game_keys: &[u64],
        limits: &Limits,
        stop: &AtomicBool,
        mut on_depth: impl FnMut(&SearchResult),
    ) -> SearchResult {
        self.deadline = time_budget(limits, pos.turn()).map(|budget| Instant::now() + budget);
        self.nodes = 0;
        self.states = vec![self.eval.refresh(pos); MAX_PLY + 1];
        self.keys = vec![0; MAX_PLY + 1];
        self.keys[0] = position_key(pos);
        self.game_keys = game_keys.to_vec();
        self.passed = vec![false; MAX_PLY + 1];
        self.pv = vec![Vec::new(); MAX_PLY + 1];
        self.table.new_search();
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
            if is_shortest_mate(score, depth) || out_of_time || stop.load(Ordering::Relaxed) {
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
        // Before the table: a repetition is a draw only on this path, so the
        // table's score for the position, reached another way, must not
        // hide it.
        if ply > 0 && self.is_draw(pos, ply) {
            return 0;
        }
        let key = self.keys[ply];
        let remembered = self.table.probe(key);
        // Never at the root: the root must produce a move and a PV, which a
        // stored score alone does not give.
        if ply > 0 {
            if let Some(score) =
                remembered.and_then(|entry| table_cutoff(entry, depth, ply, alpha, beta))
            {
                return score;
            }
        }
        if let Some(score) = self.null_move_cutoff(pos, depth, ply, beta, stop) {
            return score;
        }
        // The pass search may have run out of time: nothing it saw is a result.
        if self.aborted {
            return 0;
        }
        let mut moves = pos.legal_moves();
        if moves.is_empty() {
            return if pos.is_check() { mated(ply) } else { 0 };
        }
        order(&mut moves, remembered.and_then(|entry| entry.best_move));
        let alpha_on_entry = alpha;
        let mut best = -INF;
        let mut best_move = None;
        for &m in &moves {
            let child = self.play(pos, m, ply);
            let score = -self.negamax(&child, depth - 1, ply + 1, -beta, -alpha, stop);
            if self.aborted {
                // The scores of an unfinished subtree are not results: nothing
                // from here on reaches the table.
                return 0;
            }
            if score > best {
                best = score;
                if score > alpha {
                    alpha = score;
                    best_move = Some(m);
                    self.set_pv(ply, m);
                }
            }
            if alpha >= beta {
                break;
            }
        }
        self.table.store(
            key,
            TableEntry {
                best_move: best_move.map(PackedMove::new),
                score: score_to_table(best, ply),
                depth,
                bound: bound_of(best, alpha_on_entry, beta),
            },
        );
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
        order(&mut moves, None);
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

    /// Whether the position at `ply` is drawn whatever is played next: the
    /// fifty-move rule, mating material gone, or a repetition.
    fn is_draw(&self, pos: &Chess, ply: usize) -> bool {
        pos.halfmoves() >= 100
            || pos.is_insufficient_material()
            || repetition::repeats_an_earlier_position(
                &self.game_keys,
                &self.keys[..=ply],
                &self.passed[..=ply],
                pos.halfmoves() as usize,
            )
    }

    fn play(&mut self, pos: &Chess, m: Move, ply: usize) -> Chess {
        let dirty = dirty_pieces(pos, m);
        let mut child = pos.clone();
        child.play_unchecked(m);
        self.states[ply + 1] = self.eval.update(&self.states[ply], &child, &dirty);
        self.keys[ply + 1] = key_after(pos, self.keys[ply], m, &child);
        self.passed[ply + 1] = false;
        child
    }

    /// The score that ends this node through a pass, if one is allowed and
    /// the passed position, searched shallower, still reaches `beta`.
    ///
    /// The cutoff is remembered as a lower bound. Without it, the next search
    /// of this node (next iteration, next `go`) must redo the pass search: a
    /// warm table then saved less than half the work on Kiwipete instead of
    /// three quarters.
    fn null_move_cutoff(
        &mut self,
        pos: &Chess,
        depth: u32,
        ply: usize,
        beta: i32,
        stop: &AtomicBool,
    ) -> Option<i32> {
        if !self.may_pass(pos, depth, ply, beta) {
            return None;
        }
        let passed_position = self.play_pass(pos, ply)?;
        let reduced_depth = null_move::reduced_depth(depth);
        let score = -self.negamax(
            &passed_position,
            reduced_depth,
            ply + 1,
            -beta,
            -beta + 1,
            stop,
        );
        if self.aborted || score < beta {
            return None;
        }
        // The real moves were never searched, so a mate seen through the pass
        // is not proven: report only that the node reaches beta.
        let score = if is_mate(score) { beta } else { score };
        self.table.store(
            self.keys[ply],
            TableEntry {
                best_move: None,
                score: score_to_table(score, ply),
                depth,
                bound: Bound::Lower,
            },
        );
        Some(score)
    }

    /// The guards of null-move pruning, cheapest first; the static evaluation
    /// comes last because it is the only costly one.
    fn may_pass(&self, pos: &Chess, depth: u32, ply: usize, beta: i32) -> bool {
        ply > 0
            && depth >= null_move::MINIMUM_DEPTH
            && !self.passed[ply]
            && !is_mate(beta)
            && !pos.is_check()
            && null_move::has_pieces_besides_pawns(pos)
            && self.eval.evaluate(&self.states[ply], pos) >= beta
    }

    /// Like `play`, for a pass. No piece moves, so the evaluation state is
    /// carried over unchanged; the key is recomputed because the en-passant
    /// right may vanish.
    fn play_pass(&mut self, pos: &Chess, ply: usize) -> Option<Chess> {
        let child = null_move::pass(pos)?;
        self.states[ply + 1] = self.states[ply].clone();
        self.keys[ply + 1] = position_key(&child);
        self.passed[ply + 1] = true;
        Some(child)
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

/// The score a remembered entry lets this node return without searching, if
/// it was searched at least `depth` deep and its bound settles the window.
///
/// This cuts at every node below the root, PV nodes included. Plain
/// alpha-beta searches nearly every node with a wide window, so restricting
/// cutoffs to zero-window ("non-PV") nodes, as engines with principal
/// variation search do, would leave almost nothing to cut. The cost is that a
/// cutoff ends the reported PV early at that node.
fn table_cutoff(entry: TableEntry, depth: u32, ply: usize, alpha: i32, beta: i32) -> Option<i32> {
    if entry.depth < depth {
        return None;
    }
    let score = score_from_table(entry.score, ply);
    let settled = match entry.bound {
        Bound::Exact => true,
        Bound::Lower => score >= beta,
        Bound::Upper => score <= alpha,
    };
    settled.then_some(score)
}

/// What `best` says about the true value, given the window it was searched
/// with (see the diagram in `transposition`).
fn bound_of(best: i32, alpha_on_entry: i32, beta: i32) -> Bound {
    if best >= beta {
        Bound::Lower
    } else if best > alpha_on_entry {
        Bound::Exact
    } else {
        Bound::Upper
    }
}

/// A ply count as a score offset: mate scores move by one per ply.
fn ply_offset(ply: usize) -> i32 {
    i32::try_from(ply).expect("ply fits in i32")
}

fn mated(ply: usize) -> i32 {
    -MATE + ply_offset(ply)
}

fn is_mate(score: i32) -> bool {
    score.abs() >= MATE_BOUND
}

/// Whether `score`, from an iteration `depth` plies deep, is a mate no deeper
/// iteration can shorten: the iteration looked at every line up to `depth`
/// plies, so a mate within that distance is the shortest there is.
///
/// A mate further away than `depth` came from the transposition table, where
/// an earlier search left it: a shorter one may be waiting a few iterations
/// deeper. Stopping on it played a mate in 6 that walked back into a
/// repetition, when a mate in 3 was on the board.
fn is_shortest_mate(score: i32, depth: u32) -> bool {
    let plies_to_mate = u32::try_from(MATE - score.abs()).unwrap_or(u32::MAX);
    is_mate(score) && plies_to_mate <= depth
}

/// The remembered best move first, then captures by most valuable victim and
/// least valuable attacker (MVV-LVA), promotions among them.
fn order(moves: &mut MoveList, remembered: Option<PackedMove>) {
    moves.sort_by_key(|m| {
        let victim = m.capture().map_or(0, |r| r as i32 * 10);
        let promotion = m.promotion().map_or(0, |r| r as i32 * 10);
        -(victim + promotion - m.role() as i32)
    });
    // The packed move is only a hint (it could come from another position
    // that shares the key): it is used only if it is one of the legal moves.
    let remembered_index = remembered.and_then(|remembered| {
        moves
            .iter()
            .position(|&candidate| PackedMove::new(candidate) == remembered)
    });
    if let Some(index) = remembered_index {
        moves[..=index].rotate_right(1);
    }
}
