//! Reading the numbers of a match out of fastchess's output.
//!
//! At the end of a match (and every few games during it) fastchess prints a
//! results block, from the first engine's (the new version's) point of view:
//!
//! ```text
//! Results of new vs base (5+0.05, ...):
//! Elo: 12.34 +/- 5.67, nElo: 18.90 +/- 8.70
//! LOS: 99.99 %, DrawRatio: 40.00 %, PairsRatio: 1.50
//! Games: 1000, Wins: 300, Losses: 265, Draws: 435, Points: 517.5 (51.75 %)
//! Ptnml(0-2): [20, 110, 230, 120, 20], WL/DD Ratio: 1.10
//! LLR: 2.95 (100.3%) (-2.94, 2.94) [0.00, 10.00]
//! ```
//!
//! `nElo` is the normalised Elo, `Ptnml` the pentanomial counts of game
//! pairs by points scored (0 to 2), `LLR` the SPRT's log-likelihood ratio
//! with the two bounds that stop it. `docs/testing-elo.md` explains each.

use std::time::Duration;

use anyhow::{Result, bail};

/// The numbers of one match, from the new version's point of view.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MatchReport {
    /// `None` when fastchess prints `inf`/`nan` (one side won every game).
    pub elo: Option<f64>,
    /// Half-width of the 95 % confidence interval around `elo`.
    pub elo_error_margin: Option<f64>,
    pub normalized_elo: Option<f64>,
    /// `None` when the match ran without an SPRT.
    pub log_likelihood_ratio: Option<f64>,
    /// The values of the log-likelihood ratio that stop the SPRT.
    pub log_likelihood_ratio_bounds: Option<(f64, f64)>,
    pub games: u32,
    pub wins: u32,
    pub losses: u32,
    pub draws: u32,
    /// Game pairs that scored 0, ½, 1, 1½ and 2 points.
    pub pair_score_counts: [u32; 5],
    pub red_flags: RedFlags,
    /// Wall time of the match, measured by the harness.
    pub duration: Option<Duration>,
}

/// Game endings that mean a bug, not weaker play.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RedFlags {
    pub time_losses: u32,
    pub illegal_moves: u32,
    pub disconnects: u32,
    pub crashes: u32,
}

impl RedFlags {
    pub fn any(&self) -> bool {
        *self != RedFlags::default()
    }
}

/// Whether fastchess stopped before the match was over: it exited with an
/// error, or it says so. Covers a Ctrl-C sent to fastchess alone, a kill or
/// a crash, none of which the harness's own Ctrl-C handler sees.
pub fn was_interrupted(output: &str, exited_successfully: bool) -> bool {
    !exited_successfully || output.contains("Tournament was interrupted")
}

/// The report for a run. For a finished match, fastchess's last results
/// block; for an interrupted one, our own count of the finished games,
/// because the last block fastchess printed may be many games old.
pub fn report_for_run(output: &str, interrupted: bool) -> Result<MatchReport> {
    if interrupted {
        Ok(count_finished_games(output))
    } else {
        parse_results(output)
    }
}

/// Parse fastchess's last results block, plus the ending of every game.
pub fn parse_results(output: &str) -> Result<MatchReport> {
    let mut report = MatchReport::default();
    let mut saw_games_line = false;
    for line in last_results_block(output)? {
        if let Some(rest) = line.strip_prefix("Elo:") {
            read_elo_line(rest, &mut report);
        } else if line.starts_with("Games:") {
            read_games_line(line, &mut report);
            saw_games_line = true;
        } else if let Some(rest) = line.strip_prefix("Ptnml(0-2): [") {
            read_pair_scores_line(rest, &mut report);
        } else if let Some(rest) = line.strip_prefix("LLR:") {
            read_log_likelihood_line(rest, &mut report);
        }
    }
    if !saw_games_line {
        bail!("the fastchess results block has no `Games:` line");
    }
    report.red_flags = red_flags_in(output);
    Ok(report)
}

