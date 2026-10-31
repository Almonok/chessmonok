//! Exercise N2's spec: an accumulator updated from a move's dirty pieces is
//! the accumulator refreshed from scratch after the move.

use std::sync::OnceLock;

use eval::dirty_pieces;
use nnue::{Accumulator, Network};
use shakmaty::fen::Fen;
use shakmaty::uci::UciMove;
use shakmaty::{CastlingMode, Chess, Move, Position};

fn net() -> &'static Network {
    static NET: OnceLock<Network> = OnceLock::new();
    NET.get_or_init(|| Network::random(7))
}

fn pos(fen: &str) -> Chess {
    fen.parse::<Fen>()
        .unwrap()
        .into_position(CastlingMode::Standard)
        .unwrap()
}

fn assert_update_matches_refresh(before: &Chess, m: Move, context: &str) {
    let net = net();
    let parent = Accumulator::refresh(net, before);
    let dirty = dirty_pieces(before, m);
    let mut after = before.clone();
    after.play_unchecked(m);
    let updated = parent.update(net, &after, &dirty);
    assert_eq!(
        updated,
        Accumulator::refresh(net, &after),
        "{context}: {m:?}"
    );
}

const CASTLES_WHITE: &str = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1";
const CASTLES_BLACK: &str = "r3k2r/8/8/8/8/8/8/R3K2R b KQkq - 0 1";

#[test]
#[ignore = "exercise N2"]
fn every_kind_of_move_updates_exactly() {
    let cases = [
        (
            "quiet pawn push",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "e2e4",
        ),
        (
            "capture",
            "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2",
            "e4d5",
        ),
        (
            "white en passant",
            "rnbqkbnr/ppp1p1pp/8/3pPp2/8/8/PPPP1PPP/RNBQKBNR w KQkq f6 0 3",
            "e5f6",
        ),
        (
            "black en passant",
            "rnbqkbnr/pppp1ppp/8/8/3Pp3/8/PPP1PPPP/RNBQKBNR b KQkq d3 0 2",
            "e4d3",
        ),
        ("white castles short", CASTLES_WHITE, "e1g1"),
        ("white castles long", CASTLES_WHITE, "e1c1"),
        ("black castles short", CASTLES_BLACK, "e8g8"),
        ("black castles long", CASTLES_BLACK, "e8c8"),
        ("promotion", "8/P6k/8/8/8/8/8/K7 w - - 0 1", "a7a8q"),
        (
            "capturing under-promotion",
            "1r5k/P7/8/8/8/8/8/K7 w - - 0 1",
            "a7b8n",
        ),
        (
            "black capturing promotion",
            "k7/8/8/8/8/8/p7/1R5K b - - 0 1",
            "a2b1q",
        ),
        ("white king move", "4k3/8/8/8/8/8/8/4K2R w K - 0 1", "e1e2"),
        (
            "white king captures",
            "4k3/8/8/8/8/8/3p4/4K3 w - - 0 1",
            "e1d2",
        ),
        (
            "black king captures",
            "4k3/3P4/8/8/8/8/8/4K3 b - - 0 1",
            "e8d7",
        ),
    ];
    for (name, fen, uci) in cases {
        let before = pos(fen);
        let m = uci.parse::<UciMove>().unwrap().to_move(&before).unwrap();
        assert_update_matches_refresh(&before, m, name);
    }
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

#[test]
#[ignore = "exercise N2"]
fn update_matches_refresh_over_random_games() {
    for seed in 1..=100u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut p = Chess::default();
        for ply in 0..300 {
            let moves = p.legal_moves();
            if moves.is_empty() {
                break;
            }
            let m = moves[usize::try_from(rng.next() % moves.len() as u64).unwrap()];
            assert_update_matches_refresh(&p, m, &format!("seed {seed}, ply {ply}"));
            p.play_unchecked(m);
        }
    }
}
