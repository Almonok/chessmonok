//! Time management: how long to think about one move, given the clock.
//!
//! Iterative deepening only produces a result when an iteration finishes:
//! a search aborted halfway through depth 12 still plays depth 11's move,
//! and the time spent on depth 12 is lost. A single deadline that aborts
//! the search wastes, on average, a large share of every move's time that
//! way (a quarter of it at 5+0.05 before this module).
//!
//! So there are two limits:
//!
//! ```text
//! soft limit   checked between iterations: no new depth is started past it
//! hard limit   checked inside the search: abort, the safety net
//! ```
//!
//! Each iteration costs about twice the time of all the ones before it, so
//! an iteration started before the soft limit usually ends before twice the
//! soft limit; the hard limit is set above that, and only an iteration that
//! blows up (a score falling, re-searches) is cut by it.
//!
//! The limits are scaled from the old single deadline, the *target*: a
//! thirtieth of the clock (about thirty moves still to play) plus half the
//! increment. Stopping at the first finished iteration past a soft limit
//! `S` spends between `S` and `2S`, `S / ln 2 = 1.44 S` on average; a soft
//! limit of 0.7 x target therefore spends about the target on average, the
//! same as before, without the abandoned iterations.
//!
//! `movetime` asks for a fixed time per move: both limits are that time.

use std::time::Duration;

use shakmaty::Color;

use crate::Limits;

/// The thinking time for one move: no new iteration past `soft`, an abort
/// past `hard`. `soft` is never above `hard`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeLimits {
    pub soft: Duration,
    pub hard: Duration,
}

/// About how many moves the clock is shared between.
const MOVES_TO_GO: u32 = 30;

/// The soft limit as a share of the target, in tenths: `S / ln 2 = target`.
const SOFT_LIMIT_TENTHS: u32 = 7;

/// The hard limit as a multiple of the target, in tenths: above the `2 S =
/// 1.4 x target` an ordinary last iteration ends by.
const HARD_LIMIT_TENTHS: u32 = 25;

/// No move may take more than this share of the clock, so that a long think
/// never leaves too little for the rest of the game.
const LARGEST_CLOCK_SHARE: u32 = 4;

/// The limits for the side to move, or `None` when the search has no clock
/// (`go depth`, `go infinite`).
pub fn time_limits(limits: &Limits, turn: Color) -> Option<TimeLimits> {
    if let Some(movetime) = limits.movetime {
        return Some(TimeLimits {
            soft: movetime,
            hard: movetime,
        });
    }
    let (clock, increment) = match turn {
        Color::White => (limits.wtime, limits.winc),
        Color::Black => (limits.btime, limits.binc),
    };
    let clock = clock?;
    let target = clock / MOVES_TO_GO + increment.unwrap_or_default() / 2;
    let hard = (target * HARD_LIMIT_TENTHS / 10).min(clock / LARGEST_CLOCK_SHARE);
    let soft = (target * SOFT_LIMIT_TENTHS / 10).min(hard);
    Some(TimeLimits { soft, hard })
}
