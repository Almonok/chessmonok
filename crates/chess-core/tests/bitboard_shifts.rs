//! Spec for exercise 01 — see `docs/exercises/01-directional-shifts.md`.
//!
//! These tests describe the contract; the implementation lives in
//! `src/bitboard.rs`. They are written against behaviour rather than against
//! bit patterns on purpose: asserting `0x0101_0101_0101_0101` would hand over
//! the answer instead of checking it.

use chess_core::{Bitboard, File, Rank, Square};

/// Shorthand so the tests below read as board positions, not index arithmetic.
fn sq(algebraic: &str) -> Square {
    algebraic.parse().expect("test squares are well-formed")
}

/// Builds a board from a list of algebraic squares.
fn board(squares: &[&str]) -> Bitboard {
    squares.iter().copied().map(sq).collect()
}

// --- Files and ranks as sets ------------------------------------------------

#[test]
fn the_a_file_holds_the_eight_a_squares() {
    let a_file = Bitboard::file(File::A);

    assert_eq!(a_file.count(), 8);
    for name in ["a1", "a2", "a3", "a4", "a5", "a6", "a7", "a8"] {
        assert!(
            a_file.contains(sq(name)),
            "the a-file should contain {name}"
        );
    }
    assert!(!a_file.contains(sq("b1")));
}

#[test]
fn every_file_holds_exactly_its_own_squares() {
    for index in 0..8u8 {
        let file = File::from_index(index).expect("0..8 is in range");
        let squares = Bitboard::file(file);

        assert_eq!(squares.count(), 8, "file {index} should have eight squares");
        for square in squares {
            assert_eq!(square.file(), file, "{square} is not on file {index}");
        }
    }
}

#[test]
fn the_first_rank_holds_the_eight_rank_one_squares() {
    let first = Bitboard::rank(Rank::First);

    assert_eq!(first.count(), 8);
    for name in ["a1", "b1", "c1", "d1", "e1", "f1", "g1", "h1"] {
        assert!(first.contains(sq(name)), "rank 1 should contain {name}");
    }
    assert!(!first.contains(sq("a2")));
}

#[test]
fn every_rank_holds_exactly_its_own_squares() {
    for index in 0..8u8 {
        let rank = Rank::from_index(index).expect("0..8 is in range");
        let squares = Bitboard::rank(rank);

        assert_eq!(squares.count(), 8, "rank {index} should have eight squares");
        for square in squares {
            assert_eq!(square.rank(), rank, "{square} is not on rank {index}");
        }
    }
}

// --- The four straight directions -------------------------------------------

#[test]
fn north_moves_one_rank_towards_rank_eight() {
    assert_eq!(board(&["e4"]).north(), board(&["e5"]));
    assert_eq!(board(&["a1"]).north(), board(&["a2"]));
    assert_eq!(board(&["h7"]).north(), board(&["h8"]));
}

#[test]
fn south_moves_one_rank_towards_rank_one() {
    assert_eq!(board(&["e4"]).south(), board(&["e3"]));
    assert_eq!(board(&["h8"]).south(), board(&["h7"]));
    assert_eq!(board(&["a2"]).south(), board(&["a1"]));
}

#[test]
fn east_moves_one_file_towards_the_h_file() {
    assert_eq!(board(&["e4"]).east(), board(&["f4"]));
    assert_eq!(board(&["a1"]).east(), board(&["b1"]));
    assert_eq!(board(&["g8"]).east(), board(&["h8"]));
}

#[test]
fn west_moves_one_file_towards_the_a_file() {
    assert_eq!(board(&["e4"]).west(), board(&["d4"]));
    assert_eq!(board(&["h8"]).west(), board(&["g8"]));
    assert_eq!(board(&["b1"]).west(), board(&["a1"]));
}

#[test]
fn squares_pushed_off_the_top_or_bottom_are_dropped() {
    assert_eq!(board(&["e8"]).north(), Bitboard::EMPTY);
    assert_eq!(board(&["a8"]).north(), Bitboard::EMPTY);
    assert_eq!(board(&["e1"]).south(), Bitboard::EMPTY);
    assert_eq!(board(&["h1"]).south(), Bitboard::EMPTY);
}

