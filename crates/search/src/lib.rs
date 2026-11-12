//! Alpha-beta search for chessmonok.
//!
//! Iterative deepening runs a negamax alpha-beta search to depth 1, 2, 3...
//! until the time or depth limit, with a capture-only quiescence search at
//! the leaves. A transposition table (`transposition`), shared across
//! searches, remembers each node's result: it orders the remembered best move
//! first and ends the search of a node that was already searched deep enough.
//! Reverse futility pruning (`reverse_futility`) cuts shallow nodes whose
//! static evaluation is already far above beta, and null-move pruning
//! (`null_move`) nodes where even passing the turn keeps the side to move at
//! beta. A position that repeats an earlier one, in
//! the game or in the searched line, scores as a draw (`repetition`).
//!
//! Moves are searched best guess first (`ordering`): the remembered move,
//! then captures, then quiet moves that refuted a sibling position at the
//! same ply (`killers`), then the other quiet moves by how often they caused
//! cutoffs so far (`history`), and last the captures that static exchange
//! evaluation (`see`) shows losing material; quiescence skips those
//! altogether. `statistics` counts how often the first move tried was the
//! one that cut, the quick measure of that order.

mod history;
mod killers;
mod null_move;
mod ordering;
mod position_key;
mod reductions;
mod repetition;
mod reverse_futility;
mod see;
mod statistics;
mod transposition;

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use eval::{Evaluator, dirty_pieces};
use history::History;
use killers::Killers;
use shakmaty::{Chess, Color, Move, MoveList, Position};

pub use position_key::{key_after, position_key};
pub use statistics::SearchStatistics;
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
    pub statistics: SearchStatistics,
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
    /// Quiet moves that caused cutoffs, per ply; emptied by every search so
    /// that a search depends only on its position and limits.
    killers: Killers,
    /// How often each quiet move caused a cutoff, per side; emptied by every
    /// search, like `killers`.
    history: History,
    /// Shared, not owned: the engine keeps one table across `go` commands so
    /// each search starts from what the previous ones learnt.
    table: Arc<TranspositionTable>,
    nodes: u64,
    statistics: SearchStatistics,
    deadline: Option<Instant>,
    can_abort: bool,
    aborted: bool,
}

