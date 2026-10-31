//! `cargo xtask elo`: is this version of chessmonok stronger than that one?
//!
//! A run, step by step (see [`run`]):
//!
//! 1. Plan the match: which two versions of the code, which profile
//!    ([`profile`]), which options ([`arguments`], [`engine_options`]).
//! 2. Build each version from git ([`versions`], [`build`]).
//! 3. Check each engine over UCI before any game ([`engine_check`]).
//! 4. Play the match with fastchess until the SPRT or the game cap stops it
//!    ([`fastchess`]).
//! 5. Read the numbers and judge them ([`match_report`], [`verdict`]).
//! 6. Record valid runs between exact commits ([`history`]).
//!
//! `docs/testing-elo.md` explains the statistics behind it.

mod arguments;
mod build;
mod engine_check;
mod engine_options;
mod fastchess;
mod history;
mod interruption;
mod match_plan;
mod match_report;
mod profile;
mod utc_time;
mod verdict;
mod versions;

use std::process::ExitCode;

use anyhow::Result;

pub use arguments::EloArguments;
use interruption::Interruption;
use match_plan::MatchPlan;
use verdict::{Verdict, exit_code, format_summary};

/// Exit code for an error outside any verdict (unknown git reference,
/// failed build, missing tools).
pub const EXIT_CODE_FOR_ERRORS: u8 = 5;

/// Run `cargo xtask elo`. The exit code is the verdict's.
pub fn run(arguments: &EloArguments) -> Result<ExitCode> {
    let mut plan = MatchPlan::from_arguments(arguments)?;
    plan.print();
    if arguments.dry_run {
        plan.print_fastchess_command();
        return Ok(ExitCode::SUCCESS);
    }

    let interruption = Interruption::watch()?;
    let engines_ready = plan.build_and_check_engines();
    if interruption.happened() {
        println!("ABORTED before the match started; nothing recorded");
        return Ok(exit_code_for(&Verdict::Aborted));
    }
    engines_ready?;

    let (report, verdict) = plan.play(&interruption)?;
    println!("{}", format_summary(&report, &verdict, &plan.settings));
    plan.record_if_meaningful(&report, &verdict)?;
    Ok(exit_code_for(&verdict))
}

fn exit_code_for(verdict: &Verdict) -> ExitCode {
    ExitCode::from(exit_code(verdict))
}
