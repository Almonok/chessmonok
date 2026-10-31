//! Everything one run needs, resolved from the command line, and the steps
//! that use it.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use clap::ValueEnum;

use crate::elo::arguments::EloArguments;
use crate::elo::build::build_engine;
use crate::elo::engine_check::check_engine;
use crate::elo::engine_options::{EngineOption, parse_engine_option};
use crate::elo::fastchess::{FastchessMatch, command_line, run_fastchess};
use crate::elo::history::{
    HistoryEntry, append_entry, describe_engine_option, machine_description, should_record,
};
use crate::elo::interruption::Interruption;
use crate::elo::match_report::{MatchReport, report_for_run, was_interrupted};
use crate::elo::profile::{MatchSettings, match_settings};
use crate::elo::utc_time::current_utc_timestamp;
use crate::elo::verdict::{Verdict, decide_verdict};
use crate::elo::versions::{
    CodeVersion, Side, commit_hash_of, repository_root, versions_to_build, working_tree_version,
};

/// The opening positions `scripts/install-tools.sh` downloads.
const DEFAULT_OPENING_BOOK: &str = "books/8moves_v3.pgn";
/// Where recorded runs go, relative to the repository root.
const HISTORY_FILE: &str = "elo/history.jsonl";

/// Everything one run needs.
#[derive(Debug)]
pub struct MatchPlan<'a> {
    arguments: &'a EloArguments,
    repository_root: PathBuf,
    fastchess_binary: PathBuf,
    opening_book: Option<PathBuf>,
    pub settings: MatchSettings,
    new: Side,
    base: Side,
    new_engine_options: Vec<EngineOption>,
    base_engine_options: Vec<EngineOption>,
    seed: u64,
    games_file: PathBuf,
    log_file: PathBuf,
}

impl<'a> MatchPlan<'a> {
    pub fn from_arguments(arguments: &'a EloArguments) -> Result<Self> {
        let repository_root = repository_root()?;
        let opening_book = opening_book_path(arguments, &repository_root)?;
        let fastchess_binary = locate_fastchess(&repository_root, opening_book.as_deref())?;
        let (new_engine_options, base_engine_options) = engine_options_per_side(arguments)?;
        let base_version = CodeVersion::Commit {
            hash: commit_hash_of(&arguments.base_version)?,
        };
        // `:` is not allowed in file names everywhere.
        let run_name = current_utc_timestamp().replace(':', "-");
        Ok(MatchPlan {
            arguments,
            settings: match_settings(arguments.profile, arguments.mode, arguments),
            new: side_not_built_yet("new", new_code_version(arguments)?),
            base: side_not_built_yet("base", base_version),
            new_engine_options,
            base_engine_options,
            seed: arguments.seed.unwrap_or_else(seed_from_clock),
            games_file: repository_root
                .join("matches")
                .join(format!("{run_name}.pgn")),
            log_file: repository_root
                .join("matches")
                .join(format!("{run_name}.log")),
            repository_root,
            fastchess_binary,
            opening_book,
        })
    }

    /// Print what is about to be compared, and how.
    pub fn print(&self) {
        let new_name = self
            .arguments
            .new_version
            .as_deref()
            .unwrap_or("working tree");
        let settings = &self.settings;
        println!("new   {} ({new_name})", self.new.code_version.label());
        println!(
            "base  {} ({})",
            self.base.code_version.label(),
            self.arguments.base_version
        );
        println!(
            "{} profile, {} mode: tc {}, concurrency {}, cap {} games, \
             SPRT hypotheses [{}, {}] Elo, seed {}",
            value_name(&self.arguments.profile),
            value_name(&self.arguments.mode),
            settings.time_control,
            settings.concurrency,
            settings.max_games,
            settings.lower_elo,
            settings.upper_elo,
            self.seed
        );
        if self.opening_book.is_none() {
            eprintln!(
                "warning: no opening book. Deterministic engines will replay the same few \
                 games, so the statistics below are not trustworthy."
            );
        }
    }

    pub fn print_fastchess_command(&self) {
        println!(
            "{} {}",
            self.fastchess_binary.display(),
            command_line(&self.fastchess_match()).join(" ")
        );
    }

    /// Build both engines, then check each speaks UCI and takes its options.
    pub fn build_and_check_engines(&mut self) -> Result<()> {
        self.build_engines()?;
        check_engine(&self.new.engine_binary, &self.new_engine_options, "new")?;
        check_engine(&self.base.engine_binary, &self.base_engine_options, "base")
    }

