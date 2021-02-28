use std::time::Duration;
use chrono::{DateTime, FixedOffset, Local, Offset, Utc};
use web_sys::{window, Performance};
use ruffle_core::backend::locale::LocaleBackend;

pub struct WebLocaleBackend {
    performance: Performance,
    start_time: f64,
}

impl WebLocaleBackend {
    pub fn new() -> Self {
        let window = web_sys::window().expect("window()");
        let performance = window.performance().expect("window.performance()");

        Self {
            performance,
            start_time: performance.now(),
        }
    }
}

impl LocaleBackend for WebLocaleBackend {
    fn time_since_launch(&mut self) -> Duration {
        let dt = self.performance.now() - self.start_time;
        Duration::from_millis(dt as u64)
    }

    fn get_current_date_time(&self) -> DateTime<Utc> {
        Utc::now()
    }

    fn get_timezone(&self) -> FixedOffset {
        Local::now().offset().fix()
    }
}
