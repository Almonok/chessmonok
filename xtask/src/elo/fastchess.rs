//! The fastchess command line for a match.
//!
//! [fastchess](https://github.com/Disservin/fastchess) is the tournament
//! manager: it starts both engines, feeds them positions over UCI, enforces
//! the clocks, adjudicates, and keeps the SPRT running. This module decides
//! its arguments ([`command_line`], a pure function, so the exact command is
//! unit-tested and printed by `--dry-run`) and runs it ([`run_fastchess`]).

use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

use crate::elo::engine_options::EngineOption;
use crate::elo::profile::MatchSettings;
use crate::elo::versions::Side;

/// Everything fastchess needs for one match.
#[derive(Debug)]
pub struct FastchessMatch<'a> {
    pub fastchess_binary: &'a Path,
    pub new: &'a Side,
    pub base: &'a Side,
    pub new_engine_options: &'a [EngineOption],
    pub base_engine_options: &'a [EngineOption],
    pub settings: &'a MatchSettings,
    /// `None` starts every game from the initial position.
    pub opening_book: Option<&'a Path>,
    /// Seed for the order of the openings, so a run can be replayed.
    pub seed: u64,
    /// Where fastchess writes every game, in PGN.
    pub games_file: &'a Path,
}

/// The fastchess arguments for a match (without the program name).
///
/// `new` is the first engine, so every number fastchess reports (Elo, wins,
/// log-likelihood ratio) is from the new version's point of view.
pub fn command_line(game: &FastchessMatch) -> Vec<String> {
    let mut arguments = Vec::new();
    arguments.extend(engine_arguments(game.new, game.new_engine_options));
    arguments.extend(engine_arguments(game.base, game.base_engine_options));
    arguments.extend(time_control_arguments(game.settings));
    arguments.extend(game_pair_arguments(game.settings));
    arguments.extend(sprt_arguments(game.settings));
    arguments.extend(adjudication_arguments());
    arguments.extend(output_arguments(game.games_file));
    if let Some(book) = game.opening_book {
        arguments.extend(opening_arguments(book, game.seed));
    }
    arguments
}

/// What a fastchess run left behind.
#[derive(Debug)]
pub struct FastchessRun {
    pub output: String,
    pub exited_successfully: bool,
}

