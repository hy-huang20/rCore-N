use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use crate::async_timer::time_driver;
use crate::config::CLOCK_FREQ;
use crate::timer::MSEC_PER_SEC;

pub struct Timer {
    /// ticks
    expires_at: usize,
    yielded_once: bool,
}

impl Timer {
    pub fn at(expires_at: usize) -> Self {
        Self {
            expires_at, // ticks
            yielded_once: false,
        }
    }

    pub fn after(ticks: usize) -> Self {
        Self {
            expires_at: time_driver::now() + ticks, // ticks
            yielded_once: false,
        }
    }

    #[inline]
    pub fn after_millis(millis: usize) -> Self { // ms
        Self::after(millis * (CLOCK_FREQ / MSEC_PER_SEC))
    }
}

impl Unpin for Timer {}

impl Future for Timer {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // debug!("[TIMER] poll");
        if self.yielded_once && self.expires_at <= time_driver::now() {
            Poll::Ready(())
        } else {
            time_driver::schedule_wake(self.expires_at, cx.waker());
            self.yielded_once = true;
            Poll::Pending
        }
    }
}