/// The lines of the last `Results of ...` block, up to its closing `---`.
fn last_results_block(output: &str) -> Result<Vec<&str>> {
    let lines: Vec<&str> = output.lines().map(str::trim).collect();
    let Some(start) = lines
        .iter()
        .rposition(|line| line.starts_with("Results of"))
    else {
        bail!("no results block in the fastchess output");
    };
    Ok(lines[start + 1..]
        .iter()
        .take_while(|line| !line.starts_with("---"))
        .copied()
        .collect())
}

/// `12.34 +/- 5.67, nElo: 18.90 +/- 8.70`
fn read_elo_line(text: &str, report: &mut MatchReport) {
    let (elo_part, normalized_part) = text.split_once(", nElo:").unwrap_or((text, ""));
    let (elo, margin) = elo_part.split_once("+/-").unwrap_or((elo_part, ""));
    report.elo = finite_number(elo);
    report.elo_error_margin = finite_number(margin);
    report.normalized_elo = normalized_part
        .split_once("+/-")
        .and_then(|(normalized, _)| finite_number(normalized));
}

/// `Games: 40, Wins: 9, Losses: 26, Draws: 5, Points: ...`
fn read_games_line(line: &str, report: &mut MatchReport) {
    for field in line.split(", ") {
        let Some((key, value)) = field.split_once(": ") else {
            continue;
        };
        let count = value.trim().parse().unwrap_or(0);
        match key {
            "Games" => report.games = count,
            "Wins" => report.wins = count,
            "Losses" => report.losses = count,
            "Draws" => report.draws = count,
            _ => {}
        }
    }
}

/// `9, 3, 5, 2, 1], WL/DD Ratio: inf`
fn read_pair_scores_line(text: &str, report: &mut MatchReport) {
    let inside_brackets = text.split(']').next().unwrap_or("");
    for (count, number) in report
        .pair_score_counts
        .iter_mut()
        .zip(inside_brackets.split(','))
    {
        *count = number.trim().parse().unwrap_or(0);
    }
}

/// `0.40 (13.5%) (-2.94, 2.94) [0.00, 10.00]`
fn read_log_likelihood_line(text: &str, report: &mut MatchReport) {
    report.log_likelihood_ratio = text.split_whitespace().next().and_then(finite_number);
    report.log_likelihood_ratio_bounds = text.split('(').nth(2).and_then(|bounds| {
        let (lower, upper) = bounds.split(')').next()?.split_once(',')?;
        Some((finite_number(lower)?, finite_number(upper)?))
    });
}

/// Count the finished games ourselves, from the new version's point of view.
///
/// For an interrupted run: fastchess then stops without a final results
/// block, but every `Finished game N (white vs black): result` line is
/// there. Only wins, draws, losses and red flags can be recovered; Elo and
/// the log-likelihood ratio stay `None`.
pub fn count_finished_games(output: &str) -> MatchReport {
    let mut report = MatchReport::default();
    for line in finished_game_lines(output) {
        // "Finished game N (new vs base): 1-0 {reason}"
        let Some((players, result)) = line.split_once("): ") else {
            continue;
        };
        let new_has_white = players.contains("(new vs");
        match (result.split_whitespace().next(), new_has_white) {
            (Some("1-0"), true) | (Some("0-1"), false) => report.wins += 1,
            (Some("0-1"), true) | (Some("1-0"), false) => report.losses += 1,
            (Some("1/2-1/2"), _) => report.draws += 1,
            _ => continue,
        }
        report.games += 1;
    }
    report.red_flags = red_flags_in(output);
    report
}

fn finished_game_lines(output: &str) -> impl Iterator<Item = &str> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("Finished game"))
}

/// Count the game endings that mean a bug, from the `{reason}` fastchess
/// gives at the end of each `Finished game` line.
fn red_flags_in(output: &str) -> RedFlags {
    let mut flags = RedFlags::default();
    for line in finished_game_lines(output) {
        let reason = line
            .rsplit_once('{')
            .map_or("", |(_, reason)| reason)
            .to_ascii_lowercase();
        if reason.contains("on time") {
            flags.time_losses += 1;
        } else if reason.contains("illegal") {
            flags.illegal_moves += 1;
        } else if reason.contains("disconnect") || reason.contains("stall") {
            flags.disconnects += 1;
        } else if reason.contains("crash") {
            flags.crashes += 1;
        }
    }
    flags
}

