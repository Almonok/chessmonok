//! What a match concluded, and the summary printed at the end of a run.

use std::fmt;
use std::time::Duration;

use crate::elo::match_report::{MatchReport, RedFlags};
use crate::elo::profile::MatchSettings;

/// What the match concluded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The SPRT concluded the change is worth `upper_elo`.
    Pass,
    /// The SPRT concluded the change is worth `lower_elo`.
    Fail,
    /// The game cap came first. Never a pass.
    Inconclusive,
    /// Red flags: a bug, so the numbers mean nothing.
    Invalid,
    /// Cut short by Ctrl-C, or fastchess was killed or crashed.
    Aborted,
}

impl fmt::Display for Verdict {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Verdict::Pass => "PASS",
            Verdict::Fail => "FAIL",
            Verdict::Inconclusive => "INCONCLUSIVE",
            Verdict::Invalid => "INVALID",
            Verdict::Aborted => "ABORTED",
        })
    }
}

/// One time loss in 200 games is the operating system, not the engine: at
/// 5+0.05 a scheduling hiccup can cost a few milliseconds the engine never
/// had.
const GAMES_PER_TOLERATED_TIME_LOSS: u32 = 200;

/// The verdict for a match. An interrupted match is always `Aborted`.
pub fn decide_verdict(report: &MatchReport, interrupted: bool) -> Verdict {
    if interrupted {
        Verdict::Aborted
    } else if shows_a_bug(&report.red_flags, report.games) {
        Verdict::Invalid
    } else {
        sprt_verdict(report)
    }
}

fn shows_a_bug(flags: &RedFlags, games: u32) -> bool {
    let tolerated_time_losses = (games / GAMES_PER_TOLERATED_TIME_LOSS).max(1);
    flags.illegal_moves + flags.disconnects + flags.crashes > 0
        || flags.time_losses > tolerated_time_losses
}

/// The SPRT has decided once the log-likelihood ratio reaches either bound.
fn sprt_verdict(report: &MatchReport) -> Verdict {
    match (
        report.log_likelihood_ratio,
        report.log_likelihood_ratio_bounds,
    ) {
        (Some(ratio), Some((_, upper))) if ratio >= upper => Verdict::Pass,
        (Some(ratio), Some((lower, _))) if ratio <= lower => Verdict::Fail,
        _ => Verdict::Inconclusive,
    }
}

/// The process exit code for a verdict, so scripts and CI can branch on it.
pub fn exit_code(verdict: &Verdict) -> u8 {
    match verdict {
        Verdict::Pass => 0,
        Verdict::Fail => 1,
        Verdict::Inconclusive => 2,
        Verdict::Invalid => 3,
        Verdict::Aborted => 4,
    }
}

/// The block printed at the end of a run.
pub fn format_summary(report: &MatchReport, verdict: &Verdict, settings: &MatchSettings) -> String {
    let rule = "=".repeat(50);
    [
        rule.clone(),
        format!(
            "verdict    {verdict}: {}",
            meaning_of(verdict, report, settings)
        ),
        format!(
            "elo        {} +/- {} (95 %), normalized {}",
            signed_or_unknown(report.elo),
            unsigned_or_unknown(report.elo_error_margin),
            signed_or_unknown(report.normalized_elo),
        ),
        format!(
            "sprt       log-likelihood ratio {}, hypotheses [{}, {}] Elo",
            log_likelihood_progress(report),
            settings.lower_elo,
            settings.upper_elo,
        ),
        format!(
            "games      {} (W {} / D {} / L {}), pairs scoring 0..2: {:?}",
            report.games, report.wins, report.draws, report.losses, report.pair_score_counts,
        ),
        format!(
            "settings   tc {}, concurrency {}, cap {}, took {}",
            settings.time_control,
            settings.concurrency,
            settings.max_games,
            hours_minutes_seconds(report.duration),
        ),
        format!("red flags  {}", describe_red_flags(&report.red_flags)),
        rule,
    ]
    .join("\n")
}

fn meaning_of(verdict: &Verdict, report: &MatchReport, settings: &MatchSettings) -> String {
    match verdict {
        Verdict::Pass => format!(
            "the change is worth about {} Elo rather than {}",
            settings.upper_elo, settings.lower_elo
        ),
        Verdict::Fail => format!(
            "the change is worth about {} Elo rather than {}",
            settings.lower_elo, settings.upper_elo
        ),
        Verdict::Inconclusive => "the game cap came before a decision: not a pass".into(),
        Verdict::Invalid => format!(
            "{}: a bug, not weaker play; nothing recorded",
            describe_red_flags(&report.red_flags)
        ),
        Verdict::Aborted => "cut short: games so far, nothing recorded".into(),
    }
}

