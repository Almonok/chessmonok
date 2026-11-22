//! The transposition table: what the search already learnt about a position.
//!
//! # Why a chess engine needs one
//!
//! Alpha-beta explores a *tree*, but chess positions form a *graph*: many
//! move orders reach the same position (`1.Nf3 Nc6 2.Nc3` and
//! `1.Nc3 Nc6 2.Nf3`), and iterative deepening searches the whole tree again
//! at every new depth. Without memory, every one of those visits starts from
//! nothing. The transposition table remembers, per position, what the last
//! search of it found:
//!
//! - **a score**, which can end the search of that node at once when it was
//!   searched at least as deep as needed now (a *cutoff*);
//! - **the best move**, which is searched first next time. This is the bigger
//!   win in practice: alpha-beta prunes most when the best move comes first,
//!   and the move that was best at depth 6 is usually best at depth 7.
//!
//! # What a score means: the bound
//!
//! Alpha-beta rarely learns a node's exact value. It searches a window
//! `(alpha, beta)` and only learns how the true value compares with it:
//!
//! ```text
//!   true value ≤ score       true value = score       true value ≥ score
//!   ── Upper ──────────┤ alpha ──── Exact ──── beta ├────────── Lower ──
//!   no move beat alpha       a move landed inside     a move reached beta
//!                                                     (and was cut off)
//! ```
//!
//! A stored score can only be reused when its bound answers the new question:
//! an upper bound at or below the new alpha, a lower bound at or above the new
//! beta, or an exact score.
//!
//! # Layout: buckets of four 16-byte slots
//!
//! The table is one fixed-size array (the UCI `Hash` option, in MB) of
//! buckets, indexed by the position's 64-bit Zobrist key (see
//! `position_key`). A bucket is four slots, 64 bytes aligned on 64 bytes:
//!
//! ```text
//!   bucket = | slot | slot | slot | slot |   64 bytes, one cache line
//! ```
//!
//! Every lookup is a memory access far from anything recently used, which
//! costs about as much as hundreds of instructions. Reading one slot or four
//! from the same cache line costs the same single access, so a position may
//! live in any of its bucket's four slots: a lookup checks all four, and a
//! store chooses which to overwrite (see `store`). With one slot per
//! position, two positions whose keys land on the same slot evicted each
//! other at once; now four can share an address before anything is lost,
//! and what is lost is the least valuable of them.
//!
//! Each slot holds one entry, packed into one `u64` of data:
//!
//! ```text
//!  bits  0..16   best move (PackedMove; 0 = none)
//!  bits 16..32   score (i16)
//!  bits 32..40   depth searched
//!  bits 40..48   bound (1 upper, 2 lower, 3 exact; 0 = empty slot)
//!  bits 48..56   generation (which `go` wrote it)
//! ```
//!
//! and a second `u64` that is the key XOR the data. The full key is kept so
//! that two positions sharing a bucket are told apart; a wrong match would
//! hand the search the score of a different position.
//!
//! # Lock-free from the start
//!
//! The search is single-threaded today, but engines get most of their
//! multi-core strength from threads that share one table ("Lazy SMP"). A
//! lock per slot would cost more than the lookup itself, so slots are written
//! without locks, and a slot written by two threads at once could end up with
//! one thread's key and the other's data. Storing `key ^ data` instead of the
//! key catches that: reading back recomputes `key = (key ^ data) ^ data`, and
//! a mismatched pair yields a key that matches no probe, so it reads as an
//! empty slot instead of wrong data. (This is Hyatt and Mann's "lockless
//! hashing", used by Crafty and Stockfish.)

use std::fmt;
use std::num::NonZeroU16;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use shakmaty::Move;

use crate::{MATE_BOUND, ply_offset};

/// The UCI `Hash` option's default, in megabytes.
pub const DEFAULT_HASH_MEGABYTES: usize = 16;
/// The largest `Hash` the engine accepts, in megabytes.
pub const MAX_HASH_MEGABYTES: usize = 1024;

/// How many slots `hashfull` looks at: enough to estimate the fill to a few
/// per mille, few enough to cost nothing once per iteration.
const HASHFULL_SAMPLE: usize = 1000;