    fn build_engines(&mut self) -> Result<()> {
        let versions: Vec<CodeVersion> =
            versions_to_build(&self.new.code_version, &self.base.code_version)
                .into_iter()
                .cloned()
                .collect();
        for version in versions {
            println!("building {} ...", version.label());
            let binary = build_engine(&version, &self.repository_root)?;
            for side in [&mut self.new, &mut self.base] {
                if side.code_version == version {
                    side.engine_binary.clone_from(&binary);
                }
            }
        }
        Ok(())
    }

    /// Play the match and judge it.
    pub fn play(&self, interruption: &Interruption) -> Result<(MatchReport, Verdict)> {
        let started = Instant::now();
        let run = run_fastchess(
            &self.fastchess_match(),
            &self.repository_root,
            &self.log_file,
        )?;
        let interrupted =
            interruption.happened() || was_interrupted(&run.output, run.exited_successfully);
        let mut report = report_for_run(&run.output, interrupted)
            .with_context(|| format!("full log: {}", self.log_file.display()))?;
        report.duration = Some(started.elapsed());
        let verdict = decide_verdict(&report, interrupted);
        Ok((report, verdict))
    }

    /// Append the run to the history file if it is evidence worth keeping.
    pub fn record_if_meaningful(&self, report: &MatchReport, verdict: &Verdict) -> Result<()> {
        if should_record(
            verdict,
            &self.new.code_version,
            &self.base.code_version,
            self.arguments.no_record,
        ) {
            append_entry(
                &self.repository_root.join(HISTORY_FILE),
                &self.history_entry(report, verdict),
            )?;
            println!("recorded in {HISTORY_FILE}: commit it with the change it measures");
        } else {
            println!("not recorded (uncommitted changes, aborted, invalid or --no-record)");
        }
        Ok(())
    }

    fn fastchess_match(&self) -> FastchessMatch<'_> {
        FastchessMatch {
            fastchess_binary: &self.fastchess_binary,
            new: &self.new,
            base: &self.base,
            new_engine_options: &self.new_engine_options,
            base_engine_options: &self.base_engine_options,
            settings: &self.settings,
            opening_book: self.opening_book.as_deref(),
            seed: self.seed,
            games_file: &self.games_file,
        }
    }

    fn history_entry(&self, report: &MatchReport, verdict: &Verdict) -> HistoryEntry {
        HistoryEntry {
            date: current_utc_timestamp(),
            new_commit: self.new.code_version.label(),
            base_commit: self.base.code_version.label(),
            mode: value_name(&self.arguments.mode),
            profile: value_name(&self.arguments.profile),
            time_control: self.settings.time_control.clone(),
            lower_elo: self.settings.lower_elo,
            upper_elo: self.settings.upper_elo,
            verdict: verdict.to_string(),
            elo: report.elo,
            elo_error_margin: report.elo_error_margin,
            log_likelihood_ratio: report.log_likelihood_ratio,
            games: report.games,
            wins: report.wins,
            losses: report.losses,
            draws: report.draws,
            pair_score_counts: report.pair_score_counts,
            engine_options: self.recorded_engine_options(),
            opening_book: self
                .opening_book
                .as_deref()
                .and_then(Path::file_name)
                .map(|name| name.to_string_lossy().into_owned()),
            seed: self.seed,
            machine: machine_description(),
        }
    }

    /// Every option, prefixed with the side it was given to.
    fn recorded_engine_options(&self) -> Vec<String> {
        let new = self.new_engine_options.iter().map(|option| ("new", option));
        let base = self
            .base_engine_options
            .iter()
            .map(|option| ("base", option));
        new.chain(base)
            .map(|(side, option)| format!("{side}:{}", describe_engine_option(option)))
            .collect()
    }
}

/// The version under test: a commit if one was named, else the working tree.
fn new_code_version(arguments: &EloArguments) -> Result<CodeVersion> {
    match &arguments.new_version {
        Some(git_reference) => Ok(CodeVersion::Commit {
            hash: commit_hash_of(git_reference)?,
        }),
        None => working_tree_version(),
    }
}

fn side_not_built_yet(name: &str, code_version: CodeVersion) -> Side {
    Side {
        name: name.to_string(),
        engine_binary: PathBuf::from(format!("<{name} build of {}>", code_version.label())),
        code_version,
    }
}

/// The opening book as an absolute path, or `None` with `--no-opening-book`.
///
/// Absolute because `cargo xtask` may run from any subdirectory while
/// fastchess always runs from the repository root.
fn opening_book_path(arguments: &EloArguments, repository_root: &Path) -> Result<Option<PathBuf>> {
    if arguments.no_opening_book {
        return Ok(None);
    }
    match &arguments.opening_book {
        Some(book) => Ok(Some(std::path::absolute(book)?)),
        None => Ok(Some(repository_root.join(DEFAULT_OPENING_BOOK))),
    }
}