/// A finite number, or `None` for `inf`, `nan` or anything unreadable.
fn finite_number(text: &str) -> Option<f64> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL_SPRT: &str = include_str!("../../tests/fixtures/sprt_small.txt");
    const NO_SPRT: &str = include_str!("../../tests/fixtures/no_sprt.txt");
    const TIME_LOSSES: &str = include_str!("../../tests/fixtures/time_loss.txt");

    #[test]
    fn reads_a_small_sprt_run() {
        let report = parse_results(SMALL_SPRT).unwrap();
        assert_eq!((report.games, report.wins), (20, 20));
        assert_eq!(report.pair_score_counts, [0, 0, 0, 0, 10]);
        assert_eq!(report.elo, None, "the fixture says inf");
        assert_eq!(report.log_likelihood_ratio, Some(0.40));
        assert_eq!(report.log_likelihood_ratio_bounds, Some((-2.94, 2.94)));
    }

    #[test]
    fn reads_elo_and_its_margin() {
        let report = parse_results(NO_SPRT).unwrap();
        assert_eq!(report.elo, Some(-157.66));
        assert_eq!(report.elo_error_margin, Some(122.25));
        assert_eq!(report.normalized_elo, Some(-168.96));
        assert_eq!(report.log_likelihood_ratio, None);
        assert_eq!((report.wins, report.losses, report.draws), (9, 26, 5));
    }

    #[test]
    fn the_last_results_block_wins() {
        let interim = "Results of new vs old (x):\nElo: 1.00 +/- 2.00, nElo: 1.00 +/- 2.00\nGames: 2, Wins: 1, Losses: 1, Draws: 0, Points: 1.0 (50.00 %)\n--------\n";
        let output = format!("{interim}{NO_SPRT}");
        assert_eq!(parse_results(&output).unwrap().games, 40);
    }

    #[test]
    fn output_without_results_is_an_error() {
        assert!(parse_results("Started game 1 of 2 (new vs base)").is_err());
    }

    #[test]
    fn counts_red_flags() {
        let report = parse_results(TIME_LOSSES).unwrap();
        assert_eq!(report.red_flags.time_losses, 2);
        assert_eq!(report.red_flags.disconnects, 1);
    }

    #[test]
    fn counts_finished_games_for_the_new_version() {
        let output = "\
Finished game 1 (new vs base): 1-0 {White mates}
Finished game 2 (base vs new): 1-0 {White wins by adjudication}
Finished game 3 (base vs new): 0-1 {Black mates}
Finished game 4 (new vs base): 1/2-1/2 {Draw by 3-fold repetition}
Finished game 5 (new vs base): 0-1 {White loses on time}
";
        let report = count_finished_games(output);
        assert_eq!(report.games, 5);
        assert_eq!((report.wins, report.draws, report.losses), (2, 1, 2));
        assert_eq!(report.red_flags.time_losses, 1);
    }

    #[test]
    fn detects_an_interrupted_fastchess() {
        assert!(was_interrupted(
            "Tournament was interrupted. To resume...\nFinished match",
            true
        ));
        assert!(was_interrupted("Finished match", false));
        assert!(!was_interrupted("Finished match", true));
    }

    #[test]
    fn interrupted_runs_count_the_games_themselves() {
        let interim = "Results of new vs base (x):\nGames: 2, Wins: 1, Losses: 1, Draws: 0, Points: 1.0 (50.00 %)\n---\n";
        let games = "Finished game 1 (new vs base): 1-0 {White mates}\n".repeat(6);
        let output = format!("{interim}{games}");
        assert_eq!(report_for_run(&output, true).unwrap().games, 6);
        assert_eq!(report_for_run(&output, false).unwrap().games, 2);
    }
}
