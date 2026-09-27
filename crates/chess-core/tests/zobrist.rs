//! Spec for exercise 02, part 2 — see `docs/exercises/02-pieces-and-position.md`.
//!
//! The position hash. There are no moves yet, so nothing here can test the
//! incremental update that is the whole point of the scheme — that lands with
//! make/unmake. What these tests pin down is the part that would be painful to
//! change afterwards: which facts about a position the hash is a function of.

use chess_core::Position;

fn pos(fen: &str) -> Position {
    Position::from_fen(fen).expect("test FENs are well-formed")
}

fn hash(fen: &str) -> u64 {
    pos(fen).hash()
}

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

// --- The basics --------------------------------------------------------------

#[test]
fn a_position_hashes_to_something() {
    // A hash of zero usually means the key table never got XORed in at all.
    assert_ne!(hash(START), 0);
}

#[test]
fn hashing_is_deterministic_within_a_run() {
    let position = pos(START);

    assert_eq!(position.hash(), position.hash());
    assert_eq!(position.hash(), Position::starting().hash());
}

#[test]
fn the_keys_do_not_change_between_builds() {
    // The table must be generated at compile time from a literal seed, never
    // from a random source — a hash written to a file or shared between
    // processes is meaningless if the keys differ per run.
    //
    // A test cannot observe two builds, so this only checks the observable half:
    // the keys are not re-randomised per `Position`. Once your implementation is
    // green, paste the printed value into the `assert_eq!` below and uncomment
    // it — then any accidental change to the generator becomes a failing test
    // rather than a silent invalidation of every stored hash.
    let start = hash(START);
    println!("starting position hash: {start:#018x}");

    assert_eq!(start, Position::starting().hash());
    assert_eq!(start, pos(START).hash(), "a fresh parse must hash the same");
    // assert_eq!(start, 0x____________________);
}

// --- What the hash is a function of ------------------------------------------

#[test]
fn different_piece_placement_hashes_differently() {
    assert_ne!(hash(START), hash("4k3/8/8/8/8/8/8/4K3 w - - 0 1"));

    // One pawn moved.
    assert_ne!(
        hash(START),
        hash("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 1")
    );
}

#[test]
fn moving_a_piece_and_moving_it_back_restores_the_hash() {
    // The property the whole XOR scheme rests on: a key XORed in twice cancels.
    // Here the "move" is done by writing two FENs rather than by make_move, but
    // the invariant is the same one make_move will rely on.
    let home = hash("4k3/8/8/8/8/8/8/R3K3 w - - 0 1");
    let moved = hash("4k3/8/8/8/8/8/8/3RK3 w - - 0 1");

    assert_ne!(home, moved);
    assert_eq!(home, hash("4k3/8/8/8/8/8/8/R3K3 w - - 0 1"));
}

#[test]
fn the_side_to_move_is_part_of_the_hash() {
    // Same board, different player to move: genuinely different positions, and
    // the transposition table must not confuse them.
    let board = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

    assert_ne!(
        hash(&format!("{board} w KQkq - 0 1")),
        hash(&format!("{board} b KQkq - 0 1"))
    );
}

#[test]
fn castling_rights_are_part_of_the_hash() {
    let board = "r3k2r/8/8/8/8/8/8/R3K2R";

    let all = hash(&format!("{board} w KQkq - 0 1"));
    let some = hash(&format!("{board} w KQk - 0 1"));
    let none = hash(&format!("{board} w - - 0 1"));

    assert_ne!(all, some, "losing a right must change the hash");
    assert_ne!(all, none);
    assert_ne!(some, none);
}

#[test]
fn the_en_passant_square_is_part_of_the_hash() {
    let board = "rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR";

    assert_ne!(
        hash(&format!("{board} w KQkq - 0 2")),
        hash(&format!("{board} w KQkq c6 0 2"))
    );
}

#[test]
fn en_passant_on_different_files_hashes_differently() {
    // Keyed by file, not by square — but different files must still separate.
    let after_e4 = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1";
    let after_d4 = "rnbqkbnr/pppppppp/8/8/3P4/8/PPP1PPPP/RNBQKBNR b KQkq d3 0 1";

    assert_ne!(hash(after_e4), hash(after_d4));
}

