//! Test profiles: how long and how precisely a match runs.
//!
//! An SPRT does not measure "how many Elo"; it decides between two
//! hypotheses, "the change is worth `lower_elo`" and "the change is worth
//! `upper_elo`". The closer the two, the more games it needs: halving the
//! gap roughly quadruples the games. The profiles trade that precision for
//! machine time.

use clap::ValueEnum;

use crate::elo::EloArguments;

/// What the change under test claims.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    /// The change should make the engine stronger: prove a gain.
    Gain,
    /// A refactor, simplification or pure speed change: prove it loses
    /// nothing.
    NoRegression,
}

/// How much machine time the test may take.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Profile {
    /// The default: about 15 minutes on the Mac, a screening test that
    /// resolves changes worth about 20 Elo or more.
    Quick,
    /// Many hours, tight bounds, for subtle changes. Rare.
    Long,
    /// Sized for a 2-core GitHub runner.
    Ci,
}

/// Everything about a match that a profile decides.
#[derive(Clone, Debug, PartialEq)]
pub struct MatchSettings {
    /// fastchess time control: `seconds+increment` per side and game.
    pub time_control: String,
    /// Games played at the same time, roughly the number of busy cores.
    pub concurrency: u32,
    /// Stop after this many games even if the SPRT has not decided.
    pub max_games: u32,
    /// The SPRT's "nothing to see" hypothesis, in Elo.
    pub lower_elo: f64,
    /// The SPRT's "real effect" hypothesis, in Elo.
    pub upper_elo: f64,
}

/// The defaults of one profile.
struct ProfileDefaults {
    time_control: &'static str,
    concurrency: u32,
    max_games: u32,
    elo_bounds_for_gain: (f64, f64),
    elo_bounds_for_no_regression: (f64, f64),
}

/// The profile table. The no-regression bounds sit below zero: "no loss"
/// at 0 against "a real loss" below it.
fn profile_defaults(profile: Profile) -> ProfileDefaults {
    match profile {
        Profile::Quick => ProfileDefaults {
            time_control: "5+0.05",
            concurrency: 8,
            // Calibrated on the Mac (Apple M4, 10 cores): 200 games took
            // 4 min 7 s at concurrency 8 and no time losses, so 1500 games
            // take about 15 minutes.
            max_games: 1500,
            elo_bounds_for_gain: (0.0, 10.0),
            elo_bounds_for_no_regression: (-10.0, 0.0),
        },
        Profile::Long => ProfileDefaults {
            time_control: "10+0.1",
            concurrency: 8,
            max_games: 20_000,
            elo_bounds_for_gain: (0.0, 5.0),
            elo_bounds_for_no_regression: (-5.0, 0.0),
        },
        Profile::Ci => ProfileDefaults {
            time_control: "5+0.05",
            concurrency: 2,
            max_games: 400,
            elo_bounds_for_gain: (0.0, 10.0),
            elo_bounds_for_no_regression: (-10.0, 0.0),
        },
    }
}

/// The settings for `profile` and `mode`, with any value given on the
/// command line taking precedence.
pub fn match_settings(profile: Profile, mode: Mode, arguments: &EloArguments) -> MatchSettings {
    let defaults = profile_defaults(profile);
    let (lower_elo, upper_elo) = match mode {
        Mode::Gain => defaults.elo_bounds_for_gain,
        Mode::NoRegression => defaults.elo_bounds_for_no_regression,
    };
    MatchSettings {
        time_control: arguments
            .time_control
            .clone()
            .unwrap_or_else(|| defaults.time_control.to_string()),
        concurrency: arguments.concurrency.unwrap_or(defaults.concurrency),
        max_games: arguments.max_games.unwrap_or(defaults.max_games),
        lower_elo: arguments.lower_elo.unwrap_or(lower_elo),
        upper_elo: arguments.upper_elo.unwrap_or(upper_elo),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_gain_defaults() {
        let settings = match_settings(Profile::Quick, Mode::Gain, &EloArguments::default());
        assert_eq!(settings.time_control, "5+0.05");
        assert_eq!(settings.max_games, 1500);
        assert_eq!((settings.lower_elo, settings.upper_elo), (0.0, 10.0));
    }

    #[test]
    fn long_no_regression_bounds() {
        let settings = match_settings(Profile::Long, Mode::NoRegression, &EloArguments::default());
        assert_eq!((settings.lower_elo, settings.upper_elo), (-5.0, 0.0));
    }

    #[test]
    fn command_line_values_take_precedence() {
        let arguments = EloArguments {
            time_control: Some("3+0.03".into()),
            max_games: Some(600),
            ..EloArguments::default()
        };
        let settings = match_settings(Profile::Quick, Mode::Gain, &arguments);
        assert_eq!(settings.time_control, "3+0.03");
        assert_eq!(settings.max_games, 600);
    }
}
