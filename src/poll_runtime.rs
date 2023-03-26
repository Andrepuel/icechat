use std::{
    future::Future,
    time::{Duration, Instant},
};
use tokio::runtime::Handle;

pub struct PollRuntime {
    last_run: Instant,
}
impl Default for PollRuntime {
    fn default() -> Self {
        Self::new()
    }
}
impl PollRuntime {
    pub fn new() -> Self {
        let last_run = Instant::now();

        Self { last_run }
    }

    pub fn poll<F: Future>(&mut self, runtime: Handle, future: F) -> Option<F::Output> {
        let now = Instant::now();
        if now
            .checked_duration_since(self.last_run)
            .unwrap_or_default()
            > Duration::from_millis(5)
        {
            self.last_run = now;
            Some(runtime.block_on(future))
        } else {
            None
        }
    }
}
