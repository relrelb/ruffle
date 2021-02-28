use std::time::{Duration, Instant};
use ruffle_core::backend::locale::LocaleBackend;
use ruffle_core::chrono::{DateTime, FixedOffset, Local, Offset, Utc};

pub struct DesktopLocaleBackend {
    /// The time that the SWF was launched.
    start_time: Instant,
}

impl DesktopLocaleBackend {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
        }
    }
}

impl LocaleBackend for DesktopLocaleBackend {
    fn time_since_launch(&self) -> Duration {
        self.start_time.elapsed()
    }

    fn get_current_date_time(&self) -> DateTime<Utc> {
        Utc::now()
    }

    fn get_timezone(&self) -> FixedOffset {
        Local::now().offset().fix()
    }
}
