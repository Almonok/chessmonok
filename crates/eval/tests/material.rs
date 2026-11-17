use eval::{Evaluator, MaterialEval, dirty_pieces};
use shakmaty::fen::Fen;
use shakmaty::uci::UciMove;
use shakmaty::{CastlingMode, Chess, Move, Position};

fn pos(fen: &str) -> Chess {
    fen.parse::<Fen>()
        .unwrap()
        .into_position(CastlingMode::Standard)
        .unwrap()
}

fn eval(p: &Chess) -> i32 {
    let e = MaterialEval;
    e.evaluate(&e.refresh(p), p)
}

#[test]
fn start_position_is_equal() {
    assert_eq!(eval(&Chess::default()), 0);
}

#[test]
fn queen_up_from_both_sides_to_move() {
    assert_eq!(eval(&pos("3qk3/8/8/8/8/8/8/3QK2Q w - - 0 1")), 900);
    assert_eq!(eval(&pos("3qk3/8/8/8/8/8/8/3QK2Q b - - 0 1")), -900);
}

#[test]
fn piece_values() {
    assert_eq!(
        eval(&pos("4k3/8/8/8/8/8/PNBR4/4K3 w - - 0 1")),
        100 + 320 + 330 + 500
    );
}

fn incremental<E: Evaluator>(e: &E, before: &Chess, m: Move, after: &Chess) -> i32 {
    let parent = e.refresh(before);
    let mut state = e.refresh(before);
    e.update(&parent, &mut state, after, &dirty_pieces(before, m));
    e.evaluate(&state, after)
}

#[test]
fn update_after_a_capture_matches_refresh() {
    let before = pos("rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2");
    let m = "e4d5".parse::<UciMove>().unwrap().to_move(&before).unwrap();
    let mut after = before.clone();
    after.play_unchecked(m);
    assert_eq!(incremental(&MaterialEval, &before, m, &after), eval(&after));
    assert_eq!(eval(&after), -100);
}
