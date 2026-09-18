use vestrace_domain::time::Timestamp;

use crate::run::ports::RunClockPort;

pub struct SystemClock;

impl SystemClock {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl RunClockPort for SystemClock {
    fn now(&self) -> Timestamp {
        vestrace_domain::time::now()
    }
}
