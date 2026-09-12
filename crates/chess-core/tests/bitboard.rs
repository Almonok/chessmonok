use chess_core::{Bitboard, File, Rank, Square};

/// Shorthand so the tests below read as board positions, not index arithmetic.
fn sq(algebraic: &str) -> Square {
    algebraic.parse().expect("test squares are well-formed")
}

// --- The numbering contract -------------------------------------------------

#[test]
fn bit_n_is_square_n() {
    // This is the whole reason bitboards and squares are interchangeable: the
    // bit position *is* the square index. If this test ever fails, every
    // attack table in the crate is silently wrong.
    for index in 0..64u8 {
        let square = Square::from_index(index).expect("0..64 is in range");
        let board = Bitboard::from_square(square);

        assert_eq!(
            board.bits(),
            1u64 << index,
            "{square} should be bit {index}"
        );
    }
}

#[test]
fn empty_contains_nothing_and_full_contains_everything() {
    for index in 0..64u8 {
        let square = Square::from_index(index).expect("0..64 is in range");

        assert!(!Bitboard::EMPTY.contains(square));
        assert!(Bitboard::FULL.contains(square));
    }

    assert!(Bitboard::EMPTY.is_empty());
    assert!(!Bitboard::FULL.is_empty());
}

#[test]
fn from_square_sets_exactly_that_square() {
    let board = Bitboard::from_square(sq("e4"));

    assert!(board.contains(sq("e4")));
    assert!(!board.contains(sq("e5")));
    assert!(!board.contains(sq("d4")));
    assert_eq!(board.count(), 1);
}

// --- Single-square mutation -------------------------------------------------

#[test]
fn insert_adds_only_the_named_square() {
    let mut board = Bitboard::EMPTY;

    board.insert(sq("a1"));
    board.insert(sq("h8"));

    assert!(board.contains(sq("a1")));
    assert!(board.contains(sq("h8")));
    assert_eq!(board.count(), 2);
}

#[test]
fn inserting_a_square_twice_is_the_same_as_once() {
    let mut board = Bitboard::EMPTY;

    board.insert(sq("c3"));
    board.insert(sq("c3"));

    assert_eq!(board.count(), 1);
}

#[test]
fn remove_clears_only_the_named_square() {
    let mut board = Bitboard::FULL;

    board.remove(sq("d5"));

    assert!(!board.contains(sq("d5")));
    assert_eq!(board.count(), 63);
}

#[test]
fn removing_an_absent_square_changes_nothing() {
    let mut board = Bitboard::from_square(sq("g7"));

    board.remove(sq("b2"));

    assert_eq!(board, Bitboard::from_square(sq("g7")));
}

#[test]
fn toggling_a_square_twice_restores_the_board() {
    let original = Bitboard::from_square(sq("f6"));
    let mut board = original;

    board.toggle(sq("b5"));
    assert!(board.contains(sq("b5")));

    board.toggle(sq("b5"));
    assert_eq!(board, original);
}

// --- Counting and scanning --------------------------------------------------

#[test]
fn count_reports_the_number_of_occupied_squares() {
    assert_eq!(Bitboard::EMPTY.count(), 0);
    assert_eq!(Bitboard::FULL.count(), 64);

    let mut board = Bitboard::EMPTY;
    for name in ["a1", "b2", "c3", "d4"] {
        board.insert(sq(name));
    }
    assert_eq!(board.count(), 4);
}

#[test]
fn lsb_is_the_lowest_numbered_occupied_square() {
    let mut board = Bitboard::EMPTY;
    board.insert(sq("h8"));
    board.insert(sq("c1"));
    board.insert(sq("e4"));

    // c1 is index 2; e4 is 28; h8 is 63.
    assert_eq!(board.lsb(), Some(sq("c1")));
}

#[test]
fn lsb_of_an_empty_board_is_none() {
    assert_eq!(Bitboard::EMPTY.lsb(), None);
}

#[test]
fn pop_lsb_drains_the_board_in_ascending_order() {
    let mut board = Bitboard::EMPTY;
    board.insert(sq("e4"));
    board.insert(sq("a1"));
    board.insert(sq("h8"));

    assert_eq!(board.pop_lsb(), Some(sq("a1")));
    assert_eq!(board.pop_lsb(), Some(sq("e4")));
    assert_eq!(board.pop_lsb(), Some(sq("h8")));
    assert_eq!(board.pop_lsb(), None);
    assert!(board.is_empty());
}

// --- Set algebra ------------------------------------------------------------

