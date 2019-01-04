use std::time::SystemTime;
use std::time::Duration;
use hlt::log::Log;

pub struct Stats {
    start: SystemTime,
    runtime: Duration,
    count: u32,
    max: (Duration, u32),
}

impl Stats {
    pub fn new() -> Stats {
        Stats {
            start: SystemTime::now(),
            runtime: Duration::default(),
            count: 0,
            max: (Duration::default(), 0),
        }
    }

    pub fn start(&mut self) {
        self.start = SystemTime::now();
    }

    pub fn end(&mut self) {
        let duration = SystemTime::now().duration_since(self.start).expect("Time goes forwards");

        self.runtime += duration;
        self.max = self.max.max((duration, self.count));

        let mean = self.runtime / (self.count + 1);

        Log::info(format!("Time: {:?}, mean: {:?}, max: {:?}, total: {:?}", duration, mean, self.max, self.runtime));

        self.count += 1;
    }

}
