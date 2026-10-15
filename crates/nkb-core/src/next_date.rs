//! The rule behind a value of type `next-date` (`pack-format.md` 6).
//!
//! Some test values are dates that only mean something while they are ahead:
//! the hour a clock change skips, the hour it repeats. Written out, such a date
//! goes stale within a year and the value quietly stops testing anything - a
//! form that takes only future dates refuses it for being past and never
//! reaches the hour that does not exist (`OBS-182`). So the pack names a yearly
//! rule instead, and the date is worked out on the day the value is inserted.
//!
//! # A rule, not an offset
//!
//! The day of a clock change is "the last Sunday of March", which no offset
//! from today can express. `date-offset` stays reserved for "today plus N days"
//! until the catalogue needs one (`D133`).
//!
//! # The text has one length on every day
//!
//! `architektura.md` 6.1 wants the size of a value known before the value
//! exists. A date here is always written with a four digit year, which
//! [`NextDate::day_for`] guarantees for every today it can be given, so the
//! length depends on the rule alone and [`NextDate::metrics`] answers without a
//! day.

use crate::calendar::{Day, Weekday, days_in_month};
use crate::metrics::TextMetrics;
use crate::text::LiteralText;

/// How many days after today's date in UTC the earliest date lies.
///
/// Two, so that the date is ahead of today in every time zone. Today arrives as
/// the date in UTC, and no zone is a whole day further ahead than that - the
/// furthest, UTC+14, is fourteen hours ahead - so the day after tomorrow in UTC
/// is after today everywhere. A date that is today somewhere would already be
/// past there by the afternoon, and that is the stale value this type exists to
/// avoid.
pub const DAYS_AHEAD: i64 = 2;

/// Which occurrence of a weekday in its month.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Week {
    /// Days 1 to 7.
    First,
    /// Days 8 to 14.
    Second,
    /// Days 15 to 21.
    Third,
    /// Days 22 to 28.
    Fourth,
    /// The last seven days of the month, whatever its length.
    Last,
}

impl Week {
    /// Every week a rule can name. No fifth: a month has a fifth Sunday in
    /// some years only, and a rule that has no day in a given year would leave
    /// the value with nothing to insert.
    pub const ALL: [Self; 5] = [
        Self::First,
        Self::Second,
        Self::Third,
        Self::Fourth,
        Self::Last,
    ];

    /// The word a pack file writes for this week.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::First => "first",
            Self::Second => "second",
            Self::Third => "third",
            Self::Fourth => "fourth",
            Self::Last => "last",
        }
    }

    /// The week a pack file names, or nothing for a word outside the vocabulary.
    #[must_use]
    pub fn from_name(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|week| week.name() == word)
    }

    /// How many whole weeks lie before this one, for the four that count from
    /// the start of the month.
    fn weeks_before(self) -> Option<i64> {
        match self {
            Self::First => Some(0),
            Self::Second => Some(1),
            Self::Third => Some(2),
            Self::Fourth => Some(3),
            Self::Last => None,
        }
    }
}

/// A time of day, to the minute, as a pack file writes it: `02:30`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimeOfDay {
    hour: u32,
    minute: u32,
}

impl TimeOfDay {
    /// Reads `HH:MM` on the 24 hour clock, or nothing for anything else.
    ///
    /// Exactly two digits each side. `2:30`, `02:30:00` and `24:00` are refused:
    /// one way to write a time is a way a reviewer can read at a glance, and the
    /// time written is the time the value carries, character for character.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        let [h1, h2, b':', m1, m2] = bytes else {
            return None;
        };
        let digit = |byte: &u8| byte.is_ascii_digit().then(|| u32::from(*byte - b'0'));
        let hour = digit(h1)? * 10 + digit(h2)?;
        let minute = digit(m1)? * 10 + digit(m2)?;
        (hour <= 23 && minute <= 59).then_some(Self { hour, minute })
    }

    #[must_use]
    pub fn hour(self) -> u32 {
        self.hour
    }

    #[must_use]
    pub fn minute(self) -> u32 {
        self.minute
    }

    /// The time as a pack file writes it, which is also how the value writes it.
    #[must_use]
    pub fn written(self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }
}

