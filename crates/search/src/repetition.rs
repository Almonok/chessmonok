//! Repetition detection: a position that already occurred scores as a draw.
//!
//! Chess draws a game when the same position occurs three times. A search
//! that cannot see this treats a cycle like `Qh7+ Kd6 Qd3+ Ke7 Qh7+` as free:
//! the line keeps its evaluation, so a winning engine happily shuffles until
//! the opponent claims the draw, and a losing one never looks for the cycle
//! that would save it.
//!
//! The rule used here is the usual one in engines: a position that repeats
//! **any** earlier position, in the game or in the line being searched, scores
//! 0, without waiting for the third occurrence. If one side can return to a
//! position once, it can usually do it again, and scoring it 0 right away
//! lets the search see the draw two cycles sooner.
//!
//! Scoring a repetition 0 is all it takes to get the behaviour a player wants:
//!
//! ```text
//!   winning (+15): repeat = 0  <  progress = +15   -> avoids the repetition
//!   losing  (-3):  repeat = 0  >  anything else    -> heads for the repetition
//! ```
//!
//! Only a few earlier positions can be equal to the current one, which keeps
//! the check cheap:
//!
//! - **The same side to move**, so only every second position back.
//! - **At least four plies back**: each side must move away and come back.
//! - **Since the last capture or pawn move**, which the halfmove clock counts:
//!   those moves can never be undone, so nothing before them comes back.
//! - **Since the last pass** of null-move pruning: a pass is not a legal move,
//!   so a position before it is not one the game can return to.
//!
//! A known blemish: a draw found through a repetition holds only on the path
//! that led to it, yet the nodes above it store their scores in the
//! transposition table like any others, where another path may read them.
//! Engines accept this "graph history interaction": it is rare, and the cures
//! (keying the table by history) cost more than they win.

/// The shortest cycle: one move away and one move back, for each side.
const SHORTEST_CYCLE: usize = 4;

/// Whether the last position of `line_keys` repeats one it can be reached
/// from.
///
/// - `game_keys`: the keys of the game's positions before the search's root,
///   oldest first.
/// - `line_keys`: the keys of the line being searched, root first, the current
///   position last.
/// - `passed`: for each position of `line_keys`, whether the move into it was
///   a pass.
/// - `reversible_plies`: the current position's halfmove clock.
pub fn repeats_an_earlier_position(
    game_keys: &[u64],
    line_keys: &[u64],
    passed: &[bool],
    reversible_plies: usize,
) -> bool {
    let Some(&current_key) = line_keys.last() else {
        return false;
    };
    let reachable_plies = reversible_plies.min(plies_since_last_pass(passed));
    // Newest first, so index `n` is the position `n` plies back.
    let earlier_positions = line_keys.iter().rev().chain(game_keys.iter().rev());
    earlier_positions
        .take(reachable_plies + 1)
        .skip(SHORTEST_CYCLE)
        .step_by(2)
        .any(|&earlier_key| earlier_key == current_key)
}

/// How many plies back the line goes before a pass, or `usize::MAX` without
/// one. The position the pass led to still counts: only those before it are
/// cut off.
fn plies_since_last_pass(passed: &[bool]) -> usize {
    passed
        .iter()
        .rev()
        .position(|&was_a_pass| was_a_pass)
        .unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Positions A B C D A: the last one repeats the first, four plies back.
    const CYCLE: [u64; 5] = [1, 2, 3, 4, 1];
    const NO_PASS: [bool; 5] = [false; 5];

    #[test]
    fn a_position_four_plies_back_repeats() {
        assert!(repeats_an_earlier_position(&[], &CYCLE, &NO_PASS, 4));
    }

    #[test]
    fn a_new_position_does_not_repeat() {
        assert!(!repeats_an_earlier_position(
            &[],
            &[1, 2, 3, 4, 5],
            &NO_PASS,
            4
        ));
    }

    #[test]
    fn the_root_alone_does_not_repeat() {
        assert!(!repeats_an_earlier_position(&[], &[1], &[false], 0));
    }

    #[test]
    fn positions_before_the_root_count() {
        // The game went B X C D to the root E, and the search line E F
        // reaches B again: six plies after the game had it.
        let game_keys = [2, 9, 3, 4];
        let line_keys = [5, 6, 2];
        assert!(repeats_an_earlier_position(
            &game_keys,
            &line_keys,
            &[false; 3],
            6
        ));
    }

    #[test]
    fn nothing_before_the_last_capture_or_pawn_move_counts() {
        // Four plies back is the same key, but the halfmove clock says a
        // capture or pawn move happened three plies ago.
        assert!(!repeats_an_earlier_position(&[], &CYCLE, &NO_PASS, 3));
    }

    #[test]
    fn only_the_same_side_to_move_counts() {
        // Five plies back has the key, but the other side was to move there.
        assert!(!repeats_an_earlier_position(
            &[],
            &[1, 2, 3, 4, 5, 1],
            &[false; 6],
            5
        ));
    }

    #[test]
    fn a_pass_cuts_off_the_positions_before_it() {
        // The move into the third position was a pass.
        let passed = [false, false, true, false, false];
        assert!(!repeats_an_earlier_position(&[], &CYCLE, &passed, 4));
    }

    #[test]
    fn positions_after_a_pass_still_count() {
        // A pass, then A B C D A.
        let line_keys = [7, 1, 2, 3, 4, 1];
        let passed = [false, true, false, false, false, false];
        assert!(repeats_an_earlier_position(&[], &line_keys, &passed, 4));
    }
}
