use chrono::{DateTime, Local, TimeZone};

use nala::ports::wall_clock::WallClock;

/// Always returns the fixed instant it was built with, so tools that print
/// the current date/time can be asserted on exactly.
pub struct FakeWallClock {
    pub now: DateTime<Local>,
}

impl FakeWallClock {
    pub fn new(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> Self {
        let now = Local
            .with_ymd_and_hms(year, month, day, hour, minute, second)
            .single()
            .expect("fixed date/time should be unambiguous");

        Self { now }
    }
}

impl WallClock for FakeWallClock {
    fn now_local(&self) -> DateTime<Local> {
        self.now
    }
}
