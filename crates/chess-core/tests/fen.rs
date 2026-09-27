//! Spec for exercise 02, part 4 — see `docs/exercises/02-pieces-and-position.md`.
//!
//! FEN parsing and writing. These tests are the reason `put` and `take` can stay
//! private: FEN is the only way into a `Position` until make/unmake exists, so
//! it is also the only place the one-king invariant has to be enforced.

use chess_core::{CastlingSide, Color, FenError, Piece, PieceKind, Position, Square};

fn pos(fen: &str) -> Position {
    Position::from_fen(fen).expect("test FENs are well-formed")
}

fn err(fen: &str) -> FenError {
    Position::from_fen(fen).expect_err("this FEN should have been rejected")
}

fn sq(algebraic: &str) -> Square {
    algebraic.parse().expect("test squares are well-formed")
}

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

// --- Piece placement ---------------------------------------------------------

#[test]
fn the_starting_fen_parses_to_the_starting_position() {
    assert_eq!(pos(START), Position::starting());
}

#[test]
fn ranks_are_read_from_rank_eight_downwards() {
    // The classic FEN bug is reading the placement field in index order and
    // ending up with the board upside down. This position is deliberately
    // asymmetric top-to-bottom so that a flipped board cannot pass.
    let position = pos("Q3k3/8/8/8/8/8/8/4K2q w - - 0 1");

    assert_eq!(
        position.piece_at(sq("a8")),
        Some(Piece::new(Color::White, PieceKind::Queen)),
        "the first placement group is rank 8"
    );
    assert_eq!(
        position.piece_at(sq("h1")),
        Some(Piece::new(Color::Black, PieceKind::Queen)),
        "the last placement group is rank 1"
    );
    assert_eq!(position.piece_at(sq("a1")), None);
    assert_eq!(position.piece_at(sq("h8")), None);
}

#[test]
fn files_within_a_rank_are_read_from_a_to_h() {
    // Asymmetric left-to-right, for the same reason.
    let position = pos("8/8/8/8/8/8/8/RK5k w - - 0 1");

    assert_eq!(
        position.piece_at(sq("a1")),
        Some(Piece::new(Color::White, PieceKind::Rook))
    );
    assert_eq!(
        position.piece_at(sq("b1")),
        Some(Piece::new(Color::White, PieceKind::King))
    );
    assert_eq!(
        position.piece_at(sq("h1")),
        Some(Piece::new(Color::Black, PieceKind::King))
    );
}

#[test]
fn digits_skip_that_many_empty_squares() {
    let position = pos("4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1");

    assert_eq!(position.occupied().count(), 4);
    assert_eq!(
        position.piece_at(sq("e8")),
        Some(Piece::new(Color::Black, PieceKind::King))
    );
    assert_eq!(
        position.piece_at(sq("a1")),
        Some(Piece::new(Color::White, PieceKind::Rook))
    );
    assert_eq!(
        position.piece_at(sq("h1")),
        Some(Piece::new(Color::White, PieceKind::Rook))
    );
    for name in ["b1", "c1", "d1", "f1", "g1"] {
        assert_eq!(position.piece_at(sq(name)), None, "{name} should be empty");
    }
}

#[test]
fn an_eight_skips_a_whole_rank() {
    let bare = pos("4k3/8/8/8/8/8/8/4K3 w - - 0 1");

    assert_eq!(bare.occupied().count(), 2);
}

// --- The other five fields ---------------------------------------------------

#[test]
fn side_to_move_is_read() {
    assert_eq!(pos(START).side_to_move(), Color::White);
    assert_eq!(
        pos("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR b KQkq - 0 1").side_to_move(),
        Color::Black
    );
}

#[test]
fn castling_rights_are_read_letter_by_letter() {
    let cases: [(&str, [bool; 4]); 5] = [
        ("KQkq", [true, true, true, true]),
        ("-", [false, false, false, false]),
        ("Kq", [true, false, false, true]),
        ("Qk", [false, true, true, false]),
        ("KQ", [true, true, false, false]),
    ];

    for (field, [wk, wq, bk, bq]) in cases {
        let rights = pos(&format!("r3k2r/8/8/8/8/8/8/R3K2R w {field} - 0 1")).castling();

        assert_eq!(rights.has(Color::White, CastlingSide::King), wk, "{field}");
        assert_eq!(rights.has(Color::White, CastlingSide::Queen), wq, "{field}");
        assert_eq!(rights.has(Color::Black, CastlingSide::King), bk, "{field}");
        assert_eq!(rights.has(Color::Black, CastlingSide::Queen), bq, "{field}");
    }
}

