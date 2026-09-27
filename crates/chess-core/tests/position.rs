//! Spec for exercise 02, part 3 — see `docs/exercises/02-pieces-and-position.md`.
//!
//! The container: castling rights, the two-views-of-one-board invariant, and
//! the accessors movegen and NNUE will lean on.
//!
//! Almost every test here builds its position by parsing a FEN, because `put`
//! and `take` are private — deliberately, so that nothing outside the module can
//! break the one-king invariant. That means this file and `fen.rs` go green
//! together rather than in sequence.

use chess_core::{
    Bitboard, CastlingRights, CastlingSide, Color, Piece, PieceKind, Position, Square,
};

fn pos(fen: &str) -> Position {
    Position::from_fen(fen).expect("test FENs are well-formed")
}

fn sq(algebraic: &str) -> Square {
    algebraic.parse().expect("test squares are well-formed")
}

const ALL_KINDS: [PieceKind; 6] = [
    PieceKind::Pawn,
    PieceKind::Knight,
    PieceKind::Bishop,
    PieceKind::Rook,
    PieceKind::Queen,
    PieceKind::King,
];

/// The heart of the exercise: the mailbox and the bitboards must never disagree.
///
/// Checked in both directions — every square the mailbox calls occupied must be
/// set in exactly the right two boards, and every bit set in any board must have
/// the matching piece in the mailbox. A one-directional check would miss a
/// `take` that cleared the mailbox but left the bitboard alone.
fn assert_views_agree(position: &Position) {
    for index in 0..64u8 {
        let square = Square::from_index(index).expect("0..64 is in range");
        let in_mailbox = position.piece_at(square);

        for color in [Color::White, Color::Black] {
            for kind in ALL_KINDS {
                let in_bitboards = position.pieces(color, kind).contains(square);
                let expected = in_mailbox == Some(Piece::new(color, kind));

                assert_eq!(
                    in_bitboards, expected,
                    "{square}: mailbox says {in_mailbox:?}, but the {color:?} {kind:?} board \
                     says {in_bitboards}"
                );
            }
        }

        assert_eq!(
            position.occupied().contains(square),
            in_mailbox.is_some(),
            "{square}: occupancy disagrees with the mailbox"
        );
    }
}

// --- CastlingRights ----------------------------------------------------------

#[test]
fn no_rights_means_no_rights() {
    let none = CastlingRights::NONE;

    for color in [Color::White, Color::Black] {
        for side in [CastlingSide::King, CastlingSide::Queen] {
            assert!(!none.has(color, side));
        }
    }
    assert_eq!(none.index(), 0);
}

#[test]
fn all_rights_means_all_four() {
    let all = CastlingRights::ALL;

    for color in [Color::White, Color::Black] {
        for side in [CastlingSide::King, CastlingSide::Queen] {
            assert!(all.has(color, side), "{color:?} {side:?} should be allowed");
        }
    }
}

#[test]
fn adding_a_right_leaves_the_other_three_alone() {
    let rights = CastlingRights::NONE.with(Color::White, CastlingSide::King);

    assert!(rights.has(Color::White, CastlingSide::King));
    assert!(!rights.has(Color::White, CastlingSide::Queen));
    assert!(!rights.has(Color::Black, CastlingSide::King));
    assert!(!rights.has(Color::Black, CastlingSide::Queen));
}

#[test]
fn removing_a_right_leaves_the_other_three_alone() {
    let rights = CastlingRights::ALL.without(Color::Black, CastlingSide::Queen);

    assert!(rights.has(Color::White, CastlingSide::King));
    assert!(rights.has(Color::White, CastlingSide::Queen));
    assert!(rights.has(Color::Black, CastlingSide::King));
    assert!(!rights.has(Color::Black, CastlingSide::Queen));
}

#[test]
fn adding_and_removing_are_idempotent() {
    let rights = CastlingRights::ALL.with(Color::White, CastlingSide::King);
    assert_eq!(rights, CastlingRights::ALL);

    let stripped = CastlingRights::NONE.without(Color::White, CastlingSide::King);
    assert_eq!(stripped, CastlingRights::NONE);
}

#[test]
fn revoking_a_colors_rights_takes_both_sides() {
    // What a king move does: one call, both rights gone, the opponent untouched.
    let rights = CastlingRights::ALL.without_color(Color::White);

    assert!(!rights.has(Color::White, CastlingSide::King));
    assert!(!rights.has(Color::White, CastlingSide::Queen));
    assert!(rights.has(Color::Black, CastlingSide::King));
    assert!(rights.has(Color::Black, CastlingSide::Queen));
}