/// Slots per bucket: four 16-byte slots fill a 64-byte cache line.
const SLOTS_PER_BUCKET: usize = 4;

/// When a bucket is full, the entry evicted is the one with the least
/// `depth - AGE_PENALTY_PLIES x age`, where age counts the searches since it
/// was written. An entry from the previous move must be 8 plies deeper than
/// a current one to outlive it: deep results stay useful a move or two
/// later (most of the next move's tree was searched during this one), but
/// not forever.
const AGE_PENALTY_PLIES: i32 = 8;

/// A move in 16 bits: origin square, target square, promotion.
///
/// A shakmaty `Move` is a rich enum (piece, capture, castling...), too big to
/// pack next to a score in one `u64`. Origin, target and promotion are enough
/// to tell apart every legal move of a position, so the search turns a packed
/// move back into a full `Move` by finding the legal move that packs the
/// same. A packed move is therefore only a hint: a move that is not legal in
/// the position (say, after a key collision) matches nothing and is ignored.
///
/// It wraps a `NonZeroU16`: no move goes from a1 to a1, so zero is free to
/// mean "no move" in the packed data, and the compiler uses that same niche
/// to make `Option<PackedMove>` 2 bytes instead of 4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackedMove(NonZeroU16);

impl PackedMove {
    pub fn new(played: Move) -> Self {
        let from = played
            .from()
            .expect("standard chess moves have an origin square");
        let promotion = played.promotion().map_or(0, |role| u16::from(role as u8));
        let bits =
            u16::from(u8::from(from)) | u16::from(u8::from(played.to())) << 6 | promotion << 12;
        Self(NonZeroU16::new(bits).expect("a move never goes from a1 to a1"))
    }

    fn from_bits(bits: u16) -> Option<Self> {
        NonZeroU16::new(bits).map(Self)
    }

    fn bits(self) -> u16 {
        self.0.get()
    }
}

/// How the stored score relates to the position's true value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bound {
    /// Every move failed low: the true value is at most the score.
    Upper = 1,
    /// A move reached beta and the search stopped there: the true value is at
    /// least the score.
    Lower = 2,
    /// The true value is the score.
    Exact = 3,
}

impl Bound {
    fn from_bits(bits: u8) -> Option<Self> {
        match bits {
            1 => Some(Bound::Upper),
            2 => Some(Bound::Lower),
            3 => Some(Bound::Exact),
            _ => None,
        }
    }
}

/// What the table remembers about one position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableEntry {
    pub best_move: Option<PackedMove>,
    /// The score as the table stores it: mate distances count from this
    /// position, not from the root (see `score_to_table`).
    pub score: i32,
    /// The depth this score was searched to.
    pub depth: u32,
    pub bound: Bound,
}

impl TableEntry {
    fn pack(self, generation: u8) -> u64 {
        let move_bits = self.best_move.map_or(0, PackedMove::bits);
        let score = i16::try_from(self.score).expect("scores fit in an i16: |score| <= MATE");
        let depth = u8::try_from(self.depth).unwrap_or(u8::MAX);
        u64::from(move_bits)
            | u64::from(u16::from_ne_bytes(score.to_ne_bytes())) << 16
            | u64::from(depth) << 32
            | u64::from(self.bound as u8) << 40
            | u64::from(generation) << 48
    }

    /// The entry and its generation, or `None` for an empty slot.
    fn unpack(data: u64) -> Option<(Self, u8)> {
        let bound = Bound::from_bits((data >> 40) as u8)?;
        let entry = TableEntry {
            best_move: PackedMove::from_bits(data as u16),
            score: i32::from(i16::from_ne_bytes(((data >> 16) as u16).to_ne_bytes())),
            depth: u32::from((data >> 32) as u8),
            bound,
        };
        Some((entry, (data >> 48) as u8))
    }
}