/// The next day matching a yearly rule - "the last Sunday of March" - with an
/// optional time of day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NextDate {
    month: u32,
    week: Week,
    weekday: Weekday,
    time: Option<TimeOfDay>,
}

impl NextDate {
    /// A rule, or nothing when `month` is not 1 to 12.
    ///
    /// The only check there is to make: every week and weekday of the
    /// vocabulary exists in every month of every year, which is why there is no
    /// fifth week.
    #[must_use]
    pub fn new(month: u32, week: Week, weekday: Weekday, time: Option<TimeOfDay>) -> Option<Self> {
        (1..=12).contains(&month).then_some(Self {
            month,
            week,
            weekday,
            time,
        })
    }

    #[must_use]
    pub fn month(&self) -> u32 {
        self.month
    }

    #[must_use]
    pub fn week(&self) -> Week {
        self.week
    }

    #[must_use]
    pub fn weekday(&self) -> Weekday {
        self.weekday
    }

    #[must_use]
    pub fn time(&self) -> Option<TimeOfDay> {
        self.time
    }

    /// The one day in `year` that matches the rule.
    #[must_use]
    pub fn day_in(&self, year: i32) -> Day {
        // The month is 1 to 12 by construction, so both ends exist in every
        // year. The fallback is never taken, and is a day rather than a stop.
        let Some(weeks_before) = self.week.weeks_before() else {
            let last_day = days_in_month(year, self.month);
            let end = Day::from_civil(year, self.month, last_day).unwrap_or(Day::UNIX_EPOCH);
            // Backwards from the month's last day to the nearest such weekday.
            return end.after(-((7 - end.days_until(self.weekday)) % 7));
        };
        let start = Day::from_civil(year, self.month, 1).unwrap_or(Day::UNIX_EPOCH);
        start.after(start.days_until(self.weekday) + 7 * weeks_before)
    }

    /// The first matching day on or after `earliest`.
    ///
    /// Each year has exactly one, so it is this year's or next year's.
    #[must_use]
    pub fn first_on_or_after(&self, earliest: Day) -> Day {
        let (year, _, _) = earliest.civil();
        let this_year = self.day_in(year);
        if this_year >= earliest {
            this_year
        } else {
            self.day_in(year.saturating_add(1))
        }
    }

    /// The day this value names when inserted on `today`, a date in UTC.
    ///
    /// The first matching day at least [`DAYS_AHEAD`] days on, kept within the
    /// years 1000 to 9999. No real clock leaves that range: the bound is for a
    /// clock gone wrong, and it is what keeps every date four digits long, so
    /// that the length is known before the date is.
    #[must_use]
    pub fn day_for(&self, today: Day) -> Day {
        let first = Day::from_civil(1000, 1, 1).unwrap_or(Day::UNIX_EPOCH);
        // The first day of 9999, not the last: every rule has a day in 9999 on
        // or after it, so the answer never spills into a fifth digit.
        let last = Day::from_civil(9999, 1, 1).unwrap_or(Day::UNIX_EPOCH);
        let earliest = today.after(DAYS_AHEAD).clamp(first, last);
        self.first_on_or_after(earliest)
    }

    /// The text this value inserts on `today`: `2027-03-28 02:30`, or
    /// `2027-03-28` without a time.
    ///
    /// The order ISO 8601 writes a date in, with a space before the time as
    /// most forms show it - the same shape the catalogue wrote by hand before
    /// this type existed.
    #[must_use]
    pub fn on(&self, today: Day) -> LiteralText {
        let (year, month, day) = self.day_for(today).civil();
        let mut text = format!("{year:04}-{month:02}-{day:02}");
        if let Some(time) = self.time {
            text.push(' ');
            text.push_str(&time.written());
        }
        LiteralText::new(text)
    }

