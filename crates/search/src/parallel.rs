//! Lazy SMP: several threads search the same position, sharing one
//! transposition table.
//!
//! The obvious way to use several cores, splitting the tree and giving each
//! thread its own subtrees, is hard: alpha-beta's tree is lopsided and only
//! known as it is searched, so the split must be renegotiated all the time.
//! Lazy SMP ("symmetric multiprocessing", done lazily) skips all of that.
//! Every thread runs the whole iterative-deepening search on the same root,
//! and the threads never talk to each other, except through the table:
//!
//! ```text
//!   main     depth 1 2 3 ... 14 ──── decides when to stop, plays its move
//!   helper   depth 1 2 3 ... 14 ───┐
//!   helper   depth 1 2 3 ... 14 ───┤ searching until told to stop
//!            └──── one shared transposition table ────┘
//! ```
//!
//! It works because the threads do not stay in step. One that reaches a
//! subtree first stores its result, and the others, arriving later, cut on
//! it or try its best move first; their move orders then drift apart
//! (killers and history are per thread), so they explore different parts
//! of the tree and fill the table for each other. Each thread is about as
//! fast as one alone, and the main thread reaches a given depth sooner
//! because much of what it needs was already searched.
//!
//! The main thread is the single-threaded search, unchanged: it keeps the
//! clock, reports each finished depth and gives the move played. Helpers
//! have no clock and no depth limit; they stop when the main thread is done.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use eval::Evaluator;
use shakmaty::Chess;

use crate::{Limits, SearchResult, Searcher, TranspositionTable};

/// The most search threads the engine accepts (the UCI `Threads` option).
pub const MAX_THREADS: usize = 64;

/// Search `pos` with `threads` threads (at least the main one), as
/// `Searcher::search_with` does with one: the same arguments, and the main
/// thread's result, its node count raised by every helper's.
///
/// `std::thread::scope` lets the helpers borrow `pos`, `game_keys` and the
/// stop flag from this function's stack: the scope does not end until every
/// thread spawned in it has finished, so the borrows cannot outlive what
/// they point to. With `std::thread::spawn` each thread would need its own
/// copy, or an `Arc`, of all of it.
#[allow(
    clippy::too_many_arguments,
    reason = "search_with's arguments, plus the evaluator, table and thread count it is given"
)]
pub fn search_in_parallel<E>(
    evaluator: E,
    table: &Arc<TranspositionTable>,
    threads: usize,
    pos: &Chess,
    game_keys: &[u64],
    limits: &Limits,
    stop: &AtomicBool,
    on_depth: impl FnMut(&SearchResult),
) -> SearchResult
where
    E: Evaluator + Clone + Send,
{
    // Once for all the threads of this search: see `Searcher::deepen`.
    table.new_search();
    let main_thread_done = AtomicBool::new(false);
    thread::scope(|scope| {
        let helpers: Vec<_> = (1..threads.max(1))
            .map(|_| {
                let evaluator = evaluator.clone();
                let table = Arc::clone(table);
                let main_thread_done = &main_thread_done;
                scope.spawn(move || {
                    Searcher::with_table(evaluator, table)
                        .deepen(pos, game_keys, &Limits::default(), main_thread_done, |_| {})
                        .nodes
                })
            })
            .collect();
        let mut result = Searcher::with_table(evaluator, Arc::clone(table))
            .deepen(pos, game_keys, limits, stop, on_depth);
        main_thread_done.store(true, Ordering::Relaxed);
        let helper_nodes: u64 = helpers
            .into_iter()
            .map(|helper| helper.join().expect("a search thread panicked"))
            .sum();
        result.nodes += helper_nodes;
        result
    })
}
