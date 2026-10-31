//! UTC timestamps (`2026-10-31T13:45:07Z`) without a date library.

use std::time::{SystemTime, UNIX_EPOCH};

const SECONDS_PER_DAY: u64 = 86_400;

/// Now, as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn current_utc_timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    utc_timestamp(seconds)
}

/// Seconds since 1970-01-01 as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn utc_timestamp(seconds_since_1970: u64) -> String {
    let (year, month, day) = calendar_date(seconds_since_1970 / SECONDS_PER_DAY);
    let seconds_today = seconds_since_1970 % SECONDS_PER_DAY;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds_today / 3600,
        seconds_today % 3600 / 60,
        seconds_today % 60
    )
}

/// The `(year, month, day)` of the day `days_since_1970` days after
/// 1970-01-01.
///
/// This is Howard Hinnant's `civil_from_days`. Two ideas make it short.
/// First, count years from March: the leap day then falls at the very end
/// of the year, so it never shifts the months after it. Second, the
/// Gregorian calendar repeats every 400 years (an "era" of 146,097 days),
/// so only a position inside one era needs working out.
fn calendar_date(days_since_1970: u64) -> (u64, u64, u64) {
    const DAYS_FROM_0000_03_01_TO_1970_01_01: u64 = 719_468;
    const DAYS_PER_ERA: u64 = 146_097;

    let days_since_0000_03_01 = days_since_1970 + DAYS_FROM_0000_03_01_TO_1970_01_01;
    let era = days_since_0000_03_01 / DAYS_PER_ERA;
    let day_of_era = days_since_0000_03_01 % DAYS_PER_ERA;
    // Every 4 years add a leap day, except every 100, except every 400.
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    // Months from March have lengths 31, 30, 31, 30, 31, 31, 30, ...: a
    // pattern that `(5 * day + 2) / 153` reproduces exactly.
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    };
    let year = era * 400 + year_of_era + u64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_dates() {
        assert_eq!(utc_timestamp(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc_timestamp(1_793_454_307), "2026-10-31T13:45:07Z");
    }

    #[test]
    fn a_leap_day() {
        assert_eq!(utc_timestamp(1_709_251_199), "2024-02-29T23:59:59Z");
    }
}
