//! Spec for exercise 02, part 1 — see `docs/exercises/02-pieces-and-position.md`.
//!
//! The vocabulary types: `Color`, `PieceKind` and the `Piece` that pairs them.
//! No board yet — these are pure conversions, and they are worth getting exactly
//! right because every later type indexes arrays with them.

use chess_core::{Color, Piece, PieceKind};

/// Every piece, in index order. Handy for exhaustive loops below.
fn all_pieces() -> Vec<Piece> {
    (0..12)
        .map(|index| {
            let color = if index < 6 {
                Color::White
            } else {
                Color::Black
            };
            let kind = PieceKind::from_index(index % 6).expect("0..6 is in range");
            Piece::new(color, kind)
        })
        .collect()
}

const ALL_KINDS: [PieceKind; 6] = [
    PieceKind::Pawn,
    PieceKind::Knight,
    PieceKind::Bishop,
    PieceKind::Rook,
    PieceKind::Queen,
    PieceKind::King,
];

// --- Color -------------------------------------------------------------------

#[test]
fn the_two_colors_have_indices_zero_and_one() {
    assert_eq!(Color::White.index(), 0);
    assert_eq!(Color::Black.index(), 1);
}

#[test]
fn flipping_a_color_gives_the_other_one() {
    assert_eq!(Color::White.flip(), Color::Black);
    assert_eq!(Color::Black.flip(), Color::White);
}

#[test]
fn flipping_twice_is_the_identity() {
    assert_eq!(Color::White.flip().flip(), Color::White);
    assert_eq!(Color::Black.flip().flip(), Color::Black);
}

#[test]
fn the_not_operator_flips_a_color() {
    // `!side_to_move` reads better than `side_to_move.flip()` inside movegen,
    // which is the only reason the operator exists.
    assert_eq!(!Color::White, Color::Black);
    assert_eq!(!Color::Black, Color::White);
}

// --- PieceKind ---------------------------------------------------------------

#[test]
fn piece_kinds_are_indexed_pawn_through_king() {
    assert_eq!(PieceKind::Pawn.index(), 0);
    assert_eq!(PieceKind::Knight.index(), 1);
    assert_eq!(PieceKind::Bishop.index(), 2);
    assert_eq!(PieceKind::Rook.index(), 3);
    assert_eq!(PieceKind::Queen.index(), 4);
    assert_eq!(PieceKind::King.index(), 5);
}

#[test]
fn piece_kind_survives_a_round_trip_through_its_index() {
    for kind in ALL_KINDS {
        let back = PieceKind::from_index(kind.index()).expect("a real kind's index is in range");
        assert_eq!(back, kind);
    }
}

#[test]
fn there_is_no_seventh_piece_kind() {
    assert_eq!(PieceKind::from_index(6), None);
    assert_eq!(PieceKind::from_index(usize::MAX), None);
}

#[test]
fn piece_kinds_print_as_their_lowercase_fen_letters() {
    // Knight is `n`, because `k` is taken by the king.
    assert_eq!(PieceKind::Pawn.to_char(), 'p');
    assert_eq!(PieceKind::Knight.to_char(), 'n');
    assert_eq!(PieceKind::Bishop.to_char(), 'b');
    assert_eq!(PieceKind::Rook.to_char(), 'r');
    assert_eq!(PieceKind::Queen.to_char(), 'q');
    assert_eq!(PieceKind::King.to_char(), 'k');
}

// --- Piece -------------------------------------------------------------------

#[test]
fn a_piece_remembers_the_color_and_kind_it_was_built_from() {
    for color in [Color::White, Color::Black] {
        for kind in ALL_KINDS {
            let piece = Piece::new(color, kind);

            assert_eq!(piece.color(), color, "{piece:?} lost its colour");
            assert_eq!(piece.kind(), kind, "{piece:?} lost its kind");
        }
    }
}

#[test]
fn piece_indices_are_color_times_six_plus_kind() {
    // The layout the accessors depend on: white pieces occupy 0..6 and black
    // 6..12, each block in PieceKind order. Zobrist's key table is indexed this
    // way too, so the arithmetic has to hold exactly.
    for color in [Color::White, Color::Black] {
        for kind in ALL_KINDS {
            assert_eq!(
                Piece::new(color, kind).index(),
                color.index() * 6 + kind.index()
            );
        }
    }
}

#[test]
fn the_twelve_pieces_have_twelve_distinct_indices() {
    let mut seen = all_pieces().iter().map(|p| p.index()).collect::<Vec<_>>();
    seen.sort_unstable();
    seen.dedup();

    assert_eq!(seen, (0..12).collect::<Vec<_>>());
}

#[test]
fn white_pieces_print_uppercase_and_black_lowercase() {
    assert_eq!(Piece::new(Color::White, PieceKind::King).to_char(), 'K');
    assert_eq!(Piece::new(Color::Black, PieceKind::King).to_char(), 'k');
    assert_eq!(Piece::new(Color::White, PieceKind::Knight).to_char(), 'N');
    assert_eq!(Piece::new(Color::Black, PieceKind::Pawn).to_char(), 'p');
}

#[test]
fn a_piece_survives_a_round_trip_through_its_fen_letter() {
    for piece in all_pieces() {
        let letter = piece.to_char();
        let back = Piece::from_char(letter).expect("a real piece's letter is a piece letter");

        assert_eq!(back, piece, "{letter} did not round-trip");
    }
}

#[test]
fn non_piece_letters_are_rejected() {
    // `x` is not a piece; the digits and `/` are FEN structure, not pieces; and
    // the parser must not quietly accept them as some default.
    for letter in ['x', 'X', '1', '8', '/', ' ', '-', 'w'] {
        assert_eq!(
            Piece::from_char(letter),
            None,
            "{letter} should not parse as a piece"
        );
    }
}

// --- Representation ----------------------------------------------------------

#[test]
fn an_optional_piece_still_fits_in_one_byte() {
    // This is the test that forces `Piece` to be a flat twelve-variant enum
    // rather than a { color, kind } struct.
    //
    // A `#[repr(u8)]` enum with twelve variants only ever holds 0..12, so the
    // compiler represents `None` using one of the 244 unused bit patterns — a
    // "niche" — instead of adding a discriminant byte. That is what keeps the
    // 64-square mailbox down to exactly one cache line.
    assert_eq!(std::mem::size_of::<Piece>(), 1);
    assert_eq!(std::mem::size_of::<Option<Piece>>(), 1);
    assert_eq!(std::mem::size_of::<[Option<Piece>; 64]>(), 64);
}

#[test]
fn pieces_are_copy_and_comparable() {
    // `[None; 64]` needs Copy, and the whole test suite needs Eq. Cheap to
    // assert, annoying to discover missing halfway through `position.rs`.
    let piece = Piece::new(Color::White, PieceKind::Rook);
    let copied = piece;

    assert_eq!(piece, copied);
    assert_ne!(piece, Piece::new(Color::Black, PieceKind::Rook));
    assert_ne!(piece, Piece::new(Color::White, PieceKind::Queen));
}
