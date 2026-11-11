//! Killer moves: quiet moves that refuted a sibling position.
//!
//! The positions searched at one ply are usually siblings: they differ only
//! by the opponent's last move. When a quiet move causes a beta cutoff in
//! one of them, the threat it makes is often there in the others too:
//!
//! ```text
//! ply 3, after 1...a6:  Ng5 refutes   -> remembered as a killer at ply 3
//! ply 3, after 1...h6:  try Ng5 early, if it is legal here
//! ```
//!
//! Each ply keeps the last two such moves. Captures are never killers: MVV-LVA
//! already puts them early. A killer is only a hint for the order; it is
//! never played unless it is one of the node's own legal moves (`ordering`).

use shakmaty::Move;

use crate::MAX_PLY;

/// The two latest quiet cutoff moves of every ply, newest first.
#[derive(Clone, Debug)]
pub(crate) struct Killers {
    slots: Vec<[Option<Move>; 2]>,
}

impl Killers {
    /// No killer yet at any ply, from the root to `MAX_PLY`.
    pub(crate) fn new() -> Self {
        Self {
            slots: vec![[None, None]; MAX_PLY + 1],
        }
    }

    /// `m` caused a cutoff at `ply`: it becomes the first killer and the old
    /// first one moves to the second slot. Recording the first killer again
    /// changes nothing, so the two slots always hold two different moves.
    pub(crate) fn record(&mut self, ply: usize, refutation: Move) {
        let slot = &mut self.slots[ply];
        if slot[0] != Some(refutation) {
            slot[1] = slot[0];
            slot[0] = Some(refutation);
        }
    }

    pub(crate) fn at(&self, ply: usize) -> [Option<Move>; 2] {
        self.slots[ply]
    }
}

#[cfg(test)]
mod tests {
    use shakmaty::{Move, Role, Square};

    use super::*;
    use crate::MAX_PLY;

    fn quiet(from: Square, to: Square) -> Move {
        Move::Normal {
            role: Role::Knight,
            from,
            capture: None,
            to,
            promotion: None,
        }
    }

    #[test]
    fn recording_pushes_the_first_killer_into_the_second_slot() {
        let (first, second) = (quiet(Square::G1, Square::F3), quiet(Square::B1, Square::C3));
        let mut killers = Killers::new();
        killers.record(4, first);
        killers.record(4, second);
        assert_eq!(killers.at(4), [Some(second), Some(first)]);
    }

    #[test]
    fn recording_the_first_killer_again_changes_nothing() {
        let (first, second) = (quiet(Square::G1, Square::F3), quiet(Square::B1, Square::C3));
        let mut killers = Killers::new();
        killers.record(4, first);
        killers.record(4, second);
        killers.record(4, second);
        assert_eq!(killers.at(4), [Some(second), Some(first)]);
    }

    #[test]
    fn plies_keep_their_own_killers() {
        let mut killers = Killers::new();
        killers.record(4, quiet(Square::G1, Square::F3));
        assert_eq!(killers.at(5), [None, None]);
    }

    #[test]
    fn the_last_ply_can_hold_killers() {
        let mut killers = Killers::new();
        killers.record(MAX_PLY, quiet(Square::G1, Square::F3));
        assert!(killers.at(MAX_PLY)[0].is_some());
    }
}
