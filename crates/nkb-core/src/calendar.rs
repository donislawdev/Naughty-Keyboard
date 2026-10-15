//! Calendar days, and nothing finer.
//!
//! A value of type `next-date` (`pack-format.md` 6) is a date worked out on the
//! day it is inserted, so this layer counts days, names weekdays and turns a day
//! into a year, a month and a day of the month. It needs no hours, no time zones
//! and no leap seconds, and it has none: a day here is a day of the proleptic
//! Gregorian calendar, the one ISO 8601 writes dates in.
//!
//! # Where today comes from
//!
//! Not from here. This layer has no clock (`lib.rs`), so today arrives as a
//! [`Day`] from the clock port one layer out, and every function below answers
//! for the day it is given. That is what lets a test ask about 2099 without
//! waiting for it.
//!
//! # Why this is written out rather than taken from a crate
//!
//! A date library would be a dependency, and this crate has none on purpose. The
//! conversions are the published civil-from-days and days-from-civil algorithms,
//! exact for every day in the range [`Day`] keeps, and shorter than the argument
//! for a crate that would replace them. The arithmetic shifts the year to start
//! in March, which puts the leap day at the end of the year and removes every
//! special case for February.

/// How far from 1970 a [`Day`] may lie, in days: about eleven million years
/// either way.
///
/// Not a limit anybody meets. It exists so that no input - a clock gone wrong, a
/// test asking for the far future - can overflow the arithmetic below, which
/// would stop the program in a debug build and give a wrong date in a release
/// one. Every constructor and every step clamps to it.
const DAY_LIMIT: i64 = 1 << 32;

/// Days from 0000-03-01 to 1970-01-01 - the offset the published algorithms
/// count from.
const EPOCH_SHIFT: i64 = 719_468;

/// Days in one cycle of the Gregorian calendar, which repeats every 400 years.
const DAYS_PER_ERA: i64 = 146_097;

/// One calendar day, counted from 1970-01-01.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Day(i64);

/// A day of the week, in the order ISO 8601 numbers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl Weekday {
    /// Every weekday, Monday first.
    pub const ALL: [Self; 7] = [
        Self::Monday,
        Self::Tuesday,
        Self::Wednesday,
        Self::Thursday,
        Self::Friday,
        Self::Saturday,
        Self::Sunday,
    ];

    /// The word a pack file writes for this day - `pack-format.md` 6.
    ///
    /// Lower case English, like every other closed vocabulary of the format, so
    /// that a file reads the same on every machine whatever its language.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Monday => "monday",
            Self::Tuesday => "tuesday",
            Self::Wednesday => "wednesday",
            Self::Thursday => "thursday",
            Self::Friday => "friday",
            Self::Saturday => "saturday",
            Self::Sunday => "sunday",
        }
    }

    /// The day a pack file names, or nothing for a word outside the vocabulary.
    ///
    /// Exact, case included: `Sunday` is refused rather than read, because a
    /// format that forgives one spelling is asked to forgive the next.
    #[must_use]
    pub fn from_name(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|day| day.name() == word)
    }

    /// Monday is 0 and Sunday is 6.
    fn number(self) -> i64 {
        match self {
            Self::Monday => 0,
            Self::Tuesday => 1,
            Self::Wednesday => 2,
            Self::Thursday => 3,
            Self::Friday => 4,
            Self::Saturday => 5,
            Self::Sunday => 6,
        }
    }
}

impl Day {
    /// 1970-01-01.
    pub const UNIX_EPOCH: Self = Self(0);

    /// The day `days` after 1970-01-01, or before it for a negative count.
    #[must_use]
    pub const fn from_days_since_1970(days: i64) -> Self {
        // Written out rather than `clamp`, so that a constant can be made of it.
        if days < -DAY_LIMIT {
            Self(-DAY_LIMIT)
        } else if days > DAY_LIMIT {
            Self(DAY_LIMIT)
        } else {
            Self(days)
        }
    }