#[test]
fn an_en_passant_square_is_read_and_a_dash_is_none() {
    assert_eq!(pos(START).en_passant(), None);

    let after_e4 = pos("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1");
    assert_eq!(after_e4.en_passant(), Some(sq("e3")));

    let after_c5 = pos("rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w KQkq c6 0 2");
    assert_eq!(after_c5.en_passant(), Some(sq("c6")));
}

#[test]
fn the_clocks_are_read() {
    let position = pos("4k3/8/8/8/8/8/8/4K3 w - - 42 137");

    assert_eq!(position.halfmove_clock(), 42);
    assert_eq!(position.fullmove_number(), 137);
}

// --- Writing -----------------------------------------------------------------

#[test]
fn the_starting_position_writes_the_starting_fen() {
    assert_eq!(Position::starting().to_fen(), START);
}

#[test]
fn display_writes_the_fen() {
    // Display and FromStr are inverses; that is what makes the round-trip below
    // worth anything.
    assert_eq!(Position::starting().to_string(), START);
}

#[test]
fn empty_runs_are_written_as_digits_not_repeated_ones() {
    let fen = "4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1";

    assert_eq!(pos(fen).to_fen(), fen);
}

#[test]
fn every_field_survives_a_round_trip() {
    let fens = [
        START,
        "4k3/8/8/8/8/8/8/4K3 w - - 0 1",
        "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
        "r1bq1rk1/pp2ppbp/2np1np1/8/2BNP3/2N1BP2/PPPQ2PP/R3K2R w KQ - 0 10",
        "r3k2r/8/8/8/8/8/8/R3K2R b Kq - 42 137",
        "8/8/4k3/8/8/1K6/8/8 w - - 99 250",
        "8/2k5/8/8/8/8/5K2/8 b - - 0 1",
    ];

    for fen in fens {
        assert_eq!(pos(fen).to_fen(), fen, "{fen} did not survive a round trip");
        assert_eq!(
            fen.parse::<Position>().expect("well-formed"),
            pos(fen),
            "{fen}: parse and from_fen disagree"
        );
    }
}

// --- Rejection ---------------------------------------------------------------

#[test]
fn the_field_count_must_be_six() {
    assert!(matches!(
        err("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0"),
        FenError::WrongFieldCount { .. }
    ));
    assert!(matches!(
        err("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1 extra"),
        FenError::WrongFieldCount { .. }
    ));
    assert!(matches!(err(""), FenError::WrongFieldCount { .. }));
}

#[test]
fn the_placement_field_must_have_eight_ranks() {
    assert!(matches!(
        err("8/8/8/8/8/8/4K1k1 w - - 0 1"),
        FenError::WrongRankCount { .. }
    ));
    assert!(matches!(
        err("8/8/8/8/8/8/8/8/4K1k1 w - - 0 1"),
        FenError::WrongRankCount { .. }
    ));
}

#[test]
fn every_rank_must_describe_exactly_eight_squares() {
    // Seven squares on rank 8.
    assert!(matches!(
        err("4k2/8/8/8/8/8/8/4K3 w - - 0 1"),
        FenError::RankIsNotEightSquares { .. }
    ));
    // Nine, via a digit that overshoots.
    assert!(matches!(
        err("4k4/8/8/8/8/8/8/4K3 w - - 0 1"),
        FenError::RankIsNotEightSquares { .. }
    ));
    // Nine, via too many pieces.
    assert!(matches!(
        err("kqqqqqqqq/8/8/8/8/8/8/4K3 w - - 0 1"),
        FenError::RankIsNotEightSquares { .. }
    ));
}

#[test]
fn unknown_piece_letters_are_rejected() {
    assert!(matches!(
        err("4k3/8/8/8/3x4/8/8/4K3 w - - 0 1"),
        FenError::UnknownPiece('x')
    ));
    // `0` is not a run length; the digits are 1..=8.
    assert!(matches!(
        err("4k3/8/8/8/08/8/8/4K3 w - - 0 1"),
        FenError::UnknownPiece('0')
    ));
    // `9` overshoots a rank on its own.
    assert!(matches!(
        err("4k3/9/8/8/8/8/8/4K3 w - - 0 1"),
        FenError::UnknownPiece('9')
    ));
}

