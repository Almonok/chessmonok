//! The accumulator: the first layer of the net, kept up to date move by move.
//!
//! The first layer is by far the biggest (41,024 inputs, 256 outputs), but
//! a move changes only two or three of its inputs. So instead of computing it
//! at every position, the search keeps its output (the accumulator) and
//! adjusts it by those few inputs after each move. That is the "efficiently
//! updatable" in NNUE. `tests/verify.rs` checks a refresh against
//! almoNnuet's values; `tests/incremental.rs` checks that an update always
//! equals a refresh.

use eval::DirtyPieces;
use shakmaty::{Board, Chess, Color, Position, Role, Square};

use crate::features::{active_features, feature_index};
use crate::refresh_cache::RefreshCache;
use crate::{L1, Network};

/// The feature transformer's output for both perspectives: `ft_bias` plus the
/// `ft_row` of every feature that perspective sees.
///
/// Indexed by colour, never by side to move, so a move never swaps the
/// halves; `forward::activate` puts the side to move first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Accumulator {
    pub white: [i16; L1],
    pub black: [i16; L1],
}

impl Accumulator {
    /// One perspective's half.
    pub fn half(&self, perspective: Color) -> &[i16; L1] {
        match perspective {
            Color::White => &self.white,
            Color::Black => &self.black,
        }
    }

    /// One perspective's half, mutably.
    pub fn half_mut(&mut self, perspective: Color) -> &mut [i16; L1] {
        match perspective {
            Color::White => &mut self.white,
            Color::Black => &mut self.black,
        }
    }

    /// Both halves from scratch.
    pub fn refresh(net: &Network, position: &Chess) -> Self {
        let mut accumulator = Accumulator {
            white: [0; L1],
            black: [0; L1],
        };
        for perspective in [Color::White, Color::Black] {
            accumulator.refresh_half(net, position.board(), perspective);
        }
        accumulator
    }

    /// Recompute `perspective`'s half from scratch: the bias plus one row per
    /// feature in `features::active_features(board, perspective)`. Also used
    /// by `update` when `perspective`'s own king moved.
    pub fn refresh_half(&mut self, net: &Network, board: &Board, perspective: Color) {
        let half = self.half_mut(perspective);
        *half = net.ft_bias;
        for feature in active_features(board, perspective) {
            add_row(half, net.ft_row(feature));
        }
    }

    /// Write into `child` the accumulator after the move whose disturbed
    /// pieces are `dirty`, computed from this one (the accumulator before
    /// the move). `after` is the position after the move: it knows where the
    /// kings now stand. Whatever `child` held is overwritten.
    pub fn update(
        &self,
        net: &Network,
        after: &Chess,
        dirty: &DirtyPieces,
        child: &mut Accumulator,
        refresh_cache: &mut RefreshCache,
    ) {
        for perspective in [Color::White, Color::Black] {
            if own_king_moved(dirty, perspective) {
                // Every feature of this side depends on its own king's
                // square, so all of them change: rebuild the half, from the
                // last one computed with the king on this square.
                refresh_cache.refresh_half(
                    net,
                    after.board(),
                    perspective,
                    child.half_mut(perspective),
                );
            } else {
                let king = after
                    .board()
                    .king_of(perspective)
                    .expect("a legal position has both kings");
                RowChanges::new(net, dirty, king, perspective)
                    .apply(self.half(perspective), child.half_mut(perspective));
            }
        }
    }
}

/// Fills the unused places of `RowChanges`; never read.
const NO_ROW: &[i16; L1] = &[0; L1];

/// The feature rows one move takes out of a half and puts into it, as seen
/// from one perspective. Kings are skipped: they have no feature of their
/// own. A move disturbs at most three pieces, so three places each suffice.
struct RowChanges<'net> {
    removed: [&'net [i16; L1]; 3],
    removed_count: usize,
    added: [&'net [i16; L1]; 3],
    added_count: usize,
}

impl<'net> RowChanges<'net> {
    /// The row of every piece that left a square is removed, the row of
    /// every piece that arrived is added.
    fn new(net: &'net Network, dirty: &DirtyPieces, king: Square, perspective: Color) -> Self {
        let mut changes = RowChanges {
            removed: [NO_ROW; 3],
            removed_count: 0,
            added: [NO_ROW; 3],
            added_count: 0,
        };
        for moved in dirty.iter().filter(|moved| moved.piece.role != Role::King) {
            let row = |square| net.ft_row(feature_index(moved.piece, square, king, perspective));
            if let Some(from) = moved.from {
                changes.removed[changes.removed_count] = row(from);
                changes.removed_count += 1;
            }
            if let Some(to) = moved.to {
                changes.added[changes.added_count] = row(to);
                changes.added_count += 1;
            }
        }
        changes
    }

    fn removed(&self) -> &[&'net [i16; L1]] {
        &self.removed[..self.removed_count]
    }

    fn added(&self) -> &[&'net [i16; L1]] {
        &self.added[..self.added_count]
    }

    /// `child = parent - removed rows + added rows`.
    ///
    /// The two shapes nearly every move has (one piece moves; or one moves
    /// and one is captured) are done in a single pass, reading each input
    /// and writing each output once. Copying the parent first and then
    /// making one pass per row, as the general case does, reads and writes
    /// the half once per row on top of the copy.
    fn apply(&self, parent: &[i16; L1], child: &mut [i16; L1]) {
        match (self.removed(), self.added()) {
            ([removed], [added]) => move_one_piece(parent, removed, added, child),
            ([first_removed, second_removed], [added]) => {
                move_one_piece_and_capture(parent, first_removed, second_removed, added, child);
            }
            (removed_rows, added_rows) => {
                *child = *parent;
                for removed in removed_rows {
                    subtract_row(child, removed);
                }
                for added in added_rows {
                    add_row(child, added);
                }
            }
        }
    }
}

/// A quiet move, a castling rook or a promotion: one row out, one row in.
fn move_one_piece(
    parent: &[i16; L1],
    removed: &[i16; L1],
    added: &[i16; L1],
    child: &mut [i16; L1],
) {
    for (((value, &before), &removed), &added) in
        child.iter_mut().zip(parent).zip(removed).zip(added)
    {
        *value = before - removed + added;
    }
}

/// A capture or an en-passant capture: two rows out (the moving piece's
/// old square and the captured piece), one row in.
fn move_one_piece_and_capture(
    parent: &[i16; L1],
    first_removed: &[i16; L1],
    second_removed: &[i16; L1],
    added: &[i16; L1],
    child: &mut [i16; L1],
) {
    for ((((value, &before), &first), &second), &added) in child
        .iter_mut()
        .zip(parent)
        .zip(first_removed)
        .zip(second_removed)
        .zip(added)
    {
        *value = before - first - second + added;
    }
}

/// Add a feature's row of weights to one half. Plain `i16` addition: the
/// training clips every weight so that a half stays within ±8128, so an
/// overflow (which panics in a debug build) can only mean a bug.
pub(crate) fn add_row(half: &mut [i16; L1], row: &[i16; L1]) {
    for (value, weight) in half.iter_mut().zip(row) {
        *value += weight;
    }
}

fn own_king_moved(dirty: &DirtyPieces, perspective: Color) -> bool {
    dirty
        .iter()
        .any(|moved| moved.piece.role == Role::King && moved.piece.color == perspective)
}

pub(crate) fn subtract_row(half: &mut [i16; L1], row: &[i16; L1]) {
    for (value, weight) in half.iter_mut().zip(row) {
        *value -= weight;
    }
}
