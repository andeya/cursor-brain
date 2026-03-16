//! In-memory metrics for GET /v1/metrics (JSON). Counts requests and cursor call outcomes.

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub struct Metrics {
    pub requests_total: AtomicU64,
    pub cursor_calls_ok: AtomicU64,
    pub cursor_calls_fail: AtomicU64,
    pub cursor_calls_timeout: AtomicU64,
}

impl Metrics {
    pub fn inc_requests(&self) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_cursor_ok(&self) {
        self.cursor_calls_ok.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_cursor_fail(&self) {
        self.cursor_calls_fail.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_cursor_timeout(&self) {
        self.cursor_calls_timeout.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> (u64, u64, u64, u64) {
        (
            self.requests_total.load(Ordering::Relaxed),
            self.cursor_calls_ok.load(Ordering::Relaxed),
            self.cursor_calls_fail.load(Ordering::Relaxed),
            self.cursor_calls_timeout.load(Ordering::Relaxed),
        )
    }
}
