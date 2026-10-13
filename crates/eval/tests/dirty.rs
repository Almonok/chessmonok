use eval::{DirtyPiece, DirtyPieces, dirty_pieces};
use shakmaty::fen::Fen;
use shakmaty::uci::UciMove;
use shakmaty::{CastlingMode, Chess, Color, Move, Piece, Position, Role, Square};

fn pos(fen: &str) -> Chess {
    fen.parse::<Fen>()
        .unwrap()
        .into_position(CastlingMode::Standard)
        .unwrap()
}

fn mv(p: &Chess, uci: &str) -> Move {
    uci.parse::<UciMove>().unwrap().to_move(p).unwrap()
}

fn dp(color: Color, role: Role, from: Option<Square>, to: Option<Square>) -> DirtyPiece {
    DirtyPiece {
        piece: Piece { color, role },
        from,
        to,
    }
}

fn assert_dirty(d: &DirtyPieces, expected: &[DirtyPiece]) {
    assert_eq!(d.len(), expected.len(), "{d:?}");
    for e in expected {
        assert!(d.contains(e), "missing {e:?} in {d:?}");
    }
}

use Color::{Black, White};
use Role::{King, Knight, Pawn, Queen, Rook};
use Square::*;

#[test]
fn quiet_pawn_push() {
    let p = Chess::default();
    assert_dirty(
        &dirty_pieces(&p, mv(&p, "e2e4")),
        &[dp(White, Pawn, Some(E2), Some(E4))],
    );
}

#[test]
fn capture() {
    let p = pos("rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2");
    assert_dirty(
        &dirty_pieces(&p, mv(&p, "e4d5")),
        &[
            dp(White, Pawn, Some(E4), Some(D5)),
            dp(Black, Pawn, Some(D5), None),
        ],
    );
}

#[test]
fn en_passant() {
    let p = pos("rnbqkbnr/ppp1p1pp/8/3pPp2/8/8/PPPP1PPP/RNBQKBNR w KQkq f6 0 3");
    assert_dirty(
        &dirty_pieces(&p, mv(&p, "e5f6")),
        &[
            dp(White, Pawn, Some(E5), Some(F6)),
            dp(Black, Pawn, Some(F5), None),
        ],
    );
}

const CASTLES: &str = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1";

#[test]
fn white_castles_both_sides() {
    let p = pos(CASTLES);
    assert_dirty(
        &dirty_pieces(&p, mv(&p, "e1g1")),
        &[
            dp(White, King, Some(E1), Some(G1)),
            dp(White, Rook, Some(H1), Some(F1)),
        ],
    );
    assert_dirty(
        &dirty_pieces(&p, mv(&p, "e1c1")),
        &[
            dp(White, King, Some(E1), Some(C1)),
            dp(White, Rook, Some(A1), Some(D1)),
        ],
    );
}

#[test]
fn black_castles_both_sides() {
    let p = pos("r3k2r/8/8/8/8/8/8/R3K2R b KQkq - 0 1");
    assert_dirty(
        &dirty_pieces(&p, mv(&p, "e8g8")),
        &[
            dp(Black, King, Some(E8), Some(G8)),
            dp(Black, Rook, Some(H8), Some(F8)),
        ],
    );
    assert_dirty(
        &dirty_pieces(&p, mv(&p, "e8c8")),
        &[
            dp(Black, King, Some(E8), Some(C8)),
            dp(Black, Rook, Some(A8), Some(D8)),
        ],
    );
}

#[test]
fn promotion() {
    let p = pos("8/4P3/8/8/8/8/k7/4K3 w - - 0 1");
    assert_dirty(
        &dirty_pieces(&p, mv(&p, "e7e8q")),
        &[
            dp(White, Pawn, Some(E7), None),
            dp(White, Queen, None, Some(E8)),
        ],
    );
}

#[test]
fn capture_promotion() {
    let p = pos("3r4/4P3/8/8/8/8/k7/4K3 w - - 0 1");
    assert_dirty(
        &dirty_pieces(&p, mv(&p, "e7d8n")),
        &[
            dp(White, Pawn, Some(E7), None),
            dp(Black, Rook, Some(D8), None),
            dp(White, Knight, None, Some(D8)),
        ],
    );
}

#[test]
fn black_promotion() {
    let p = pos("4k3/8/8/8/8/8/p7/4K3 b - - 0 1");
    assert_dirty(
        &dirty_pieces(&p, mv(&p, "a2a1q")),
        &[
            dp(Black, Pawn, Some(A2), None),
            dp(Black, Queen, None, Some(A1)),
        ],
    );
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn mailbox(p: &Chess) -> [Option<Piece>; 64] {
    let mut board = [None; 64];
    for (sq, piece) in p.board() {
        board[sq.to_usize()] = Some(piece);
    }
    board
}

#[test]
fn replaying_dirty_pieces_tracks_the_board_over_random_games() {
    for seed in 1..=100u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut p = Chess::default();
        let mut board = mailbox(&p);
        for _ in 0..300 {
            let moves = p.legal_moves();
            if moves.is_empty() {
                break;
            }
            let m = moves[(rng.next() % moves.len() as u64) as usize];
            let dirty = dirty_pieces(&p, m);
            for d in dirty.iter().filter(|d| d.from.is_some()) {
                let from = d.from.unwrap().to_usize();
                assert_eq!(board[from], Some(d.piece), "seed {seed}, {m:?}");
                board[from] = None;
            }
            for d in dirty.iter().filter(|d| d.to.is_some()) {
                board[d.to.unwrap().to_usize()] = Some(d.piece);
            }
            p.play_unchecked(m);
            assert_eq!(board, mailbox(&p), "seed {seed}, after {m:?}");
        }
    }
}