    /// The size of the text, which is the same on every day.
    #[must_use]
    pub fn metrics(&self) -> TextMetrics {
        TextMetrics::of(self.on(Day::UNIX_EPOCH).as_str())
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

    fn rule(month: u32, week: Week, weekday: Weekday, time: Option<&str>) -> NextDate {
        let time = time.map(|text| TimeOfDay::parse(text).expect("a written time"));
        NextDate::new(month, week, weekday, time).expect("a month of the year")
    }

    fn spring() -> NextDate {
        rule(3, Week::Last, Weekday::Sunday, Some("02:30"))
    }

    fn autumn() -> NextDate {
        rule(10, Week::Last, Weekday::Sunday, Some("02:30"))
    }

    /// A month and a day of it.
    type MonthAndDay = (u32, u32);

    /// The days central Europe changed or will change its clocks, from the IANA
    /// time zone database rather than from this code: `Europe/Warsaw`, tzdata
    /// 2026c, read with Python's `zoneinfo` on 2026-10-15. For each year, the
    /// day on which 02:30 local time did not exist and the day on which it
    /// happened twice. The EU rule has held since 1996.
    const CLOCK_CHANGES: [(i32, MonthAndDay, MonthAndDay); 12] = [
        (1996, (3, 31), (10, 27)),
        (2000, (3, 26), (10, 29)),
        (2004, (3, 28), (10, 31)),
        (2010, (3, 28), (10, 31)),
        (2020, (3, 29), (10, 25)),
        (2024, (3, 31), (10, 27)),
        (2025, (3, 30), (10, 26)),
        (2026, (3, 29), (10, 25)),
        (2027, (3, 28), (10, 31)),
        (2028, (3, 26), (10, 29)),
        (2029, (3, 25), (10, 28)),
        (2037, (3, 29), (10, 25)),
    ];

    #[test]
    fn the_rule_finds_the_days_the_time_zone_database_names() {
        // The oracle is not this code: the table above comes from tzdata.
        for (year, (march_month, march_day), (october_month, october_day)) in CLOCK_CHANGES {
            assert_eq!(
                spring().day_in(year),
                day(year, march_month, march_day),
                "{year}"
            );
            assert_eq!(
                autumn().day_in(year),
                day(year, october_month, october_day),
                "{year}"
            );
        }
    }

    #[test]
    fn every_week_of_the_vocabulary_lands_on_its_weekday_inside_its_seven_days() {
        // Every rule, in every month of a full 400 year cycle and around it.
        for year in 1999..2401 {
            for month in 1..=12 {
                for week in Week::ALL {
                    for weekday in Weekday::ALL {
                        let found = rule(month, week, weekday, None).day_in(year);
                        let (y, m, d) = found.civil();
                        assert_eq!((y, m), (year, month), "{year}-{month} {week:?} {weekday:?}");
                        assert_eq!(found.weekday(), weekday);
                        let range = match week.weeks_before() {
                            Some(before) => {
                                let start = u32::try_from(before).unwrap_or(0) * 7 + 1;
                                start..=start + 6
                            }
                            None => {
                                let last = days_in_month(year, month);
                                last - 6..=last
                            }
                        };
                        assert!(
                            range.contains(&d),
                            "{year}-{month} {week:?} {weekday:?}: {d}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn today_gives_the_next_clock_change_at_least_two_days_ahead() {
        // The day this type was written: the autumn change of 2026 is still to
        // come, the spring one of 2027 is next.
        let today = day(2026, 10, 15);
        assert_eq!(autumn().on(today).as_str(), "2026-10-25 02:30");
        assert_eq!(spring().on(today).as_str(), "2027-03-28 02:30");

        // Three days before the change it is still this year's...
        assert_eq!(autumn().on(day(2026, 10, 22)).as_str(), "2026-10-25 02:30");
        assert_eq!(autumn().on(day(2026, 10, 23)).as_str(), "2026-10-25 02:30");
        // ...and from the day before the day before, next year's: on 24 October
        // in UTC it is already 25 October in Kiribati.
        assert_eq!(autumn().on(day(2026, 10, 24)).as_str(), "2027-10-31 02:30");
        assert_eq!(autumn().on(day(2026, 10, 25)).as_str(), "2027-10-31 02:30");
    }

    #[test]
    fn the_date_is_always_ahead_by_at_least_two_days_and_at_most_a_year_and_a_week() {
        // The longest wait: this year's day falls the day before the earliest
        // allowed one, and next year's lies 371 days after it - a weekday rule
        // moves by up to six days from one year to the next, so the same rule
        // is 364 or 371 days apart. Hence DAYS_AHEAD - 1 + 371.
        let rules = [
            spring(),
            autumn(),
            rule(2, Week::First, Weekday::Monday, None),
        ];
        let start = day(2025, 1, 1);
        for offset in 0..(366 * 4) {
            let today = start.after(offset);
            for one in rules {
                let found = one.day_for(today);
                let ahead = found.days_since_1970() - today.days_since_1970();
                assert!(ahead >= DAYS_AHEAD, "{today:?} {one:?}: {ahead}");
                assert!(ahead <= DAYS_AHEAD - 1 + 371, "{today:?} {one:?}: {ahead}");
            }
        }
    }

    #[test]
    fn a_rule_without_a_time_writes_the_date_alone() {
        let first_monday = rule(2, Week::First, Weekday::Monday, None);
        assert_eq!(first_monday.on(day(2026, 10, 15)).as_str(), "2027-02-01");
        assert_eq!(first_monday.metrics().code_points, 10);
    }

    #[test]
    fn the_length_is_the_same_on_every_day_however_wrong_the_clock() {
        // metrics() answers without a day, so it is only true if every day
        // gives the same length - including a clock in year 1 or year 50 000.
        let promised = spring().metrics();
        assert_eq!(promised.code_points, 16);
        assert_eq!(promised.bytes, 16);
        for today in [
            Day::from_days_since_1970(i64::MIN),
            day(1, 1, 1),
            day(999, 12, 31),
            Day::UNIX_EPOCH,
            day(2026, 10, 15),
            day(9998, 12, 31),
            day(9999, 12, 31),
            day(50_000, 6, 1),
            Day::from_days_since_1970(i64::MAX),
        ] {
            let text = spring().on(today);
            assert_eq!(
                TextMetrics::of(text.as_str()),
                promised,
                "{today:?}: {}",
                text.as_str()
            );
        }
    }

    #[test]
    fn a_time_is_read_only_in_its_one_written_form() {
        let read = TimeOfDay::parse("02:30").map(|time| (time.hour(), time.minute()));
        assert_eq!(read, Some((2, 30)));
        assert_eq!(
            TimeOfDay::parse("00:00").map(TimeOfDay::written).as_deref(),
            Some("00:00")
        );
        assert_eq!(
            TimeOfDay::parse("23:59").map(TimeOfDay::written).as_deref(),
            Some("23:59")
        );
        for refused in [
            "2:30",
            "02:3",
            "02:30:00",
            "24:00",
            "23:60",
            "02-30",
            "0230",
            "",
            " 02:30",
            "02:30 ",
            "\u{FF10}2:30",
            "+2:30",
            "-2:30",
        ] {
            assert_eq!(TimeOfDay::parse(refused), None, "{refused:?}");
        }
    }

    #[test]
    fn a_month_outside_the_year_is_no_rule() {
        assert_eq!(NextDate::new(0, Week::Last, Weekday::Sunday, None), None);
        assert_eq!(NextDate::new(13, Week::Last, Weekday::Sunday, None), None);
        assert!(NextDate::new(12, Week::Last, Weekday::Sunday, None).is_some());
    }

    #[test]
    fn every_week_name_reads_back_and_nothing_else_does() {
        for week in Week::ALL {
            assert_eq!(Week::from_name(week.name()), Some(week));
        }
        for word in ["Last", "fifth", "1", "", "last "] {
            assert_eq!(Week::from_name(word), None, "{word:?}");
        }
    }
}