    /// The day with this year, month and day of the month, or nothing when there
    /// is no such day - 30 February, month 13, day 0.
    #[must_use]
    pub fn from_civil(year: i32, month: u32, day: u32) -> Option<Self> {
        if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return None;
        }
        let month = i64::from(month);
        let year = i64::from(year) - i64::from(month <= 2);
        let era = year.div_euclid(400);
        let year_of_era = year.rem_euclid(400);
        // Months counted from March, so that February is last.
        let shifted_month = (month + 9) % 12;
        let day_of_year = (153 * shifted_month + 2) / 5 + i64::from(day) - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        Some(Self::from_days_since_1970(
            era * DAYS_PER_ERA + day_of_era - EPOCH_SHIFT,
        ))
    }

    /// Days since 1970-01-01, negative before it.
    #[must_use]
    pub fn days_since_1970(self) -> i64 {
        self.0
    }

    /// The year, the month (1 to 12) and the day of the month (1 to 31).
    #[must_use]
    pub fn civil(self) -> (i32, u32, u32) {
        let shifted = self.0 + EPOCH_SHIFT;
        let era = shifted.div_euclid(DAYS_PER_ERA);
        let day_of_era = shifted.rem_euclid(DAYS_PER_ERA);
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let shifted_month = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
        let month = if shifted_month < 10 {
            shifted_month + 3
        } else {
            shifted_month - 9
        };
        let year = year_of_era + era * 400 + i64::from(month <= 2);

        // The limit on a day keeps the year within eleven million either way, so
        // these conversions cannot fail. Written as fallbacks rather than as an
        // `expect`, because a program that stops over a date is worse than one
        // that prints a wrong one.
        (
            i32::try_from(year).unwrap_or(i32::MAX),
            u32::try_from(month).unwrap_or(1),
            u32::try_from(day).unwrap_or(1),
        )
    }

    /// The day of the week.
    #[must_use]
    pub fn weekday(self) -> Weekday {
        // 1970-01-01 was a Thursday, number 3 when Monday is 0.
        let number = (self.0 + 3).rem_euclid(7);
        Weekday::ALL
            .into_iter()
            .find(|day| day.number() == number)
            .unwrap_or(Weekday::Monday)
    }

    /// The day `days` later, or earlier for a negative count.
    #[must_use]
    pub fn after(self, days: i64) -> Self {
        Self::from_days_since_1970(self.0.saturating_add(days))
    }

    /// How many days lie from `self` forward to the next `weekday`, zero when
    /// `self` already is one.
    #[must_use]
    pub fn days_until(self, weekday: Weekday) -> i64 {
        (weekday.number() - self.weekday().number()).rem_euclid(7)
    }
}