// --- What the hash is deliberately NOT a function of -------------------------

#[test]
fn the_clocks_are_not_part_of_the_hash() {
    // This is a decision, not an oversight.
    //
    // The transposition table stores a search result for a position, and that
    // result depends on the pieces, the rights and whose turn it is — not on how
    // long it has been since a pawn moved. Including the clocks would make two
    // identical positions reached by different move orders miss each other in
    // the table, which is exactly the saving the table exists for.
    //
    // The cost is that the fifty-move rule has to be handled by the search
    // itself rather than falling out of the hash. That is the standard trade.
    let board = "r1bq1rk1/pp2ppbp/2np1np1/8/2BNP3/2N1BP2/PPPQ2PP/R3K2R w KQ -";

    assert_eq!(
        hash(&format!("{board} 0 10")),
        hash(&format!("{board} 40 60")),
        "the halfmove clock and fullmove number must not reach the hash"
    );
}

// --- Spread ------------------------------------------------------------------

#[test]
fn distinct_positions_get_distinct_hashes() {
    // Not a proof of anything — 64-bit collisions are a fact of life and engines
    // live with them — but a table built from a broken PRNG (all zeros, or a
    // short cycle) fails this instantly.
    let fens = [
        START,
        "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
        "rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w KQkq c6 0 2",
        "r1bq1rk1/pp2ppbp/2np1np1/8/2BNP3/2N1BP2/PPPQ2PP/R3K2R w KQ - 0 10",
        "4k3/8/8/8/8/8/8/4K3 w - - 0 1",
        "4k3/8/8/8/8/8/8/4K3 b - - 0 1",
        "8/8/4k3/8/8/1K6/8/8 w - - 0 1",
        "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
        "r3k2r/8/8/8/8/8/8/R3K2R w Kq - 0 1",
        "r3k2r/8/8/8/8/8/8/R3K2R b - - 0 1",
        "8/2k5/8/8/8/8/5K2/8 w - - 0 1",
        "8/2k5/8/8/8/8/5K2/8 b - - 0 1",
    ];

    let mut hashes: Vec<u64> = fens.iter().copied().map(hash).collect();
    let total = hashes.len();

    hashes.sort_unstable();
    hashes.dedup();

    assert_eq!(hashes.len(), total, "two distinct positions collided");
}

#[test]
fn every_piece_on_every_square_has_its_own_key() {
    // Walks one rook across all 64 squares and checks no two placements agree.
    // A table indexed wrongly — square and piece transposed, say — shows up here
    // as a pile of collisions rather than as a subtle search bug months later.
    let mut hashes = Vec::new();

    for index in 0..64u8 {
        let square = chess_core::Square::from_index(index).expect("0..64 is in range");

        // Skip the squares the two kings need.
        if square.to_string() == "a1" || square.to_string() == "h8" {
            continue;
        }

        // Kings on a1 and h8, the rook on `square`.
        let mut ranks = vec![String::new(); 8];
        let mut board = [[' '; 8]; 8];
        board[0][0] = 'K';
        board[7][7] = 'k';
        board[(index / 8) as usize][(index % 8) as usize] = 'R';

        for (rank, row) in board.iter().enumerate() {
            let mut text = String::new();
            let mut empties = 0;

            for piece in row {
                if *piece == ' ' {
                    empties += 1;
                } else {
                    if empties > 0 {
                        text.push_str(&empties.to_string());
                        empties = 0;
                    }
                    text.push(*piece);
                }
            }
            if empties > 0 {
                text.push_str(&empties.to_string());
            }
            ranks[7 - rank] = text;
        }

        hashes.push(hash(&format!("{} w - - 0 1", ranks.join("/"))));
    }

    let total = hashes.len();
    hashes.sort_unstable();
    hashes.dedup();

    assert_eq!(hashes.len(), total, "a rook on two squares hashed the same");
}
