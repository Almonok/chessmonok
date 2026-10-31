# chessmonok

A UCI chess engine in Rust, built for strength: alpha-beta search with an
NNUE evaluation trained by the sibling project almoNnuet.

## How code is written here

Strength and speed come first, but the code is also meant to be read and
learnt from. Every module follows the same standard:

- **Module docs explain the concept** from intuition up (what problem this
  solves in a chess engine, the core idea, a small diagram or formula when
  it helps), then point at the code.
- **Comments explain why**, at the point of use: why this reduction is
  safe, why this layout, why this constant. Not what the next line does.
- **Rust techniques are named where they are not obvious**: `unsafe` and
  its safety argument, SIMD intrinsics, const generics, `#[repr(align)]`,
  atomics, RAII guards, and so on, with why they beat the obvious
  alternative here.

Code is reviewed by a human, so it must read clearly at first sight:

- **Full, descriptive names; no abbreviations.** `engine_options`, not
  `opts`; `time_control`, not `tc`; `log_likelihood_ratio`, not `llr`;
  `sender`/`receiver`, not `tx`/`rx`; no one-letter variables outside
  tiny closures. The terms of the domain (Elo, SPRT, UCI, NNUE) are fine,
  explained once in the module docs.
- **Structure before comments.** When a function needs comments to be
  followed, split it into well-named functions first, so the top-level
  function reads as a sequence of steps. Comments are the last resort, for
  the *why* that names cannot carry.
- **One responsibility per module.** A module whose doc needs "and" to
  describe it is probably two modules.

## Evidence for every change

- **Strength** is measured with `cargo xtask elo` (see `xtask/src/elo/`):
  - `--mode gain` for a change meant to add strength: it merges on a quick
    PASS.
  - `--mode no-regression` for a refactor, simplification or speed-up: it merges
    on a quick result that is not FAIL.
  - INCONCLUSIVE is never a pass; the maintainer decides whether to drop
    the change, merge on judgement or run `--profile long`.
- **Always ask the maintainer before starting a match.** It occupies the
  machine for up to 20 minutes (quick) or many hours (long).
- Commit the line a recorded run appends to `elo/history.jsonl` together
  with the change it measures.
- Speed changes also report before/after numbers from the relevant
  benchmark.

## Checks

CI runs, and every change must pass:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

`scripts/install-tools.sh` fetches fastchess and the opening suite into the
gitignored `tools/` and `books/`.

## Commits

`type(scope): :gitmoji: subject`, then a prose body explaining why. For
example: `feat(elo): :sparkles: record clean results in elo/history.jsonl`.

## Local-only docs

`docs/` is gitignored here and is its own git repository with no remote:
the long-form teaching chapters live there. Never add a remote to it and
never copy its content into tracked files.
