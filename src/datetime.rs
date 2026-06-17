//! Minimal UTC date/time helpers — no chrono dependency.
//!
//! Messages store a plain Unix-seconds instant; the UI derives the `HH:MM`
//! display and the day grouping from these functions.

use std::time::{SystemTime, UNIX_EPOCH};

const SECS_PER_DAY: i64 = 86_400;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Seconds since the Unix epoch (UTC). Zero if the clock predates the epoch.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Whole days since the Unix epoch for an instant — the day-grouping key.
pub fn epoch_day(at: i64) -> i64 {
    at.div_euclid(SECS_PER_DAY)
}

/// `HH:MM` (UTC) for an instant.
pub fn hhmm(at: i64) -> String {
    let tod = at.rem_euclid(SECS_PER_DAY);
    format!("{:02}:{:02}", tod / 3600, (tod % 3600) / 60)
}

/// A WhatsApp-style day heading relative to `today`: Today / Yesterday / date.
pub fn day_label(day: i64, today: i64) -> String {
    match today - day {
        0 => "Today".to_string(),
        1 => "Yesterday".to_string(),
        _ => {
            let (year, month, dom) = civil(day);
            format!("{} {}, {}", MONTHS[(month - 1) as usize], dom, year)
        }
    }
}

/// Civil `(year, month, day)` from days since the epoch (Hinnant's algorithm).
fn civil(day: i64) -> (i64, u32, u32) {
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let dom = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    (year + if month <= 2 { 1 } else { 0 }, month, dom)
}