fn log_likelihood_progress(report: &MatchReport) -> String {
    match (
        report.log_likelihood_ratio,
        report.log_likelihood_ratio_bounds,
    ) {
        (Some(ratio), Some((lower, upper))) => {
            format!("{ratio:.2}, stops at {lower:.2} or {upper:.2}")
        }
        _ => "n/a".into(),
    }
}

fn describe_red_flags(flags: &RedFlags) -> String {
    if !flags.any() {
        return "none".into();
    }
    format!(
        "{} time losses, {} illegal moves, {} disconnects, {} crashes",
        flags.time_losses, flags.illegal_moves, flags.disconnects, flags.crashes
    )
}

fn signed_or_unknown(number: Option<f64>) -> String {
    number.map_or_else(|| "n/a".into(), |number| format!("{number:+.2}"))
}

fn unsigned_or_unknown(number: Option<f64>) -> String {
    number.map_or_else(|| "n/a".into(), |number| format!("{number:.2}"))
}

fn hours_minutes_seconds(duration: Option<Duration>) -> String {
    let Some(duration) = duration else {
        return "n/a".into();
    };
    let seconds = duration.as_secs();
    format!(
        "{}:{:02}:{:02}",
        seconds / 3600,
        seconds % 3600 / 60,
        seconds % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elo::match_report::parse_results;

    fn report_with_ratio(log_likelihood_ratio: f64) -> MatchReport {
        MatchReport {
            log_likelihood_ratio: Some(log_likelihood_ratio),
            log_likelihood_ratio_bounds: Some((-2.94, 2.94)),
            games: 100,
            ..MatchReport::default()
        }
    }

    fn settings() -> MatchSettings {
        MatchSettings {
            time_control: "10+0.1".into(),
            concurrency: 4,
            max_games: 40,
            lower_elo: 0.0,
            upper_elo: 10.0,
        }
    }

    #[test]
    fn the_log_likelihood_ratio_decides_pass_fail_or_inconclusive() {
        assert_eq!(
            decide_verdict(&report_with_ratio(3.0), false),
            Verdict::Pass
        );
        assert_eq!(
            decide_verdict(&report_with_ratio(-3.0), false),
            Verdict::Fail
        );
        assert_eq!(
            decide_verdict(&report_with_ratio(0.4), false),
            Verdict::Inconclusive
        );
    }

    #[test]
    fn an_interruption_overrides_everything() {
        assert_eq!(
            decide_verdict(&report_with_ratio(3.0), true),
            Verdict::Aborted
        );
    }

    #[test]
    fn red_flags_make_the_run_invalid() {
        let report = parse_results(include_str!("../../tests/fixtures/time_loss.txt")).unwrap();
        assert_eq!(decide_verdict(&report, false), Verdict::Invalid);
    }

    #[test]
    fn a_single_time_loss_is_tolerated() {
        let mut report = report_with_ratio(3.0);
        report.red_flags.time_losses = 1;
        assert_eq!(decide_verdict(&report, false), Verdict::Pass);
    }

    #[test]
    fn every_verdict_has_its_own_exit_code() {
        let codes = [
            Verdict::Pass,
            Verdict::Fail,
            Verdict::Inconclusive,
            Verdict::Invalid,
            Verdict::Aborted,
        ]
        .map(|verdict| exit_code(&verdict));
        assert_eq!(codes, [0, 1, 2, 3, 4]);
    }

    #[test]
    fn the_summary_shows_verdict_and_elo() {
        let report = parse_results(include_str!("../../tests/fixtures/no_sprt.txt")).unwrap();
        let summary = format_summary(&report, &Verdict::Inconclusive, &settings());
        assert!(summary.contains("INCONCLUSIVE"), "{summary}");
        assert!(summary.contains("-157.66 +/- 122.25"), "{summary}");
    }

    #[test]
    fn the_summary_shows_the_duration() {
        let report = MatchReport {
            duration: Some(Duration::from_secs(247)),
            ..report_with_ratio(0.0)
        };
        let summary = format_summary(&report, &Verdict::Inconclusive, &settings());
        assert!(summary.contains("0:04:07"), "{summary}");
    }
}
