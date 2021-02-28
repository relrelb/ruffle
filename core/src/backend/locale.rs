use std::time::Duration;
use chrono::{DateTime, FixedOffset, TimeZone, Utc};

pub trait LocaleBackend {
    /// Get the amount of time since the SWF was launched.
    /// Used by the `getTimer` ActionScript call.
    fn time_since_launch(&self) -> Duration;

    fn get_current_date_time(&self) -> DateTime<Utc>;

    fn get_timezone(&self) -> FixedOffset;
}

/// Locale backend that mostly does nothing.
///
/// For tests, this backend will emulate being in Nepal with a local time of 2001-02-03 at 04:05:06.
/// Nepal has a timezone offset of +5:45, and has never used DST.
/// This makes it an ideal candidate for fixed tests.
pub struct NullLocaleBackend {}

impl NullLocaleBackend {
    pub fn new() -> Self {
        Self {}
    }
}

impl LocaleBackend for NullLocaleBackend {
    fn time_since_launch(&self) -> Duration {
        Duration::from_millis(0)
    }

    fn get_current_date_time(&self) -> DateTime<Utc> {
        self.get_timezone().ymd(2001, 2, 3).and_hms(4, 5, 6).into()
    }

    fn get_timezone(&self) -> FixedOffset {
        FixedOffset::east(20700)
    }
}

impl Default for NullLocaleBackend {
    fn default() -> Self {
        NullLocaleBackend::new()
    }
}