#[test]
fn east_and_west_do_not_wrap_around_the_edge() {
    // h1 has no eastern neighbour. The naive `bits << 1` would land it on a2,
    // one rank up on the far side of the board — the classic wrap bug.
    let wrapped = board(&["h1"]).east();
    assert!(
        !wrapped.contains(sq("a2")),
        "h1 moved east must not reappear on a2:\n{wrapped}"
    );
    assert_eq!(wrapped, Bitboard::EMPTY);

    let wrapped = board(&["a2"]).west();
    assert!(
        !wrapped.contains(sq("h1")),
        "a2 moved west must not reappear on h1:\n{wrapped}"
    );
    assert_eq!(wrapped, Bitboard::EMPTY);

    assert_eq!(board(&["h4"]).east(), Bitboard::EMPTY);
    assert_eq!(board(&["a4"]).west(), Bitboard::EMPTY);
}

// --- The four diagonals ------------------------------------------------------

#[test]
fn diagonals_move_one_rank_and_one_file() {
    assert_eq!(board(&["e4"]).north_east(), board(&["f5"]));
    assert_eq!(board(&["e4"]).north_west(), board(&["d5"]));
    assert_eq!(board(&["e4"]).south_east(), board(&["f3"]));
    assert_eq!(board(&["e4"]).south_west(), board(&["d3"]));
}

#[test]
fn diagonals_do_not_wrap_off_the_h_file() {
    assert_eq!(board(&["h4"]).north_east(), Bitboard::EMPTY);
    assert_eq!(board(&["h4"]).south_east(), Bitboard::EMPTY);
    assert_eq!(board(&["h1"]).north_east(), Bitboard::EMPTY);
}

#[test]
fn diagonals_do_not_wrap_off_the_a_file() {
    assert_eq!(board(&["a4"]).north_west(), Bitboard::EMPTY);
    assert_eq!(board(&["a4"]).south_west(), Bitboard::EMPTY);
    assert_eq!(board(&["a8"]).south_west(), Bitboard::EMPTY);
    assert_eq!(board(&["a1"]).north_west(), Bitboard::EMPTY);
}

#[test]
fn diagonals_drop_squares_that_leave_the_top_or_bottom() {
    assert_eq!(board(&["e8"]).north_east(), Bitboard::EMPTY);
    assert_eq!(board(&["e8"]).north_west(), Bitboard::EMPTY);
    assert_eq!(board(&["e1"]).south_east(), Bitboard::EMPTY);
    assert_eq!(board(&["e1"]).south_west(), Bitboard::EMPTY);
}

// --- Whole-set behaviour -----------------------------------------------------

#[test]
fn shifting_moves_every_square_in_the_set_at_once() {
    // The point of bitboards: one shift relocates the whole set.
    let pawns: Bitboard = board(&["a2", "b2", "c2", "d2", "e2", "f2", "g2", "h2"]);

    assert_eq!(
        pawns.north(),
        board(&["a3", "b3", "c3", "d3", "e3", "f3", "g3", "h3"])
    );
}

#[test]
fn shifting_a_full_board_loses_exactly_the_trailing_line() {
    assert_eq!(Bitboard::FULL.north(), !Bitboard::rank(Rank::First));
    assert_eq!(Bitboard::FULL.south(), !Bitboard::rank(Rank::Eighth));
    assert_eq!(Bitboard::FULL.east(), !Bitboard::file(File::A));
    assert_eq!(Bitboard::FULL.west(), !Bitboard::file(File::H));

    // A diagonal shift loses a rank *and* a file: 7 * 7 squares survive.
    assert_eq!(Bitboard::FULL.north_east().count(), 49);
    assert_eq!(Bitboard::FULL.north_west().count(), 49);
    assert_eq!(Bitboard::FULL.south_east().count(), 49);
    assert_eq!(Bitboard::FULL.south_west().count(), 49);
}

#[test]
fn a_shift_and_its_opposite_cancel_away_from_the_edges() {
    for square in Bitboard::FULL {
        let one = Bitboard::from_square(square);

        if square.rank() != Rank::Eighth {
            assert_eq!(
                one.north().south(),
                one,
                "{square} did not survive N then S"
            );
        }
        if square.file() != File::H {
            assert_eq!(one.east().west(), one, "{square} did not survive E then W");
        }
        if square.rank() != Rank::Eighth && square.file() != File::H {
            assert_eq!(
                one.north_east().south_west(),
                one,
                "{square} did not survive NE then SW"
            );
        }
    }
}

#[test]
fn an_empty_board_stays_empty_in_every_direction() {
    let empty = Bitboard::EMPTY;

    assert_eq!(empty.north(), empty);
    assert_eq!(empty.south(), empty);
    assert_eq!(empty.east(), empty);
    assert_eq!(empty.west(), empty);
    assert_eq!(empty.north_east(), empty);
    assert_eq!(empty.north_west(), empty);
    assert_eq!(empty.south_east(), empty);
    assert_eq!(empty.south_west(), empty);
}
