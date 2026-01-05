// src/server/rate_limit.rs
#![cfg(windows)]

use std::time::{Duration, Instant};

const RL_MAX_ATTEMPTS: u32 = 5;
const RL_WINDOW: Duration = Duration::from_secs(30);
const RL_LOCKOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub(crate) struct RateLimitEntry {
    window_start: Instant,
    attempts: u32,
    lockout_until: Option<Instant>,
}

impl RateLimitEntry {
    pub(crate) fn new() -> Self {
        Self {
            window_start: Instant::now(),
            attempts: 0,
            lockout_until: None,
        }
    }

    pub(crate) fn is_locked(&self) -> bool {
        match self.lockout_until {
            Some(t) => Instant::now() < t,
            None => false,
        }
    }

    pub(crate) fn remaining_lockout_secs(&self) -> u64 {
        if let Some(t) = self.lockout_until {
            if Instant::now() >= t {
                0
            } else {
                (t - Instant::now()).as_secs()
            }
        } else {
            0
        }
    }

    pub(crate) fn register_failure(&mut self) {
        // reset window if expired
        if self.window_start.elapsed() > RL_WINDOW {
            self.window_start = Instant::now();
            self.attempts = 0;
        }

        self.attempts += 1;

        if self.attempts >= RL_MAX_ATTEMPTS {
            self.lockout_until = Some(Instant::now() + RL_LOCKOUT);
        }
    }

    pub(crate) fn register_success(&mut self) {
        self.window_start = Instant::now();
        self.attempts = 0;
        self.lockout_until = None;
    }
}
