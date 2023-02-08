use std::{
    future::Future,
    time::{Duration, Instant},
};
use tokio::runtime::Runtime;

pub struct PollRuntime {
    runtime: Runtime,
    last_run: Instant,
}
impl Default for PollRuntime {
    fn default() -> Self {
        Self::new()
    }
}
impl PollRuntime {
    pub fn new() -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let last_run = Instant::now();

        Self { runtime, last_run }
    }

    pub fn poll<F: Future>(&mut self, future: F) {
        let now = Instant::now();
        if now
            .checked_duration_since(self.last_run)
            .unwrap_or_default()
            > Duration::from_millis(5)
        {
            self.last_run = now;
            self.runtime.block_on(future);
        }
    }
}