#[test]
fn the_sixteen_combinations_have_sixteen_distinct_indices() {
    // The Zobrist key table is `[u64; 16]` indexed by this, so a collision here
    // would silently make two different positions hash the same.
    let mut seen = Vec::new();

    for white_king in [false, true] {
        for white_queen in [false, true] {
            for black_king in [false, true] {
                for black_queen in [false, true] {
                    let mut rights = CastlingRights::NONE;
                    if white_king {
                        rights = rights.with(Color::White, CastlingSide::King);
                    }
                    if white_queen {
                        rights = rights.with(Color::White, CastlingSide::Queen);
                    }
                    if black_king {
                        rights = rights.with(Color::Black, CastlingSide::King);
                    }
                    if black_queen {
                        rights = rights.with(Color::Black, CastlingSide::Queen);
                    }
                    seen.push(rights.index());
                }
            }
        }
    }

    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen, (0..16).collect::<Vec<_>>());
}

// --- The starting position ---------------------------------------------------

#[test]
fn the_starting_position_has_thirty_two_pieces() {
    let start = Position::starting();

    assert_eq!(start.occupied().count(), 32);
    assert_eq!(start.by_color(Color::White).count(), 16);
    assert_eq!(start.by_color(Color::Black).count(), 16);
}

#[test]
fn the_starting_position_has_the_pieces_in_the_right_places() {
    let start = Position::starting();

    assert_eq!(
        start.piece_at(sq("e1")),
        Some(Piece::new(Color::White, PieceKind::King))
    );
    assert_eq!(
        start.piece_at(sq("d8")),
        Some(Piece::new(Color::Black, PieceKind::Queen))
    );
    assert_eq!(
        start.piece_at(sq("a1")),
        Some(Piece::new(Color::White, PieceKind::Rook))
    );
    assert_eq!(
        start.piece_at(sq("g8")),
        Some(Piece::new(Color::Black, PieceKind::Knight))
    );
    assert_eq!(
        start.piece_at(sq("h2")),
        Some(Piece::new(Color::White, PieceKind::Pawn))
    );

    // The whole middle of the board is empty.
    for name in ["e4", "d4", "a3", "h6", "c5"] {
        assert_eq!(start.piece_at(sq(name)), None, "{name} should be empty");
    }
}

#[test]
fn the_starting_position_has_the_expected_state() {
    let start = Position::starting();

    assert_eq!(start.side_to_move(), Color::White);
    assert_eq!(start.castling(), CastlingRights::ALL);
    assert_eq!(start.en_passant(), None);
    assert_eq!(start.halfmove_clock(), 0);
    assert_eq!(start.fullmove_number(), 1);
}

