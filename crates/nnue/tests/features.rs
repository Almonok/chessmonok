use std::collections::HashSet;

use nnue::L0;
use nnue::features::{active_features, feature_index, orient, piece_index};
use shakmaty::fen::Fen;
use shakmaty::{CastlingMode, Chess, Color, Piece, Position, Role, Square};

use Color::{Black, White};

fn pos(fen: &str) -> Chess {
    fen.parse::<Fen>()
        .unwrap()
        .into_position(CastlingMode::Standard)
        .unwrap()
}

fn piece(color: Color, role: Role) -> Piece {
    Piece { color, role }
}

const ROLES: [Role; 5] = [
    Role::Pawn,
    Role::Knight,
    Role::Bishop,
    Role::Rook,
    Role::Queen,
];

#[test]
fn orient_is_identity_for_white_and_flips_ranks_for_black() {
    for sq in Square::ALL {
        assert_eq!(orient(sq, White), sq.to_usize());
    }
    assert_eq!(orient(Square::A1, Black), Square::A8.to_usize());
    assert_eq!(orient(Square::E2, Black), Square::E7.to_usize());
    assert_eq!(orient(Square::D4, Black), Square::D5.to_usize());
}

#[test]
fn piece_index_is_even_for_own_pieces_and_odd_for_the_enemy() {
    assert_eq!(piece_index(piece(White, Role::Pawn), White), 0);
    assert_eq!(piece_index(piece(Black, Role::Pawn), White), 1);
    assert_eq!(piece_index(piece(Black, Role::Queen), Black), 8);
    assert_eq!(piece_index(piece(White, Role::Queen), Black), 9);
}

#[test]
#[should_panic(expected = "kings have no feature")]
fn kings_have_no_piece_index() {
    piece_index(piece(White, Role::King), White);
}

#[test]
fn hand_computed_indices_match_almonnuet() {
    // The same five cases as almoNnuet's tests/test_features.py.
    let wp = piece(White, Role::Pawn);
    let bq = piece(Black, Role::Queen);
    assert_eq!(feature_index(wp, Square::E2, Square::E1, White), 2577);
    assert_eq!(feature_index(wp, Square::E2, Square::E8, Black), 2681);
    assert_eq!(feature_index(bq, Square::D8, Square::G1, White), 4482);
    assert_eq!(feature_index(wp, Square::A1, Square::A1, White), 1);
    assert_eq!(feature_index(bq, Square::H8, Square::H8, White), L0 - 1);
}

#[test]
fn indices_are_injective_and_in_range_per_perspective() {
    for persp in [White, Black] {
        let mut seen = HashSet::new();
        for king in Square::ALL {
            for color in [White, Black] {
                for role in ROLES {
                    for sq in Square::ALL {
                        let index = feature_index(piece(color, role), sq, king, persp);
                        assert!((1..L0).contains(&index));
                        assert!(seen.insert(index), "{index} produced twice");
                    }
                }
            }
        }
        assert_eq!(seen.len(), 64 * 10 * 64);
    }
}

#[test]
fn the_start_position_has_thirty_sorted_features_per_side() {
    let start = Chess::default();
    for persp in [White, Black] {
        let features = active_features(start.board(), persp);
        assert_eq!(features.len(), 30);
        assert!(features.is_sorted());
    }
}

#[test]
fn bare_kings_have_no_features() {
    let p = pos("4k3/8/8/8/8/8/8/4K3 w - - 0 1");
    assert!(active_features(p.board(), White).is_empty());
    assert!(active_features(p.board(), Black).is_empty());
}

#[test]
fn colour_mirroring_swaps_the_perspectives() {
    // Mirrors computed with python-chess's Board.mirror().
    let pairs = [
        (
            "r1bqkb1r/pppp1ppp/2n2n2/4p2Q/2B1P3/8/PPPP1PPP/RNB1K1NR w KQkq - 0 1",
            "rnb1k1nr/pppp1ppp/8/2b1p3/4P2q/2N2N2/PPPP1PPP/R1BQKB1R b KQkq - 0 1",
        ),
        (
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
            "8/4p1p1/8/1r3P1K/kp5R/3P4/2P5/8 b - - 0 1",
        ),
        (
            "3qk3/8/8/8/8/8/8/6K1 b - - 0 1",
            "6k1/8/8/8/8/8/8/3QK3 w - - 0 1",
        ),
    ];
    for (fen, mirrored) in pairs {
        let (a, b) = (pos(fen), pos(mirrored));
        assert_eq!(
            active_features(a.board(), White),
            active_features(b.board(), Black)
        );
        assert_eq!(
            active_features(a.board(), Black),
            active_features(b.board(), White)
        );
    }
}