/// The fastchess binary, after checking that it and the opening book exist.
fn locate_fastchess(repository_root: &Path, opening_book: Option<&Path>) -> Result<PathBuf> {
    let fastchess = repository_root.join("tools/fastchess");
    if !fastchess.exists() {
        bail!(
            "{} is missing: run scripts/install-tools.sh",
            fastchess.display()
        );
    }
    if let Some(book) = opening_book {
        if !book.exists() {
            bail!(
                "opening book {} is missing: run scripts/install-tools.sh",
                book.display()
            );
        }
    }
    Ok(fastchess)
}

/// `--engine-option` goes to both engines, `--new-engine-option` and
/// `--base-engine-option` to one.
fn engine_options_per_side(
    arguments: &EloArguments,
) -> Result<(Vec<EngineOption>, Vec<EngineOption>)> {
    let parse_all = |texts: &[String]| -> Result<Vec<EngineOption>> {
        texts.iter().map(|text| parse_engine_option(text)).collect()
    };
    let for_both = parse_all(&arguments.engine_options)?;
    let mut for_new = for_both.clone();
    for_new.extend(parse_all(&arguments.new_engine_options)?);
    let mut for_base = for_both;
    for_base.extend(parse_all(&arguments.base_engine_options)?);
    Ok((for_new, for_base))
}

/// A seed from the clock: different on every run, short enough to retype,
/// and printed so a run can be replayed with `--seed`.
fn seed_from_clock() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| u64::from(elapsed.subsec_nanos()))
}

/// The command-line spelling of a mode or profile, e.g. `no-regression`.
fn value_name(value: &impl ValueEnum) -> String {
    value
        .to_possible_value()
        .map_or_else(String::new, |possible| possible.get_name().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elo::profile::Mode;
    use std::fs;

    fn empty_repository_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("xtask-root-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn with_fastchess(root: &Path) {
        fs::create_dir_all(root.join("tools")).unwrap();
        fs::write(root.join("tools/fastchess"), "").unwrap();
    }

    #[test]
    fn missing_fastchess_points_to_the_install_script() {
        let root = empty_repository_root("no-tools");
        let error = format!("{:#}", locate_fastchess(&root, None).unwrap_err());
        assert!(error.contains("scripts/install-tools.sh"), "{error}");
    }

    #[test]
    fn a_missing_opening_book_points_to_the_install_script() {
        let root = empty_repository_root("no-book");
        with_fastchess(&root);
        let book = root.join("books/8moves_v3.pgn");
        let error = format!("{:#}", locate_fastchess(&root, Some(&book)).unwrap_err());
        assert!(error.contains("scripts/install-tools.sh"), "{error}");
        assert!(error.contains("8moves_v3.pgn"), "{error}");
    }

    #[test]
    fn fastchess_is_found_when_installed() {
        let root = empty_repository_root("tools");
        with_fastchess(&root);
        assert_eq!(
            locate_fastchess(&root, None).unwrap(),
            root.join("tools/fastchess")
        );
    }

    #[test]
    fn opening_book_paths_are_absolute() {
        let root = Path::new("/repository");
        let given = EloArguments {
            opening_book: Some("Cargo.toml".into()),
            ..EloArguments::default()
        };
        let book = opening_book_path(&given, root).unwrap().unwrap();
        assert!(book.is_absolute(), "{}", book.display());
        assert!(book.ends_with("xtask/Cargo.toml"), "{}", book.display());
        let default = opening_book_path(&EloArguments::default(), root).unwrap();
        assert_eq!(default, Some(root.join("books/8moves_v3.pgn")));
        let without = EloArguments {
            no_opening_book: true,
            ..EloArguments::default()
        };
        assert_eq!(opening_book_path(&without, root).unwrap(), None);
    }

    #[test]
    fn engine_options_reach_the_right_sides() {
        let arguments = EloArguments {
            engine_options: vec!["Hash=16".into()],
            new_engine_options: vec!["EvalFile=Cargo.toml".into()],
            ..EloArguments::default()
        };
        let (for_new, for_base) = engine_options_per_side(&arguments).unwrap();
        let names = |options: &[EngineOption]| -> Vec<String> {
            options.iter().map(|option| option.name.clone()).collect()
        };
        assert_eq!(names(&for_new), ["Hash", "EvalFile"]);
        assert_eq!(names(&for_base), ["Hash"]);
    }

    #[test]
    fn modes_are_spelled_as_on_the_command_line() {
        assert_eq!(value_name(&Mode::NoRegression), "no-regression");
    }
}
