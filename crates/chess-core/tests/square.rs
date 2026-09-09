use chess_core::{File, Rank, Square};

#[test]
fn a1_is_index_zero_and_h8_is_index_sixty_three() {
    assert_eq!(Square::A1.index(), 0);
    assert_eq!(Square::H8.index(), 63);
}

#[test]
fn square_is_built_from_a_file_and_a_rank() {
    // e4: file e (index 4), rank 4 (index 3) -> 3 * 8 + 4 = 28
    assert_eq!(Square::new(File::E, Rank::Fourth).index(), 28);
}

#[test]
fn square_reports_the_file_and_rank_it_was_built_from() {
    let e4 = Square::new(File::E, Rank::Fourth);

    assert_eq!(e4.file(), File::E);
    assert_eq!(e4.rank(), Rank::Fourth);
}

#[test]
fn every_index_round_trips_through_file_and_rank() {
    for index in 0..64u8 {
        let square = Square::from_index(index).expect("0..64 is in range");
        let rebuilt = Square::new(square.file(), square.rank());

        assert_eq!(rebuilt, square, "index {index} did not round-trip");
    }
}

#[test]
fn index_sixty_four_is_not_a_square() {
    assert_eq!(Square::from_index(64), None);
}

#[test]
fn square_displays_as_algebraic_notation() {
    assert_eq!(Square::A1.to_string(), "a1");
    assert_eq!(Square::H8.to_string(), "h8");
    assert_eq!(Square::new(File::E, Rank::Fourth).to_string(), "e4");
}

#[test]
fn algebraic_notation_parses_back_into_a_square() {
    assert_eq!(
        "e4".parse::<Square>(),
        Ok(Square::new(File::E, Rank::Fourth))
    );
    assert_eq!("a1".parse::<Square>(), Ok(Square::A1));
    assert_eq!("h8".parse::<Square>(), Ok(Square::H8));
}

#[test]
fn malformed_algebraic_notation_is_rejected() {
    assert!("e9".parse::<Square>().is_err());
    assert!("i4".parse::<Square>().is_err());
    assert!("e".parse::<Square>().is_err());
    assert!("e44".parse::<Square>().is_err());
    assert!("".parse::<Square>().is_err());
}
