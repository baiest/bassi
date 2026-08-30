use std::time::{SystemTime, UNIX_EPOCH};

/// Formats a `SystemTime` as an RFC 3339 / ISO 8601 UTC timestamp with
/// second resolution (`2024-01-05T13:07:22Z`), so CSV rows are directly
/// parseable by `pandas.read_csv(..., parse_dates=[...])` without a timezone
/// dependency. No `chrono`/`time` crate: the calendar math is Howard
/// Hinnant's well-known `civil_from_days` algorithm, self-contained and easy
/// to unit-test — pulling in a date/time crate for one formatting function
/// would be overkill.
pub fn format_rfc3339_utc(time: SystemTime) -> String {
    let total_seconds = time
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);

    let days = total_seconds.div_euclid(86_400);
    let seconds_of_day = total_seconds.rem_euclid(86_400);

    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3600;
    let minute = (seconds_of_day % 3600) / 60;
    let second = seconds_of_day % 60;

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Days since the Unix epoch -> (year, month, day). Howard Hinnant's
/// `civil_from_days`: https://howardhinnant.github.io/date_algorithms.html
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_the_unix_epoch() {
        assert_eq!(format_rfc3339_utc(UNIX_EPOCH), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn formats_a_known_date_and_time() {
        // 2024-01-05T13:07:22Z, verified against `date -u -d @1704460042`.
        let time = UNIX_EPOCH + std::time::Duration::from_secs(1_704_460_042);
        assert_eq!(format_rfc3339_utc(time), "2024-01-05T13:07:22Z");
    }

    #[test]
    fn formats_a_leap_day() {
        // 2024-02-29T00:00:00Z.
        let time = UNIX_EPOCH + std::time::Duration::from_secs(1_709_164_800);
        assert_eq!(format_rfc3339_utc(time), "2024-02-29T00:00:00Z");
    }
}