impl<E: Evaluator> fmt::Debug for Searcher<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Searcher")
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
            killers: Killers::new(),
            history: History::new(),
            table,
            nodes: 0,
            statistics: SearchStatistics::default(),
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
        self.statistics = SearchStatistics::default();
        self.states = vec![self.eval.refresh(pos); MAX_PLY + 1];
        self.keys = vec![0; MAX_PLY + 1];
        self.keys[0] = position_key(pos);
        self.game_keys = game_keys.to_vec();
        self.passed = vec![false; MAX_PLY + 1];
        self.pv = vec![Vec::new(); MAX_PLY + 1];
        self.killers = Killers::new();
        self.history = History::new();
        self.table.new_search();
        let mut best = SearchResult {
            best_move: None,
            score: 0,
            depth: 0,
            nodes: 0,
            pv: Vec::new(),
            statistics: SearchStatistics::default(),
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
                statistics: self.statistics,
            };
            on_depth(&best);
            let out_of_time = self.deadline.is_some_and(|d| Instant::now() >= d);
            if is_shortest_mate(score, depth) || out_of_time || stop.load(Ordering::Relaxed) {
                break;
            }
        }
        best.nodes = self.nodes;
        best.statistics = self.statistics;
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
        let in_check = pos.is_check();
        let static_evaluation = self.static_evaluation(pos, ply, in_check);
        if let Some(score) = self.reverse_futility_cutoff(depth, alpha, beta, static_evaluation) {
            return score;
        }
        if let Some(score) = self.null_move_cutoff(pos, depth, ply, beta, static_evaluation, stop) {
            return score;
        }
        // The pass search may have run out of time: nothing it saw is a result.
        if self.aborted {
            return 0;
        }
        let mut moves = pos.legal_moves();
        if moves.is_empty() {
            return if in_check { mated(ply) } else { 0 };
        }
        ordering::order_moves(
            &mut moves,
            pos,
            remembered.and_then(|entry| entry.best_move),
            self.killers.at(ply),
            &self.history,
        );
        let alpha_on_entry = alpha;
        let mut best = -INF;
        let mut best_move = None;
        let mut quiet_moves_tried = MoveList::new();
        for (move_index, &chess_move) in moves.iter().enumerate() {
            let child = self.play(pos, chess_move, ply);
            let score = self.search_after_move(
                &child, chess_move, depth, ply, alpha, beta, move_index, in_check, stop,
            );
            if self.aborted {
                // The scores of an unfinished subtree are not results: nothing
                // from here on reaches the table.
                return 0;
            }
            if score > best {
                best = score;
                if score > alpha {
                    alpha = score;
                    best_move = Some(chess_move);
                    self.set_pv(ply, chess_move);
                }
            }
            if alpha >= beta {
                self.statistics.record_cutoff(move_index == 0);
                if ordering::is_quiet(chess_move) {
                    self.learn_from_quiet_cutoff(
                        pos.turn(),
                        chess_move,
                        &quiet_moves_tried,
                        depth,
                        ply,
                    );
                }
                break;
            }
            if ordering::is_quiet(chess_move) {
                quiet_moves_tried.push(chess_move);
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

    /// The score of `chess_move`, which led to `child`, from the side to
    /// move at `ply`, searched as principal variation search (PVS) with
    /// late-move reductions does.
    ///
    /// The first move is searched with the full window: it is probably the
    /// best, and its exact score becomes `alpha`, the score to beat. Every
    /// later move only has to answer "is it better than `alpha`?", which a
    /// zero window `(alpha, alpha + 1)` answers far more cheaply, since the
    /// first refutation found settles it. A late quiet move asks it at a
    /// reduced depth first (`reductions`), and again at full depth if the
    /// answer is yes. Only a move still saying yes, and below `beta`, is
    /// searched again with the full window for its exact score. At a node
    /// that was itself given a zero window, `beta == alpha + 1`: a yes is
    /// already a cutoff, and never re-searched.
    #[allow(
        clippy::too_many_arguments,
        reason = "the move, the node's window and its place in the tree, as negamax has them"
    )]
    fn search_after_move(
        &mut self,
        child: &Chess,
        chess_move: Move,
        depth: u32,
        ply: usize,
        alpha: i32,
        beta: i32,
        move_index: usize,
        in_check: bool,
        stop: &AtomicBool,
    ) -> i32 {
        if move_index == 0 {
            return -self.negamax(child, depth - 1, ply + 1, -beta, -alpha, stop);
        }
        let reduction =
            if reductions::may_reduce(depth, move_index, chess_move, in_check, child.is_check()) {
                reductions::late_move_reduction(depth, move_index)
            } else {
                0
            };
        let mut score = self.zero_window_search(child, depth - 1 - reduction, ply, alpha, stop);
        if self.aborted {
            return 0;
        }
        if reduction > 0 {
            let surprised = score > alpha;
            self.statistics.record_reduced_search(surprised);
            if surprised {
                score = self.zero_window_search(child, depth - 1, ply, alpha, stop);
                if self.aborted {
                    return 0;
                }
            }
        }
        let beats_the_first_move = score > alpha && score < beta;
        self.statistics
            .record_zero_window_search(beats_the_first_move);
        if beats_the_first_move {
            return -self.negamax(child, depth - 1, ply + 1, -beta, -alpha, stop);
        }
        score
    }

    /// The score of `child` at `depth`, from the side to move at `ply`, as
    /// far as the zero window `(alpha, alpha + 1)` tells it: only whether it
    /// is above `alpha` is exact.
    fn zero_window_search(
        &mut self,
        child: &Chess,
        depth: u32,
        ply: usize,
        alpha: i32,
        stop: &AtomicBool,
    ) -> i32 {
        -self.negamax(child, depth, ply + 1, -alpha - 1, -alpha, stop)
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
        ordering::order_captures(&mut moves);
        for &chess_move in &moves {
            // Out of check the side to move may stand pat, so a capture
            // matters only if it can lift the score above the static
            // evaluation. One that loses material once the recaptures are
            // played out almost never does, and searching its recaptures
            // is where quiescence spends most of its nodes.
            if !in_check && see::static_exchange(pos, chess_move) < 0 {
                self.statistics.record_losing_capture_skipped();
                continue;
            }
            let child = self.play(pos, chess_move, ply);
            let score = -self.quiesce(&child, ply + 1, -beta, -alpha, stop);
            if self.aborted {
                return 0;
            }
            if score > best {
                best = score;
                if score > alpha {
                    alpha = score;
                    self.set_pv(ply, chess_move);
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

    fn play(&mut self, pos: &Chess, chess_move: Move, ply: usize) -> Chess {
        let dirty = dirty_pieces(pos, chess_move);
        let mut child = pos.clone();
        child.play_unchecked(chess_move);
        // The borrow checker cannot see that `ply` and `ply + 1` are two
        // different slots of one vector; `split_at_mut` proves it, safely,
        // by handing out the two sides of the cut as separate borrows.
        let (up_to_ply, after_ply) = self.states.split_at_mut(ply + 1);
        self.eval
            .update(&up_to_ply[ply], &mut after_ply[0], &child, &dirty);
        self.keys[ply + 1] = key_after(pos, self.keys[ply], chess_move, &child);
        self.passed[ply + 1] = false;
        child
    }

    /// The evaluation of the position at `ply` before any move, which both
    /// pruning steps compare to beta: evaluated once here rather than by
    /// each. `None` at the root, which is never pruned, and in check, where
    /// it would ignore that the king must be saved.
    fn static_evaluation(&self, pos: &Chess, ply: usize, in_check: bool) -> Option<i32> {
        (ply > 0 && !in_check).then(|| self.eval.evaluate(&self.states[ply], pos))
    }

    /// The score that ends this node before any move is searched, when its
    /// static evaluation is far enough above `beta` (`reverse_futility`).
    ///
    /// Only at zero-window nodes: at a principal-variation node the exact
    /// score and the line are wanted, not just "at least beta". The cutoff
    /// is not stored in the table: one evaluation finds it again, and an
    /// entry would claim a searched depth that was never searched.
    fn reverse_futility_cutoff(
        &mut self,
        depth: u32,
        alpha: i32,
        beta: i32,
        static_evaluation: Option<i32>,
    ) -> Option<i32> {
        let zero_window = beta - alpha == 1;
        if !zero_window {
            return None;
        }
        let score = reverse_futility::cutoff_score(depth, static_evaluation?, beta)?;
        self.statistics.record_reverse_futility_cutoff();
        Some(score)
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
        static_evaluation: Option<i32>,
        stop: &AtomicBool,
    ) -> Option<i32> {
        if !self.may_pass(pos, depth, ply, beta, static_evaluation?) {
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

    /// The guards of null-move pruning. The root and positions in check
    /// have no static evaluation, so the caller never asks for them.
    fn may_pass(
        &self,
        pos: &Chess,
        depth: u32,
        ply: usize,
        beta: i32,
        static_evaluation: i32,
    ) -> bool {
        depth >= null_move::MINIMUM_DEPTH
            && !self.passed[ply]
            && !is_mate(beta)
            && static_evaluation >= beta
            && null_move::has_pieces_besides_pawns(pos)
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

    /// The quiet move `cutting` reached beta: it becomes a killer at this
    /// ply and gains history, and the quiet moves tried before it, which did
    /// not cut, lose history.
    fn learn_from_quiet_cutoff(
        &mut self,
        side_to_move: Color,
        cutting: Move,
        quiet_moves_tried: &MoveList,
        remaining_depth: u32,
        ply: usize,
    ) {
        self.killers.record(ply, cutting);
        self.history.reward(side_to_move, cutting, remaining_depth);
        for &failed in quiet_moves_tried {
            self.history.penalise(side_to_move, failed, remaining_depth);
        }
    }

    fn set_pv(&mut self, ply: usize, chess_move: Move) {
        let mut line = vec![chess_move];
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
/// This cuts at every node below the root, PV nodes included. Engines with
/// principal variation search often cut only at zero-window ("non-PV")
/// nodes, which keeps the reported PV whole; chessmonok keeps cutting at PV
/// nodes for now, on purpose, so that PVS was measured on its own. Limiting
/// the cutoffs is a later step with its own measurement. The cost meanwhile
/// is that a cutoff ends the reported PV early at that node.
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

/// Whether `score`, from an iteration `depth` plies deep, is a mate found
/// within that iteration's own reach, where iterative deepening stops.
///
/// Before late-move reductions this was the shortest mate there is: the
/// iteration looked at every line up to `depth` plies. With reductions it
/// is not quite: a quiet first move tried late is searched shallower, so a
/// shorter mate starting with one can be missed while a longer one is found.
/// The mate found is still a proven mate (a reduced line that reaches one is
/// searched again at full depth), and its distance falls with every move
/// played, so the engine still mates; only a little later than it could.
///
/// A mate further away than `depth` came from the transposition table, where
/// an earlier search left it: a shorter one may be waiting a few iterations
/// deeper. Stopping on it played a mate in 6 that walked back into a
/// repetition, when a mate in 3 was on the board.
fn is_shortest_mate(score: i32, depth: u32) -> bool {
    let plies_to_mate = u32::try_from(MATE - score.abs()).unwrap_or(u32::MAX);
    is_mate(score) && plies_to_mate <= depth
}