#[test]
fn the_side_to_move_must_be_w_or_b() {
    assert!(matches!(
        err("4k3/8/8/8/8/8/8/4K3 x - - 0 1"),
        FenError::BadSideToMove(_)
    ));
    assert!(matches!(
        err("4k3/8/8/8/8/8/8/4K3 W - - 0 1"),
        FenError::BadSideToMove(_),
    ));
    assert!(matches!(
        err("4k3/8/8/8/8/8/8/4K3 white - - 0 1"),
        FenError::BadSideToMove(_)
    ));
}

#[test]
fn malformed_castling_rights_are_rejected() {
    assert!(matches!(
        err("4k3/8/8/8/8/8/8/4K3 w X - 0 1"),
        FenError::BadCastlingRights(_)
    ));
    assert!(matches!(
        err("4k3/8/8/8/8/8/8/4K3 w KK - 0 1"),
        FenError::BadCastlingRights(_),
    ));
    assert!(matches!(
        err("4k3/8/8/8/8/8/8/4K3 w  - 0 1"),
        FenError::WrongFieldCount { .. },
    ));
    assert!(matches!(
        err("4k3/8/8/8/8/8/8/4K3 w K- - 0 1"),
        FenError::BadCastlingRights(_)
    ));
}

#[test]
fn a_malformed_en_passant_square_is_rejected() {
    assert!(matches!(
        err("4k3/8/8/8/8/8/8/4K3 w - e9 0 1"),
        FenError::BadEnPassant(_)
    ));
    assert!(matches!(
        err("4k3/8/8/8/8/8/8/4K3 w - zz 0 1"),
        FenError::BadEnPassant(_)
    ));
    assert!(matches!(
        err("4k3/8/8/8/8/8/8/4K3 w - e 0 1"),
        FenError::BadEnPassant(_)
    ));
}

#[test]
fn unparseable_clocks_are_rejected() {
    assert!(matches!(
        err("4k3/8/8/8/8/8/8/4K3 w - - x 1"),
        FenError::BadHalfmoveClock(_)
    ));
    assert!(matches!(
        err("4k3/8/8/8/8/8/8/4K3 w - - 0 x"),
        FenError::BadFullmoveNumber(_)
    ));
    assert!(matches!(
        err("4k3/8/8/8/8/8/8/4K3 w - - -1 1"),
        FenError::BadHalfmoveClock(_)
    ));
}

// --- The one-king invariant --------------------------------------------------

#[test]
fn a_position_without_a_white_king_is_rejected() {
    // This is what makes `king_square` total. Without it, the accessor NNUE
    // leans on would have to return an Option forever.
    assert!(matches!(
        err("4k3/8/8/8/8/8/8/8 w - - 0 1"),
        FenError::WrongKingCount {
            color: Color::White,
            ..
        }
    ));
}

#[test]
fn a_position_without_a_black_king_is_rejected() {
    assert!(matches!(
        err("8/8/8/8/8/8/8/4K3 w - - 0 1"),
        FenError::WrongKingCount {
            color: Color::Black,
            ..
        }
    ));
}

#[test]
fn a_position_with_two_kings_of_one_color_is_rejected() {
    assert!(matches!(
        err("4k3/8/8/8/8/8/8/3KK3 w - - 0 1"),
        FenError::WrongKingCount {
            color: Color::White,
            found: 2
        }
    ));
    assert!(matches!(
        err("3kk3/8/8/8/8/8/8/4K3 w - - 0 1"),
        FenError::WrongKingCount {
            color: Color::Black,
            found: 2
        }
    ));
}

#[test]
fn an_entirely_empty_board_is_rejected() {
    assert!(matches!(
        err("8/8/8/8/8/8/8/8 w - - 0 1"),
        FenError::WrongKingCount { found: 0, .. }
    ));
}

// --- Errors are useful -------------------------------------------------------

#[test]
fn errors_describe_themselves() {
    // A parse failure ends up in front of a human, via UCI or a test failure, so
    // the message has to name what was wrong rather than just "invalid FEN".
    let message = err("4k3/8/8/8/3x4/8/8/4K3 w - - 0 1").to_string();

    assert!(
        message.contains('x'),
        "the message should name the offending character, got: {message}"
    );
}