/// Run fastchess from the repository root, showing its progress and keeping
/// its whole output, also written to `log_file`.
pub fn run_fastchess(
    game: &FastchessMatch,
    repository_root: &Path,
    log_file: &Path,
) -> Result<FastchessRun> {
    fs::create_dir_all(log_file.parent().expect("log path has a parent"))?;
    let mut log =
        File::create(log_file).with_context(|| format!("cannot create {}", log_file.display()))?;
    let mut process = Command::new(game.fastchess_binary)
        .args(command_line(game))
        .current_dir(repository_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("cannot start fastchess")?;
    let mut output = String::new();
    let standard_output = process.stdout.take().expect("stdout is piped");
    for line in BufReader::new(standard_output).lines() {
        let line = line.context("cannot read fastchess output")?;
        writeln!(log, "{line}")?;
        if is_progress_line(&line) {
            println!("{line}");
        }
        output.push_str(&line);
        output.push('\n');
    }
    let status = process.wait().context("cannot wait for fastchess")?;
    println!("full log: {}", log_file.display());
    Ok(FastchessRun {
        output,
        exited_successfully: status.success(),
    })
}

/// Lines worth showing while the match runs. The rest (principal
/// variations, warnings) only goes to the log file.
fn is_progress_line(line: &str) -> bool {
    const NOISE: [&str; 5] = ["Started game", "Warning;", "Info;", "Position;", "Moves;"];
    !NOISE.iter().any(|prefix| line.starts_with(prefix))
}

fn engine_arguments(side: &Side, options: &[EngineOption]) -> Vec<String> {
    let mut arguments = vec![
        "-engine".to_string(),
        format!("cmd={}", side.engine_binary.display()),
        format!("name={}", side.name),
    ];
    arguments.extend(
        options
            .iter()
            .map(|option| format!("option.{}={}", option.name, option.value)),
    );
    arguments
}

fn time_control_arguments(settings: &MatchSettings) -> Vec<String> {
    vec![
        "-each".into(),
        "proto=uci".into(),
        format!("tc={}", settings.time_control),
        // Milliseconds an engine may overrun its clock before losing on
        // time: absorbs operating-system hiccups at fast time controls.
        "timemargin=50".into(),
    ]
}

/// Each round is a game pair: the same opening twice, colours swapped, so
/// neither engine profits from a lopsided opening.
fn game_pair_arguments(settings: &MatchSettings) -> Vec<String> {
    vec![
        "-rounds".into(),
        (settings.max_games / 2).to_string(),
        "-games".into(),
        "2".into(),
        "-repeat".into(),
        "-concurrency".into(),
        settings.concurrency.to_string(),
    ]
}

/// `logistic` puts the bounds in ordinary Elo; alpha and beta are the two
/// error rates (5 % each).
fn sprt_arguments(settings: &MatchSettings) -> Vec<String> {
    vec![
        "-sprt".into(),
        format!("elo0={}", settings.lower_elo),
        format!("elo1={}", settings.upper_elo),
        "alpha=0.05".into(),
        "beta=0.05".into(),
        "model=logistic".into(),
    ]
}

/// End a game early when both engines agree: one side 10 pawns (1000
/// centipawns) up for 3 moves, or a score within 0.1 pawn for 8 moves after
/// move 40.
fn adjudication_arguments() -> Vec<String> {
    [
        "-resign",
        "movecount=3",
        "score=1000",
        "twosided=true",
        "-draw",
        "movenumber=40",
        "movecount=8",
        "score=10",
    ]
    .map(String::from)
    .to_vec()
}

fn output_arguments(games_file: &Path) -> Vec<String> {
    vec!["-pgnout".into(), format!("file={}", games_file.display())]
}

fn opening_arguments(book: &Path, seed: u64) -> Vec<String> {
    vec![
        "-openings".into(),
        format!("file={}", book.display()),
        "format=pgn".into(),
        "order=random".into(),
        "-srand".into(),
        seed.to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elo::versions::CodeVersion;
    use std::path::PathBuf;

    fn side(name: &str) -> Side {
        Side {
            name: name.into(),
            code_version: CodeVersion::Commit { hash: "abc".into() },
            engine_binary: PathBuf::from(format!("/bin/{name}")),
        }
    }

    fn quick_settings() -> MatchSettings {
        MatchSettings {
            time_control: "5+0.05".into(),
            concurrency: 8,
            max_games: 1000,
            lower_elo: 0.0,
            upper_elo: 10.0,
        }
    }

    fn command_line_text(opening_book: Option<&Path>) -> String {
        let (new, base, settings) = (side("new"), side("base"), quick_settings());
        let hash_option = [EngineOption {
            name: "Hash".into(),
            value: "16".into(),
        }];
        let game = FastchessMatch {
            fastchess_binary: Path::new("tools/fastchess"),
            new: &new,
            base: &base,
            new_engine_options: &hash_option,
            base_engine_options: &[],
            settings: &settings,
            opening_book,
            seed: 42,
            games_file: Path::new("matches/x.pgn"),
        };
        command_line(&game).join(" ")
    }

    #[test]
    fn carries_engines_sprt_book_and_game_pairs() {
        let text = command_line_text(Some(Path::new("books/8moves_v3.pgn")));
        for expected in [
            "-engine cmd=/bin/new name=new option.Hash=16 -engine cmd=/bin/base name=base -each",
            "tc=5+0.05",
            "-rounds 500 -games 2 -repeat",
            "-concurrency 8",
            "-sprt elo0=0 elo1=10 alpha=0.05 beta=0.05 model=logistic",
            "-openings file=books/8moves_v3.pgn format=pgn order=random",
            "-srand 42",
            "-pgnout file=matches/x.pgn",
        ] {
            assert!(text.contains(expected), "missing `{expected}` in {text}");
        }
    }

    #[test]
    fn without_a_book_there_are_no_opening_arguments() {
        let text = command_line_text(None);
        assert!(
            !text.contains("-openings") && !text.contains("-srand"),
            "{text}"
        );
    }
}
