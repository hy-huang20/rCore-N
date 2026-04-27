//! queue generic

use alloc::vec::Vec;
use core::cmp::{min, Ordering};
use core::task::Waker;

use super::task::TaskRef;
use super::waker;

use crate::config::CPU_NUM;

#[derive(Debug)]
struct Timer {
    at: usize,
    waker: Waker,
}

impl PartialEq for Timer {
    fn eq(&self, other: &Self) -> bool {
        self.at == other.at
    }
}

impl Eq for Timer {}

impl PartialOrd for Timer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.at.partial_cmp(&other.at)
    }
}

impl Ord for Timer {
    fn cmp(&self, other: &Self) -> Ordering {
        self.at.cmp(&other.at)
    }
}

pub struct GenericQueue {
    queue: Vec<Timer>,
}

impl GenericQueue {
    /// Creates a new timer queue.
    pub const fn new() -> Self {
        Self { queue: Vec::new() }
    }

    /// Schedules a task to run at a specific time, and returns whether any changes were made.
    ///
    /// If this function returns `true`, the called should find the next expiration time and set
    /// a new alarm for that time.
    pub fn schedule_wake(&mut self, at: usize, waker: &Waker) -> bool {
        self.queue
            .iter_mut()
            .find(|timer| timer.waker.will_wake(waker))
            .map(|timer| {
                if timer.at > at {
                    timer.at = at;
                    true
                } else {
                    false
                }
            })
            .unwrap_or_else(|| {
                let mut timer = Timer {
                    waker: waker.clone(),
                    at,
                };

                self.queue.push(timer);

                true
            })
    }

    /// Dequeues expired timers and returns the next alarm time.
    pub fn next_expiration(&mut self, now: usize) -> usize {
        let mut next_alarm = usize::MAX;

        let mut i = 0;
        while i < self.queue.len() {
            let timer = &self.queue[i];
            if timer.at <= now {
                let timer = self.queue.swap_remove(i);
                timer.waker.wake();
            } else {
                next_alarm = min(next_alarm, timer.at);
                i += 1;
            }
        }

        next_alarm
    }
}


pub struct Queue {
    queue: GenericQueue,
}

impl Queue {
    /// Creates a new timer queue.
    pub fn new() -> Self {
        Self {
            queue: GenericQueue::new(),
        }
    }

    /// Schedules a task to run at a specific time, and returns whether any changes were made.
    ///
    /// If this function returns `true`, the called should find the next expiration time and set
    /// a new alarm for that time.
    pub fn schedule_wake(&mut self, at: usize, waker: &Waker) -> bool {
        self.queue.schedule_wake(at, waker)
    }

    /// Dequeues expired timers and returns the next alarm time.
    pub fn next_expiration(&mut self, now: usize) -> usize {
        self.queue.next_expiration(now)
    }
}