#[test]
fn intersection_keeps_only_shared_squares() {
    let left = Bitboard::from_square(sq("a1")) | Bitboard::from_square(sq("b2"));
    let right = Bitboard::from_square(sq("b2")) | Bitboard::from_square(sq("c3"));

    assert_eq!(left & right, Bitboard::from_square(sq("b2")));
}

#[test]
fn union_keeps_squares_from_both_sides() {
    let union = Bitboard::from_square(sq("a1")) | Bitboard::from_square(sq("h8"));

    assert!(union.contains(sq("a1")));
    assert!(union.contains(sq("h8")));
    assert_eq!(union.count(), 2);
}

#[test]
fn symmetric_difference_drops_the_squares_both_sides_share() {
    let left = Bitboard::from_square(sq("a1")) | Bitboard::from_square(sq("b2"));
    let right = Bitboard::from_square(sq("b2")) | Bitboard::from_square(sq("c3"));

    assert_eq!(
        left ^ right,
        Bitboard::from_square(sq("a1")) | Bitboard::from_square(sq("c3"))
    );
}

#[test]
fn complement_swaps_occupied_and_empty() {
    assert_eq!(!Bitboard::EMPTY, Bitboard::FULL);
    assert_eq!(!Bitboard::FULL, Bitboard::EMPTY);

    let board = Bitboard::from_square(sq("e4"));
    assert_eq!((!board).count(), 63);
    assert!(!(!board).contains(sq("e4")));
}

#[test]
fn difference_is_intersection_with_a_complement() {
    // The idiom movegen leans on constantly: "my moves, minus my own pieces".
    let targets = Bitboard::from_square(sq("d4")) | Bitboard::from_square(sq("d5"));
    let friendly = Bitboard::from_square(sq("d5"));

    assert_eq!(targets & !friendly, Bitboard::from_square(sq("d4")));
}

#[test]
fn assign_operators_match_their_binary_forms() {
    let left = Bitboard::from_square(sq("a1")) | Bitboard::from_square(sq("b2"));
    let right = Bitboard::from_square(sq("b2")) | Bitboard::from_square(sq("c3"));

    let mut and = left;
    and &= right;
    assert_eq!(and, left & right);

    let mut or = left;
    or |= right;
    assert_eq!(or, left | right);

    let mut xor = left;
    xor ^= right;
    assert_eq!(xor, left ^ right);
}

// --- Iteration --------------------------------------------------------------

#[test]
fn iteration_yields_every_occupied_square_in_ascending_order() {
    let mut board = Bitboard::EMPTY;
    for name in ["h8", "a1", "e4", "c1"] {
        board.insert(sq(name));
    }

    let visited: Vec<Square> = board.into_iter().collect();

    assert_eq!(visited, vec![sq("a1"), sq("c1"), sq("e4"), sq("h8")]);
}

#[test]
fn an_empty_board_iterates_over_nothing() {
    assert_eq!(Bitboard::EMPTY.into_iter().count(), 0);
}

#[test]
fn a_full_board_iterates_over_all_sixty_four_squares() {
    let visited: Vec<Square> = Bitboard::FULL.into_iter().collect();

    assert_eq!(visited.len(), 64);
    assert_eq!(visited[0], Square::A1);
    assert_eq!(visited[63], Square::H8);
}

#[test]
fn collecting_squares_rebuilds_the_board() {
    let mut board = Bitboard::EMPTY;
    for name in ["b7", "d2", "f5"] {
        board.insert(sq(name));
    }

    let rebuilt: Bitboard = board.into_iter().collect();

    assert_eq!(rebuilt, board);
}

// --- Display ----------------------------------------------------------------

#[test]
fn display_draws_the_board_with_rank_eight_on_top() {
    let board = Bitboard::from_square(Square::new(File::E, Rank::Fourth));

    let expected = concat!(
        ". . . . . . . .\n",
        ". . . . . . . .\n",
        ". . . . . . . .\n",
        ". . . . . . . .\n",
        ". . . . X . . .\n",
        ". . . . . . . .\n",
        ". . . . . . . .\n",
        ". . . . . . . ."
    );

    assert_eq!(board.to_string(), expected);
}

#[test]
fn display_puts_the_a_file_on_the_left() {
    // a1 is the bottom-left corner, h1 the bottom-right.
    let board = Bitboard::from_square(sq("a1")) | Bitboard::from_square(sq("h1"));

    let last_line = board
        .to_string()
        .lines()
        .last()
        .expect("eight lines")
        .to_owned();

    assert_eq!(last_line, "X . . . . . . X");
}
