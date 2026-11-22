//! An accumulator updated from a move's dirty pieces is
//! the accumulator refreshed from scratch after the move.

use std::sync::OnceLock;

use eval::dirty_pieces;
use nnue::features::active_features;
use nnue::{Accumulator, L1, Network, RefreshCache};
use shakmaty::fen::Fen;
use shakmaty::uci::UciMove;
use shakmaty::{CastlingMode, Chess, Color, Move, Position};

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

/// `refresh_cache` is shared by the calls of one game, as in a search: its
/// entries are left over from earlier positions, and a king move must still
/// rebuild the exact half from them.
fn assert_update_matches_refresh(
    before: &Chess,
    m: Move,
    refresh_cache: &mut RefreshCache,
    context: &str,
) {
    let net = net();
    let parent = Accumulator::refresh(net, before);
    let dirty = dirty_pieces(before, m);
    let mut after = before.clone();
    after.play_unchecked(m);
    // The search hands `update` a slot still holding an older line's
    // accumulator: the result must not depend on it.
    let mut updated = Accumulator {
        white: [1234; L1],
        black: [-1234; L1],
    };
    parent.update(net, &after, &dirty, &mut updated, refresh_cache);
    assert_eq!(
        updated,
        Accumulator::refresh(net, &after),
        "{context}: {m:?}"
    );
}

#[test]
fn refresh_is_the_bias_plus_one_row_per_active_feature() {
    let network = net();
    let position = pos("4k3/8/8/8/8/8/4P3/4K3 w - - 0 1");
    let accumulator = Accumulator::refresh(network, &position);
    for perspective in [Color::White, Color::Black] {
        let features = active_features(position.board(), perspective);
        assert_eq!(features.len(), 1, "one pawn; kings have no feature");
        let expected: [i16; L1] =
            std::array::from_fn(|i| network.ft_bias[i] + network.ft_row(features[0])[i]);
        assert_eq!(*accumulator.half(perspective), expected, "{perspective:?}");
    }
}

const CASTLES_WHITE: &str = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1";
const CASTLES_BLACK: &str = "r3k2r/8/8/8/8/8/8/R3K2R b KQkq - 0 1";

#[test]
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
        assert_update_matches_refresh(&before, m, &mut RefreshCache::new(net()), name);
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
fn update_matches_refresh_over_random_games() {
    for seed in 1..=100u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut p = Chess::default();
        let mut refresh_cache = RefreshCache::new(net());
        for ply in 0..300 {
            let moves = p.legal_moves();
            if moves.is_empty() {
                break;
            }
            let m = moves[usize::try_from(rng.next() % moves.len() as u64).unwrap()];
            assert_update_matches_refresh(
                &p,
                m,
                &mut refresh_cache,
                &format!("seed {seed}, ply {ply}"),
            );
            p.play_unchecked(m);
        }
    }
}
