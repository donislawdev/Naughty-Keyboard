//! How often a send says how far it got (`OBS-160`), the same on every system
//! that sends: Windows and macOS count it between two events, Linux between two
//! batches.

use core::time::Duration;

/// How often a send says how far it got.
///
/// A tenth of a second is about the shortest change a person reads as movement
/// rather than flicker, and it is the delay before the FIRST report too: a send
/// that ends sooner says nothing on the way, so the many short values do not
/// make a progress band blink into view and out again.
pub(crate) const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// When the next report of a send is due. Pure, so it is tested with instants
/// of its own rather than with a clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Cadence {
    next: std::time::Instant,
    every: Duration,
}

impl Cadence {
    /// The first report is due `every` after `start`.
    pub(crate) fn starting(start: std::time::Instant, every: Duration) -> Self {
        Self {
            next: start + every,
            every,
        }
    }

    /// Whether a report is due at `now`. When it is, the next one is due a full
    /// interval after `now` - a send that stalled does not owe a burst of
    /// reports for the time it stood still.
    pub(crate) fn due(&mut self, now: std::time::Instant) -> bool {
        if now < self.next {
            return false;
        }
        self.next = now + self.every;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_reported_before_the_first_interval() {
        // A short send says nothing on the way, so a band never blinks.
        let start = std::time::Instant::now();
        let mut cadence = Cadence::starting(start, PROGRESS_INTERVAL);
        assert!(!cadence.due(start));
        assert!(!cadence.due(start + Duration::from_millis(99)));
        assert!(cadence.due(start + Duration::from_millis(100)));
    }

    #[test]
    fn a_report_is_due_once_per_interval_and_a_stall_owes_no_burst() {
        let start = std::time::Instant::now();
        let mut cadence = Cadence::starting(start, PROGRESS_INTERVAL);
        let at = |ms| start + Duration::from_millis(ms);
        assert!(cadence.due(at(100)));
        assert!(!cadence.due(at(150)), "half an interval later is not due");
        assert!(cadence.due(at(200)));
        // The send stood still for a second: one report, then a full interval.
        assert!(cadence.due(at(1200)));
        assert!(
            !cadence.due(at(1250)),
            "no burst for the time it stood still"
        );
        assert!(cadence.due(at(1300)));
    }
}
