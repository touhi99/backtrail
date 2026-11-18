//! UTC timestamps as text (`2026-11-14T09:30:00Z`), the form every table stores.
use std::time::{SystemTime, UNIX_EPOCH};

/// The current time.
pub fn now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    iso(secs)
}

/// A Unix time as a timestamp.
pub fn iso(secs: i64) -> String {
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    let (y, m, d) = civil(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Whole days from the date in `ts` to today. A timestamp that cannot be read counts as 1970-01-01.
pub fn days_ago(ts: &str) -> i64 {
    day_number(&now()) - day_number(ts)
}

/// Days since 1970-01-01 for the `YYYY-MM-DD` at the start of `ts`.
fn day_number(ts: &str) -> i64 {
    let part = |range: std::ops::Range<usize>| ts.get(range).and_then(|s| s.parse::<i64>().ok());
    let (Some(y), Some(m), Some(d)) = (part(0..4), part(5..7), part(8..10)) else {
        return 0;
    };
    // Howard Hinnant's days_from_civil.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

/// Year, month and day for a count of days since 1970-01-01 (Howard Hinnant's `civil_from_days`).
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}
