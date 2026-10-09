//! Clock and business calendar. Algeria is UTC+1 all year (no DST), spec §5.

use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, Utc};

/// Africa/Algiers offset from UTC in milliseconds.
pub const ALGIERS_OFFSET_MS: i64 = 3_600_000;

/// Current time as unix milliseconds UTC.
pub fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

/// Local wall-clock time in Algiers for a UTC timestamp.
/// Out-of-range timestamps (±262 000 years) collapse to `NaiveDateTime::MIN`.
pub fn algiers_datetime(ms: i64) -> NaiveDateTime {
    DateTime::<Utc>::from_timestamp_millis(ms.saturating_add(ALGIERS_OFFSET_MS))
        .map(|d| d.naive_utc())
        .unwrap_or(NaiveDateTime::MIN)
}

/// The Algiers day `[00:00, 24:00)` as Unix ms UTC (the appointment book's day).
pub fn algiers_day_bounds(day: NaiveDate) -> (i64, i64) {
    let start = day
        .and_time(chrono::NaiveTime::MIN)
        .and_utc()
        .timestamp_millis()
        - ALGIERS_OFFSET_MS;
    (start, start + 86_400_000)
}

/// Business date of a timestamp in Algiers.
pub fn business_date(ms: i64) -> NaiveDate {
    algiers_datetime(ms).date()
}

/// The business day of `ms` when the day turns at `cutoff_hours` o'clock (CFG-008, at most 6):
/// a sale at 01:30 with a 03:00 cut-off belongs to the day before — except on 1 January, where
/// the calendar day stays (numbering and the fiscal year never go back a year).
pub fn business_date_with(ms: i64, cutoff_hours: u8) -> NaiveDate {
    let local = algiers_datetime(ms);
    let shifted = (local - chrono::Duration::hours(i64::from(cutoff_hours.min(6)))).date();
    if shifted.year() < local.date().year() {
        local.date()
    } else {
        shifted
    }
}

/// Fiscal year = calendar year of the business date (spec §5.2).
pub fn fiscal_year(ms: i64) -> i32 {
    business_date(ms).year()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn business_date_is_algiers_local() {
        // 2026-12-31T23:30:00Z is 00:30 on 1 January 2027 in Algiers.
        assert_eq!(
            business_date(1_798_759_800_000),
            NaiveDate::from_ymd_opt(2027, 1, 1).unwrap()
        );
        assert_eq!(fiscal_year(1_798_759_800_000), 2027);
        // 2026-06-15T22:59:59.999Z is 23:59:59.999 local, still the 15th.
        assert_eq!(
            business_date(1_781_564_399_999),
            NaiveDate::from_ymd_opt(2026, 6, 15).unwrap()
        );
        // 2026-06-15T23:00:00Z is midnight local.
        assert_eq!(
            business_date(1_781_564_400_000),
            NaiveDate::from_ymd_opt(2026, 6, 16).unwrap()
        );
    }

    #[test]
    fn an_algiers_day_starts_at_local_midnight() {
        let (start, end) = algiers_day_bounds(NaiveDate::from_ymd_opt(2026, 6, 16).unwrap());
        // 2026-06-15T23:00:00Z is midnight local (see `business_date_is_algiers_local`).
        assert_eq!((start, end), (1_781_564_400_000, 1_781_650_800_000));
        assert_eq!(
            business_date(start),
            NaiveDate::from_ymd_opt(2026, 6, 16).unwrap()
        );
        assert_eq!(
            business_date(end - 1),
            NaiveDate::from_ymd_opt(2026, 6, 16).unwrap()
        );
        assert_eq!(
            business_date(end),
            NaiveDate::from_ymd_opt(2026, 6, 17).unwrap()
        );
    }

    #[test]
    fn the_business_day_turns_at_the_cutoff_hour_but_never_crosses_a_year() {
        use chrono::TimeZone;
        let at = |y, m, d, h, min| {
            chrono::FixedOffset::east_opt(3_600)
                .unwrap()
                .with_ymd_and_hms(y, m, d, h, min, 0)
                .unwrap()
                .timestamp_millis()
        };
        let day = |ms, cut| business_date_with(ms, cut).format("%Y-%m-%d").to_string();
        assert_eq!(
            day(at(2026, 9, 29, 1, 30), 3),
            "2026-09-28",
            "a café open past midnight"
        );
        assert_eq!(day(at(2026, 9, 29, 3, 0), 3), "2026-09-29");
        assert_eq!(day(at(2026, 9, 29, 1, 30), 0), "2026-09-29");
        assert_eq!(
            day(at(2027, 1, 1, 1, 0), 3),
            "2027-01-01",
            "the fiscal year never goes back"
        );
        assert_eq!(
            day(at(2026, 9, 29, 6, 30), 9),
            "2026-09-29",
            "at most 6 o'clock"
        );
    }

    #[test]
    fn algiers_datetime_adds_one_hour() {
        let local = algiers_datetime(1_790_596_800_000); // 2026-09-28T12:00:00Z
        assert_eq!(
            local.format("%Y-%m-%d %H:%M").to_string(),
            "2026-09-28 13:00"
        );
    }

    #[test]
    fn now_is_after_2026() {
        assert!(now_ms() > 1_767_225_600_000);
    }

    #[test]
    fn offset_is_one_hour_in_winter_and_summer() {
        assert_eq!(ALGIERS_OFFSET_MS, 60 * 60 * 1_000);
        // 2026-01-15T12:00:00Z and 2026-07-15T12:00:00Z: no DST shift.
        let winter = algiers_datetime(1_768_478_400_000);
        let summer = algiers_datetime(1_784_116_800_000);
        assert_eq!(
            winter.format("%Y-%m-%d %H:%M").to_string(),
            "2026-01-15 13:00"
        );
        assert_eq!(
            summer.format("%Y-%m-%d %H:%M").to_string(),
            "2026-07-15 13:00"
        );
    }

    #[test]
    fn fiscal_year_ends_at_local_midnight() {
        // 2026-12-31T22:59:59.999Z is 23:59:59.999 local on 31 December 2026.
        assert_eq!(fiscal_year(1_798_757_999_999), 2026);
        assert_eq!(fiscal_year(1_798_758_000_000), 2027);
        // Before the epoch: 1969-12-31T22:59:59.999Z is still 31 December locally.
        assert_eq!(
            business_date(-3_600_001),
            NaiveDate::from_ymd_opt(1969, 12, 31).unwrap()
        );
        assert_eq!(
            business_date(-3_600_000),
            NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()
        );
    }

    #[test]
    fn out_of_range_timestamps_collapse_without_panicking() {
        assert_eq!(algiers_datetime(i64::MAX), NaiveDateTime::MIN);
        assert_eq!(algiers_datetime(i64::MIN), NaiveDateTime::MIN);
        assert_eq!(business_date(i64::MAX), NaiveDateTime::MIN.date());
        assert_eq!(fiscal_year(i64::MIN), NaiveDateTime::MIN.year());
    }
}