/// One slot: an entry's data, and the position's key XOR that data.
///
/// `AtomicU64` with `Ordering::Relaxed` is not about synchronising threads
/// here (the XOR check handles torn slots). It is what makes a concurrent
/// read and write of the same slot defined behaviour at all: in Rust, two
/// threads touching a plain `u64` at once is a data race, which is undefined
/// behaviour even if the result is thrown away. Relaxed atomic loads and
/// stores of a `u64` compile to the same single instructions as plain ones on
/// x86-64 and ARM64, so the guarantee is free.
#[derive(Default)]
struct Slot {
    key_xor_data: AtomicU64,
    data: AtomicU64,
}

impl Slot {
    /// `(key, data)`. The key is meaningless for a torn or empty slot, so it
    /// matches no probe.
    fn load(&self) -> (u64, u64) {
        let key_xor_data = self.key_xor_data.load(Ordering::Relaxed);
        let data = self.data.load(Ordering::Relaxed);
        (key_xor_data ^ data, data)
    }

    fn save(&self, key: u64, data: u64) {
        self.key_xor_data.store(key ^ data, Ordering::Relaxed);
        self.data.store(data, Ordering::Relaxed);
    }
}

/// Four slots read with one memory access.
///
/// `#[repr(align(64))]` makes the compiler place every bucket at an address
/// that is a multiple of 64, and pad it to 64 bytes: a bucket then never
/// straddles two cache lines (on x86-64 they are 64 bytes; on Apple's ARM
/// cores 128, which 64 divides). Without it a `[Slot; 4]` is only 8-byte
/// aligned, and every other bucket would cost two memory accesses.
#[repr(align(64))]
#[derive(Default)]
struct Bucket {
    slots: [Slot; SLOTS_PER_BUCKET],
}

/// A fixed-size table shared by reference: every method takes `&self`, so
/// threads can later share one table through an `Arc` with no lock.
pub struct TranspositionTable {
    buckets: Box<[Bucket]>,
    /// Bumped by every search, so entries left by earlier searches can be
    /// told apart and overwritten first. It wraps after 256 searches; an
    /// entry that old is then mistaken for a fresh one, which costs at worst
    /// a slot kept a little longer than ideal.
    generation: AtomicU8,
}

impl fmt::Debug for TranspositionTable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TranspositionTable")
            .field("buckets", &self.buckets.len())
            .field("generation", &self.generation)
            .finish()
    }
}

impl TranspositionTable {
    /// A table of `megabytes` MB (at least one bucket), all slots empty.
    pub fn new(megabytes: usize) -> Self {
        let bucket_count = (megabytes * 1024 * 1024 / size_of::<Bucket>()).max(1);
        Self {
            buckets: (0..bucket_count).map(|_| Bucket::default()).collect(),
            generation: AtomicU8::new(0),
        }
    }

    pub fn slot_count(&self) -> usize {
        self.buckets.len() * SLOTS_PER_BUCKET
    }

    fn slots(&self) -> impl Iterator<Item = &Slot> {
        self.buckets.iter().flat_map(|bucket| &bucket.slots)
    }

    /// Empty every slot: `ucinewgame`, so one game's analysis cannot steer
    /// the next (and test results do not depend on what ran before).
    pub fn clear(&self) {
        for slot in self.slots() {
            slot.save(0, 0);
        }
    }

