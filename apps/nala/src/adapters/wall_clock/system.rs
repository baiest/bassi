use chrono::{DateTime, Local};

use crate::ports::wall_clock::WallClock;

pub struct SystemWallClock;

impl SystemWallClock {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SystemWallClock {
    fn default() -> Self {
        Self::new()
    }
}

impl WallClock for SystemWallClock {
    fn now_local(&self) -> DateTime<Local> {
        Local::now()
    }
}
