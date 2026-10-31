//! The command line of `cargo xtask elo`.

use std::path::PathBuf;

use clap::Args;

use crate::elo::profile::{Mode, Profile};

/// Command-line arguments of `cargo xtask elo`.
#[derive(Clone, Debug, Args)]
#[allow(clippy::struct_excessive_bools)] // independent switches, not a state machine
pub struct EloArguments {
    /// Version under test: any git reference (branch, tag, commit hash,
    /// `HEAD~1`...). Default: the working tree, uncommitted changes
    /// included.
    #[arg(long = "new")]
    pub new_version: Option<String>,
    /// Version to compare against: any git reference.
    #[arg(long = "base", default_value = "main")]
    pub base_version: String,
    /// `gain`: prove the change adds strength. `no-regression`: prove it
    /// loses none.
    #[arg(long, value_enum, default_value_t = Mode::Gain)]
    pub mode: Mode,
    /// How long and how precise the test is.
    #[arg(long, value_enum, default_value_t = Profile::Quick)]
    pub profile: Profile,
    /// Time per game, e.g. `5+0.05` (seconds per side + increment per move).
    #[arg(long)]
    pub time_control: Option<String>,
    /// Games played at the same time.
    #[arg(long)]
    pub concurrency: Option<u32>,
    /// Stop with INCONCLUSIVE after this many games.
    #[arg(long)]
    pub max_games: Option<u32>,
    /// The SPRT's lower hypothesis, in Elo.
    #[arg(long, allow_negative_numbers = true)]
    pub lower_elo: Option<f64>,
    /// The SPRT's upper hypothesis, in Elo.
    #[arg(long, allow_negative_numbers = true)]
    pub upper_elo: Option<f64>,
    /// UCI option for both engines, `NAME=VALUE`. Repeatable.
    #[arg(long = "engine-option", value_name = "NAME=VALUE")]
    pub engine_options: Vec<String>,
    /// UCI option for the new engine only. Repeatable.
    #[arg(long = "new-engine-option", value_name = "NAME=VALUE")]
    pub new_engine_options: Vec<String>,
    /// UCI option for the base engine only. Repeatable.
    #[arg(long = "base-engine-option", value_name = "NAME=VALUE")]
    pub base_engine_options: Vec<String>,
    /// Opening positions (PGN). Default: `books/8moves_v3.pgn`.
    #[arg(long, conflicts_with = "no_opening_book")]
    pub opening_book: Option<PathBuf>,
    /// Start every game from the initial position. Results are then
    /// unreliable: deterministic engines replay the same games.
    #[arg(long)]
    pub no_opening_book: bool,
    /// Seed for the order of the openings. Default: taken from the clock.
    #[arg(long)]
    pub seed: Option<u64>,
    /// Print the builds and the fastchess command, then stop.
    #[arg(long)]
    pub dry_run: bool,
    /// Never append to `elo/history.jsonl` (CI runs).
    #[arg(long)]
    pub no_record: bool,
}

/// The same defaults as the attributes above, so tests can write
/// `EloArguments { max_games: Some(600), ..EloArguments::default() }`.
impl Default for EloArguments {
    fn default() -> Self {
        Self {
            new_version: None,
            base_version: "main".to_string(),
            mode: Mode::Gain,
            profile: Profile::Quick,
            time_control: None,
            concurrency: None,
            max_games: None,
            lower_elo: None,
            upper_elo: None,
            engine_options: Vec::new(),
            new_engine_options: Vec::new(),
            base_engine_options: Vec::new(),
            opening_book: None,
            no_opening_book: false,
            seed: None,
            dry_run: false,
            no_record: false,
        }
    }
}