#[test]
fn the_starting_position_is_the_starting_fen() {
    assert_eq!(
        Position::starting(),
        pos("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
    );
}

// --- The two views agree -----------------------------------------------------

#[test]
fn the_mailbox_and_the_bitboards_agree_in_the_starting_position() {
    assert_views_agree(&Position::starting());
}

#[test]
fn the_mailbox_and_the_bitboards_agree_on_an_empty_board() {
    // Kings only — the least populated position the invariant permits.
    let bare = pos("4k3/8/8/8/8/8/8/4K3 w - - 0 1");

    assert_eq!(bare.occupied().count(), 2);
    assert_views_agree(&bare);
}

#[test]
fn the_mailbox_and_the_bitboards_agree_on_a_crowded_middlegame() {
    let middlegame = pos("r1bq1rk1/pp2ppbp/2np1np1/8/2BNP3/2N1BP2/PPPQ2PP/R3K2R w KQ - 0 10");

    assert_views_agree(&middlegame);
    assert_eq!(middlegame.occupied().count(), 30);
}

// --- Accessors ---------------------------------------------------------------

#[test]
fn by_kind_gathers_both_colors() {
    let start = Position::starting();

    assert_eq!(start.by_kind(PieceKind::Pawn).count(), 16);
    assert_eq!(start.by_kind(PieceKind::King).count(), 2);
    assert_eq!(start.by_kind(PieceKind::Queen).count(), 2);
}

#[test]
fn pieces_intersects_color_and_kind() {
    let start = Position::starting();

    assert_eq!(start.pieces(Color::White, PieceKind::Pawn).count(), 8);
    assert_eq!(start.pieces(Color::Black, PieceKind::Rook).count(), 2);
    assert_eq!(start.pieces(Color::White, PieceKind::King).count(), 1);

    // The defining relationship, checked rather than assumed.
    for color in [Color::White, Color::Black] {
        for kind in ALL_KINDS {
            assert_eq!(
                start.pieces(color, kind),
                start.by_kind(kind) & start.by_color(color),
                "{color:?} {kind:?}"
            );
        }
    }
}

#[test]
fn the_colors_partition_the_occupied_squares() {
    let middlegame = pos("r1bq1rk1/pp2ppbp/2np1np1/8/2BNP3/2N1BP2/PPPQ2PP/R3K2R w KQ - 0 10");

    assert_eq!(
        middlegame.by_color(Color::White) | middlegame.by_color(Color::Black),
        middlegame.occupied()
    );
    assert_eq!(
        middlegame.by_color(Color::White) & middlegame.by_color(Color::Black),
        Bitboard::EMPTY,
        "no square holds two pieces"
    );
}

#[test]
fn the_kinds_partition_the_occupied_squares() {
    let middlegame = pos("r1bq1rk1/pp2ppbp/2np1np1/8/2BNP3/2N1BP2/PPPQ2PP/R3K2R w KQ - 0 10");

    let union = ALL_KINDS
        .into_iter()
        .fold(Bitboard::EMPTY, |acc, kind| acc | middlegame.by_kind(kind));

    assert_eq!(union, middlegame.occupied());
    assert_eq!(
        ALL_KINDS
            .into_iter()
            .map(|kind| middlegame.by_kind(kind).count())
            .sum::<u32>(),
        middlegame.occupied().count(),
        "the kind boards must not overlap"
    );
}

// --- King squares ------------------------------------------------------------

#[test]
fn king_square_finds_each_king() {
    let start = Position::starting();

    assert_eq!(start.king_square(Color::White), sq("e1"));
    assert_eq!(start.king_square(Color::Black), sq("e8"));
}

#[test]
fn king_square_follows_the_kings_around() {
    // NNUE keys every feature on this, and it is asked on a hot path, so it has
    // to be right for arbitrary king placement — not just the home squares.
    let endgame = pos("8/8/4k3/8/8/1K6/8/8 w - - 0 1");

    assert_eq!(endgame.king_square(Color::White), sq("b3"));
    assert_eq!(endgame.king_square(Color::Black), sq("e6"));
}

#[test]
fn king_square_agrees_with_the_king_bitboard() {
    let position = pos("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");

    for color in [Color::White, Color::Black] {
        let from_bitboard = position
            .pieces(color, PieceKind::King)
            .lsb()
            .expect("every position has a king of each colour");

        assert_eq!(position.king_square(color), from_bitboard);
    }
}

// --- Non-starting state ------------------------------------------------------

#[test]
fn state_fields_come_back_as_parsed() {
    // After 1. e4 c5 2. Nf3, with a contrived clock to check it is read and not
    // assumed.
    let sicilian = pos("rnbqkbnr/pp1ppppp/8/2p5/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2");

    assert_eq!(sicilian.side_to_move(), Color::Black);
    assert_eq!(sicilian.castling(), CastlingRights::ALL);
    assert_eq!(sicilian.en_passant(), None);
    assert_eq!(sicilian.halfmove_clock(), 1);
    assert_eq!(sicilian.fullmove_number(), 2);
}

#[test]
fn an_en_passant_square_is_remembered() {
    // After 1. e4: the target is the square the pawn skipped over, not the one
    // it landed on.
    let after_e4 = pos("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1");

    assert_eq!(after_e4.en_passant(), Some(sq("e3")));
    assert_eq!(
        after_e4.piece_at(sq("e4")),
        Some(Piece::new(Color::White, PieceKind::Pawn)),
        "the pawn itself is on e4"
    );
    assert_eq!(after_e4.piece_at(sq("e3")), None, "e3 is empty");
}

#[test]
fn partial_castling_rights_are_remembered() {
    let position = pos("r3k2r/8/8/8/8/8/8/R3K2R w Kq - 0 1");
    let rights = position.castling();

    assert!(rights.has(Color::White, CastlingSide::King));
    assert!(!rights.has(Color::White, CastlingSide::Queen));
    assert!(!rights.has(Color::Black, CastlingSide::King));
    assert!(rights.has(Color::Black, CastlingSide::Queen));
}

// --- Equality ----------------------------------------------------------------

#[test]
fn positions_are_equal_when_every_field_matches() {
    let fen = "r1bq1rk1/pp2ppbp/2np1np1/8/2BNP3/2N1BP2/PPPQ2PP/R3K2R w KQ - 0 10";

    assert_eq!(pos(fen), pos(fen));
}

#[test]
fn positions_differing_only_in_state_are_not_equal() {
    let board = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

    let white_to_move = pos(&format!("{board} w KQkq - 0 1"));
    let black_to_move = pos(&format!("{board} b KQkq - 0 1"));
    let fewer_rights = pos(&format!("{board} w Kkq - 0 1"));
    let later = pos(&format!("{board} w KQkq - 0 5"));

    assert_ne!(white_to_move, black_to_move);
    assert_ne!(white_to_move, fewer_rights);
    assert_ne!(white_to_move, later);
}