    /// Start a new search: from now on, entries written before are stale and
    /// are the first to be replaced.
    pub fn new_search(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    /// What the table knows about the position with this key, if anything:
    /// any slot of its bucket may hold it.
    pub fn probe(&self, key: u64) -> Option<TableEntry> {
        self.bucket(key).slots.iter().find_map(|slot| {
            let (stored_key, data) = slot.load();
            if stored_key != key {
                return None;
            }
            TableEntry::unpack(data).map(|(entry, _generation)| entry)
        })
    }

    /// Remember `entry` for the position with this key.
    ///
    /// If the bucket already holds this position, that slot is updated, but
    /// a deeper result from the same search stays: it cost more nodes to
    /// produce and its score can answer more future probes (a depth-8 result
    /// can cut a depth-5 search, not the other way round). Otherwise the new
    /// entry takes the bucket's least valuable slot: an empty one, else the
    /// one with the least depth once age is counted (`AGE_PENALTY_PLIES`).
    pub fn store(&self, key: u64, entry: TableEntry) {
        let bucket = self.bucket(key);
        let generation = self.generation.load(Ordering::Relaxed);
        let mut entry = entry;
        for slot in &bucket.slots {
            let (stored_key, stored_data) = slot.load();
            if stored_key != key {
                continue;
            }
            if let Some((stored, stored_generation)) = TableEntry::unpack(stored_data) {
                if stored_generation == generation && entry.depth < stored.depth {
                    return;
                }
                // A fail-low finds no best move. If this same position had
                // one, it is still the best guess of what to try first.
                if entry.best_move.is_none() {
                    entry.best_move = stored.best_move;
                }
            }
            slot.save(key, entry.pack(generation));
            return;
        }
        let least_valuable = bucket
            .slots
            .iter()
            .min_by_key(|slot| worth(slot, generation))
            .expect("a bucket has slots");
        least_valuable.save(key, entry.pack(generation));
    }

    /// How full the table is with entries from the current search, in per
    /// mille, as UCI's `info hashfull` reports it. Estimated from the first
    /// slots: keys are spread uniformly, so they are a fair sample.
    pub fn hashfull(&self) -> u32 {
        let generation = self.generation.load(Ordering::Relaxed);
        let sample_size = self.slot_count().min(HASHFULL_SAMPLE);
        let current = self
            .slots()
            .take(sample_size)
            .filter(|slot| {
                let (_key, data) = slot.load();
                TableEntry::unpack(data).is_some_and(|(_entry, written)| written == generation)
            })
            .count();
        (current * 1000 / sample_size) as u32
    }

    /// The bucket for `key`.
    ///
    /// `key * bucket_count / 2^64` maps a uniform 64-bit key to a uniform
    /// index in `0..bucket_count` with one multiplication (Lemire's "fast
    /// range"), where `key % bucket_count` would need a division, several
    /// times slower. It also frees the table from power-of-two sizes, so
    /// `Hash` can be any number of MB.
    fn bucket(&self, key: u64) -> &Bucket {
        let index = (u128::from(key) * self.buckets.len() as u128) >> 64;
        &self.buckets[index as usize]
    }
}

/// How much a slot is worth keeping: its depth, less `AGE_PENALTY_PLIES`
/// per search since it was written. An empty slot is worth nothing.
fn worth(slot: &Slot, generation: u8) -> i32 {
    let (_key, data) = slot.load();
    TableEntry::unpack(data).map_or(i32::MIN, |(entry, written)| {
        // Wrapping: the generation counter wraps after 256 searches, and the
        // age is still right as long as no entry survives 256 searches.
        let age = i32::from(generation.wrapping_sub(written));
        i32::try_from(entry.depth).unwrap_or(i32::MAX) - AGE_PENALTY_PLIES * age
    })
}

/// A score as the table stores it: mate distances counted from `ply`, the
/// position being stored, instead of from the root.
///
/// The search scores "mated in N" as `-MATE + (plies from the root)`, so that
/// shorter mates score higher. But a table entry is reused wherever the same
/// position appears, at any ply. Stored as is, a mate found 3 plies below the
/// root and reused 7 plies below it would claim the mate is 4 plies nearer
/// (or farther) than it is: the engine could then pick a slower mate, or walk
/// into a faster one. Shifting by `ply` on store and back on probe fixes it.
pub fn score_to_table(score: i32, ply: usize) -> i32 {
    let ply = ply_offset(ply);
    if score >= MATE_BOUND {
        score + ply
    } else if score <= -MATE_BOUND {
        score - ply
    } else {
        score
    }
}

/// The inverse of `score_to_table`: a stored score seen from `ply`.
pub fn score_from_table(score: i32, ply: usize) -> i32 {
    let ply = ply_offset(ply);
    if score >= MATE_BOUND {
        score - ply
    } else if score <= -MATE_BOUND {
        score + ply
    } else {
        score
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MATE;
    use shakmaty::fen::Fen;
    use shakmaty::{CastlingMode, Chess, Position};

    fn entry(depth: u32, score: i32, bound: Bound) -> TableEntry {
        TableEntry {
            best_move: None,
            score,
            depth,
            bound,
        }
    }

    fn some_move() -> PackedMove {
        let e4 = Chess::default()
            .legal_moves()
            .into_iter()
            .find(|candidate| candidate.to().to_string() == "e4")
            .unwrap();
        PackedMove::new(e4)
    }

    const KEY: u64 = 0x9E37_79B9_7F4A_7C15;

    #[test]
    fn a_stored_entry_comes_back_unchanged() {
        let table = TranspositionTable::new(1);
        let stored = TableEntry {
            best_move: Some(some_move()),
            score: -1234,
            depth: 9,
            bound: Bound::Lower,
        };
        table.store(KEY, stored);
        assert_eq!(table.probe(KEY), Some(stored));
    }

    #[test]
    fn an_empty_table_knows_nothing_even_for_key_zero() {
        // An empty slot is all zeros, so its recomputed key is 0: the empty
        // bound, not the key, is what makes it a miss.
        let table = TranspositionTable::new(1);
        assert_eq!(table.probe(0), None);
        assert_eq!(table.probe(KEY), None);
    }

    /// Keys differing only in their lowest bits: the high bits, which pick
    /// the bucket, are the same, so all of them share `KEY`'s bucket.
    fn neighbours(count: u64) -> Vec<u64> {
        (1..=count).map(|low_bits| KEY ^ low_bits).collect()
    }

    #[test]
    fn another_position_in_the_same_bucket_is_a_miss() {
        let table = TranspositionTable::new(1);
        let neighbour = neighbours(1)[0];
        assert!(std::ptr::eq(table.bucket(KEY), table.bucket(neighbour)));
        table.store(KEY, entry(3, 50, Bound::Exact));
        assert_eq!(table.probe(neighbour), None);
    }

    #[test]
    fn four_positions_share_a_bucket() {
        let table = TranspositionTable::new(1);
        let keys = neighbours(4);
        for (depth, &key) in (1..).zip(&keys) {
            table.store(key, entry(depth, 0, Bound::Exact));
        }
        for (depth, &key) in (1..).zip(&keys) {
            assert_eq!(table.probe(key).map(|found| found.depth), Some(depth));
        }
    }

    #[test]
    fn a_full_bucket_evicts_its_shallowest_entry() {
        let table = TranspositionTable::new(1);
        let keys = neighbours(5);
        for (&key, depth) in keys.iter().zip([5, 2, 7, 9]) {
            table.store(key, entry(depth, 0, Bound::Exact));
        }
        table.store(keys[4], entry(1, 0, Bound::Exact));
        assert_eq!(table.probe(keys[1]), None, "depth 2 was the least valuable");
        for kept in [keys[0], keys[2], keys[3], keys[4]] {
            assert!(table.probe(kept).is_some());
        }
    }

    #[test]
    fn an_old_entry_gives_way_unless_much_deeper() {
        let table = TranspositionTable::new(1);
        let keys = neighbours(5);
        // Depth 10 from the previous search is worth 10 - 8 = 2 now: less
        // than the current depth-4 entries, more than nothing.
        table.store(keys[0], entry(10, 0, Bound::Exact));
        table.new_search();
        for &key in &keys[1..4] {
            table.store(key, entry(4, 0, Bound::Exact));
        }
        table.store(keys[4], entry(1, 0, Bound::Exact));
        assert_eq!(table.probe(keys[0]), None);
        assert!(keys[1..].iter().all(|&key| table.probe(key).is_some()));
    }

    #[test]
    fn a_torn_slot_reads_as_empty() {
        // Two threads storing into one slot at once can leave the key word of
        // one write next to the data word of the other.
        let table = TranspositionTable::new(1);
        let first = entry(3, 50, Bound::Exact).pack(0);
        let second = entry(7, -80, Bound::Upper).pack(0);
        let slot = &table.bucket(KEY).slots[0];
        slot.key_xor_data.store(KEY ^ first, Ordering::Relaxed);
        slot.data.store(second, Ordering::Relaxed);
        assert_eq!(table.probe(KEY), None);
    }

    #[test]
    fn a_shallower_result_does_not_replace_a_deeper_one_in_the_same_search() {
        let table = TranspositionTable::new(1);
        table.store(KEY, entry(8, 10, Bound::Exact));
        table.store(KEY, entry(3, 20, Bound::Exact));
        assert_eq!(table.probe(KEY).unwrap().depth, 8);
        table.store(KEY, entry(8, 30, Bound::Lower));
        assert_eq!(table.probe(KEY).unwrap().score, 30, "equal depth replaces");
    }

    #[test]
    fn anything_replaces_an_entry_from_an_earlier_search() {
        let table = TranspositionTable::new(1);
        table.store(KEY, entry(8, 10, Bound::Exact));
        table.new_search();
        table.store(KEY, entry(1, 20, Bound::Upper));
        assert_eq!(table.probe(KEY), Some(entry(1, 20, Bound::Upper)));
    }

    #[test]
    fn a_result_without_a_move_keeps_the_known_best_move() {
        let table = TranspositionTable::new(1);
        let with_move = TableEntry {
            best_move: Some(some_move()),
            ..entry(4, 10, Bound::Exact)
        };
        table.store(KEY, with_move);
        table.store(KEY, entry(5, -40, Bound::Upper));
        let probed = table.probe(KEY).unwrap();
        assert_eq!(probed.best_move, Some(some_move()));
        assert_eq!(probed.score, -40);
    }

    #[test]
    fn clear_forgets_everything() {
        let table = TranspositionTable::new(1);
        table.store(KEY, entry(4, 10, Bound::Exact));
        table.clear();
        assert_eq!(table.probe(KEY), None);
    }

    #[test]
    fn the_size_follows_the_megabytes() {
        assert_eq!(size_of::<Slot>(), 16);
        assert_eq!(size_of::<Bucket>(), 64);
        assert_eq!(align_of::<Bucket>(), 64);
        assert_eq!(TranspositionTable::new(1).slot_count(), 65_536);
        assert_eq!(TranspositionTable::new(16).slot_count(), 1_048_576);
    }

    #[test]
    fn hashfull_counts_only_the_current_search() {
        let table = TranspositionTable::new(1);
        assert_eq!(table.hashfull(), 0);
        // Keys k * 2^64 / buckets (plus a few) land in bucket k: fill the
        // buckets of the whole sample, four keys each.
        let step = u64::MAX / table.buckets.len() as u64 + 1;
        let sampled_buckets = (HASHFULL_SAMPLE / SLOTS_PER_BUCKET) as u64;
        for index in 0..sampled_buckets {
            for low_bits in 0..SLOTS_PER_BUCKET as u64 {
                table.store(index * step + low_bits, entry(1, 0, Bound::Exact));
            }
        }
        assert_eq!(table.hashfull(), 1000);
        table.new_search();
        assert_eq!(table.hashfull(), 0);
    }

    #[test]
    fn every_legal_move_packs_differently() {
        for fen in [
            // Castling both ways, en passant, quiet moves and captures.
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            // Promotions to all four pieces, with and without capture.
            "r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1",
        ] {
            let position: Chess = fen
                .parse::<Fen>()
                .unwrap()
                .into_position(CastlingMode::Standard)
                .unwrap();
            let moves = position.legal_moves();
            let mut packed: Vec<u16> = moves
                .iter()
                .map(|&chess_move| PackedMove::new(chess_move).bits())
                .collect();
            packed.sort_unstable();
            packed.dedup();
            assert_eq!(packed.len(), moves.len(), "{fen}");
        }
    }

    #[test]
    fn mate_scores_are_stored_relative_to_the_position() {
        // Mate in 5 plies from the root, found at ply 3: 2 plies from here.
        let stored = score_to_table(MATE - 5, 3);
        assert_eq!(stored, MATE - 2);
        // Met again at ply 7, it is a mate 7 + 2 plies from the root.
        assert_eq!(score_from_table(stored, 7), MATE - 9);
        assert_eq!(score_from_table(score_to_table(-MATE + 6, 4), 4), -MATE + 6);
        // Ordinary scores are not touched.
        assert_eq!(score_to_table(250, 10), 250);
        assert_eq!(score_from_table(-250, 10), -250);
    }
}