/// Whether February of `year` has 29 days.
///
/// Every fourth year, except every hundredth, except every four hundredth: 2000
/// was a leap year, 1900 was not, and 2100 will not be.
#[must_use]
pub fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// How many days `month` of `year` has. Zero for a month that does not exist.
#[must_use]
pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    fn day(year: i32, month: u32, day: u32) -> Day {
        Day::from_civil(year, month, day).expect("a real day")
    }

    #[test]
    fn the_calendar_arithmetic_agrees_with_dates_that_can_be_looked_up() {
        // Fixed points rather than a property, because the failure mode of date
        // arithmetic is being wrong by one day in a corner nobody visits. Each of
        // these is a day somebody can check against a calendar.
        for (days, expected) in [
            (0_i64, (1970, 1, 1)),
            (1, (1970, 1, 2)),
            (58, (1970, 2, 28)),
            (59, (1970, 3, 1)),
            // 2000 was a leap year and 1900 was not - the rule that catches naive
            // implementations, and the reason the century terms exist above.
            (11_016, (2000, 2, 29)),
            (11_017, (2000, 3, 1)),
            // 2100 is not a leap year, so this pair straddles a February that has
            // twenty eight days in a year divisible by four.
            (47_540, (2100, 2, 28)),
            (47_541, (2100, 3, 1)),
            (20_703, (2026, 9, 7)),
            (20_704, (2026, 9, 8)),
            (-1, (1969, 12, 31)),
        ] {
            assert_eq!(
                Day::from_days_since_1970(days).civil(),
                expected,
                "day {days}"
            );
            assert_eq!(
                Day::from_civil(expected.0, expected.1, expected.2),
                Some(Day::from_days_since_1970(days)),
                "{expected:?}"
            );
        }
    }

    #[test]
    fn the_two_directions_agree_on_every_day_of_four_centuries() {
        // One full Gregorian cycle around the present, plus a stretch before
        // 1970, where the arithmetic works with negative numbers. A day that
        // does not come back as itself is a day one of the two directions
        // gets wrong.
        for days in -40_000..DAYS_PER_ERA {
            let one = Day::from_days_since_1970(days);
            let (year, month, day_of_month) = one.civil();
            assert_eq!(
                Day::from_civil(year, month, day_of_month),
                Some(one),
                "day {days}"
            );
        }
    }

    #[test]
    fn consecutive_days_step_through_the_calendar_without_gaps() {
        // The month ends where days_in_month says it does, in every month of
        // a cycle - which is what a table lookup would get wrong silently.
        let mut previous = Day::from_days_since_1970(-1).civil();
        for days in 0..DAYS_PER_ERA {
            let (year, month, day_of_month) = Day::from_days_since_1970(days).civil();
            if day_of_month == 1 {
                assert_eq!(
                    previous.2,
                    days_in_month(previous.0, previous.1),
                    "day {days}"
                );
            } else {
                assert_eq!((year, month, day_of_month - 1), previous, "day {days}");
            }
            previous = (year, month, day_of_month);
        }
    }

    #[test]
    fn a_day_that_does_not_exist_is_refused() {
        assert_eq!(Day::from_civil(2026, 2, 29), None);
        assert_eq!(Day::from_civil(2026, 2, 30), None);
        assert_eq!(Day::from_civil(2026, 4, 31), None);
        assert_eq!(Day::from_civil(2026, 13, 1), None);
        assert_eq!(Day::from_civil(2026, 0, 1), None);
        assert_eq!(Day::from_civil(2026, 1, 0), None);
        // And the leap day where it does exist.
        assert!(Day::from_civil(2028, 2, 29).is_some());
        assert!(Day::from_civil(2000, 2, 29).is_some());
        assert_eq!(Day::from_civil(1900, 2, 29), None);
    }

    #[test]
    fn weekdays_fall_where_a_calendar_puts_them() {
        assert_eq!(Day::UNIX_EPOCH.weekday(), Weekday::Thursday);
        assert_eq!(day(1969, 12, 31).weekday(), Weekday::Wednesday);
        assert_eq!(day(2000, 1, 1).weekday(), Weekday::Saturday);
        assert_eq!(day(2026, 10, 15).weekday(), Weekday::Thursday);
        assert_eq!(day(2027, 3, 28).weekday(), Weekday::Sunday);
        assert_eq!(day(2027, 10, 31).weekday(), Weekday::Sunday);
        assert_eq!(day(1582, 10, 15).weekday(), Weekday::Friday);
    }

    #[test]
    fn the_week_runs_monday_to_sunday_without_a_gap() {
        let monday = day(2026, 10, 12);
        for (offset, expected) in Weekday::ALL.into_iter().enumerate() {
            let offset = i64::try_from(offset).unwrap_or(0);
            assert_eq!(monday.after(offset).weekday(), expected);
        }
        assert_eq!(monday.after(7).weekday(), Weekday::Monday);
        assert_eq!(monday.after(-1).weekday(), Weekday::Sunday);
    }

    #[test]
    fn days_until_a_weekday_counts_forward_and_is_zero_on_the_day() {
        let thursday = day(2026, 10, 15);
        assert_eq!(thursday.days_until(Weekday::Thursday), 0);
        assert_eq!(thursday.days_until(Weekday::Sunday), 3);
        assert_eq!(thursday.days_until(Weekday::Wednesday), 6);
    }

    #[test]
    fn every_weekday_name_reads_back_and_nothing_else_does() {
        for weekday in Weekday::ALL {
            assert_eq!(Weekday::from_name(weekday.name()), Some(weekday));
        }
        for word in ["Sunday", "sun", "", "sunday ", "niedziela"] {
            assert_eq!(Weekday::from_name(word), None, "{word:?}");
        }
    }

    #[test]
    fn no_day_overflows_the_arithmetic_however_far_it_is_asked_for() {
        // A clock gone wrong, or a caller adding a large number: clamped, never
        // a stop and never a wrap into the other direction.
        let far = Day::from_days_since_1970(i64::MAX);
        assert_eq!(far, Day::from_days_since_1970(DAY_LIMIT));
        assert!(far.civil().0 > 11_000_000);
        assert_eq!(far.after(i64::MAX), far);
        let early = Day::from_days_since_1970(i64::MIN);
        assert!(early.civil().0 < -11_000_000);
        assert_eq!(early.after(i64::MIN), early);
        let _ = (far.weekday(), early.weekday());
    }

    #[test]
    fn leap_years_follow_all_three_rules() {
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(2026));
        assert!(!is_leap_year(1900));
        assert!(is_leap_year(2000));
        assert!(!is_leap_year(2100));
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2028, 2), 29);
        assert_eq!(days_in_month(2026, 13), 0);
    }
}
