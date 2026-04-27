//! 参考 embassy-rp 中 time driver 的写法

use core::cell::{Cell, RefCell};
use core::{usize, array, default};
use alloc::collections::VecDeque;
use alloc::boxed::Box;
use alloc::sync::Arc;
use spin::{lazy, Mutex};
use xmas_elf::sections::{Rel, Rela};
use core::{convert::Infallible, pin::Pin};
use core::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize};
use core::sync::atomic::Ordering::Relaxed;
use core::future::Future;
use core::task::{Context, Poll, Waker};
use riscv::register::time;
use lazy_static::*;

use super::task::{Executor, TaskHeader};
use super::waker::from_task;
use super::queue::Queue;

use crate::async_timer::timer::Timer;
use crate::config::CPU_NUM;
use crate::task::hart_id;
use crate::sbi;

/// Time driver
pub trait Driver: Send + Sync + 'static {
    /// Return the current timestamp in ticks.
    fn now(&self) -> usize;

    /// Schedules a waker to be awoken at moment `at`.
    /// If this moment is in the past, the waker might be awoken immediately.
    fn schedule_wake(&self, at: usize, waker: &Waker);
}

struct AlarmState {
    timestamp: Cell<usize>,
}
unsafe impl Send for AlarmState {}

struct TimerDriver {
    /// last timestamp
    alarms: AlarmState,
    /// timer queue
    queue: RefCell<Queue>,
}

unsafe impl Send for TimerDriver {}
unsafe impl Sync for TimerDriver {}

lazy_static! {
    static ref DRIVER: [TimerDriver; CPU_NUM] = Default::default();
}

/// ticks
#[inline]
pub fn now() -> usize {
    <TimerDriver as Driver>::now(&DRIVER[hart_id()])
}

/// Schedule the given waker to be woken at `at`.
pub fn schedule_wake(at: usize, waker: &Waker) {
    <TimerDriver as Driver>::schedule_wake(&DRIVER[hart_id()], at, waker);
}

impl Driver for TimerDriver {
    fn now(&self) -> usize {
        time::read()
    }

    fn schedule_wake(&self, at: usize, waker: &Waker) {
        let mut queue = self.queue.borrow_mut();
        if queue.schedule_wake(at, waker) {
            let mut next = queue.next_expiration(self.now());
            while !self.set_alarm(next) {
                next = queue.next_expiration(self.now());
            }
        }
    }
}

impl Default for TimerDriver {
    fn default() -> Self {
        Self {
            alarms: AlarmState {
                timestamp: Cell::new(0),
            },
            queue: RefCell::new(Queue::new()),
        }
    }
}

impl TimerDriver {
    fn set_alarm(&self, timestamp: usize) -> bool {
        self.alarms.timestamp.set(timestamp);
        sbi::set_timer(timestamp);

        // 不建议在一开始就比较
        if timestamp <= self.now() {
            sbi::set_timer(usize::MAX);
            self.alarms.timestamp.set(usize::MAX);
            // 表示需要立刻处理
            false
        } else {
            true
        }
    }

    /// 中断到来时调用
    fn check_alarm(&self) {
        if self.alarms.timestamp.get() <= self.now() {
            self.trigger_alarm();
        }
    }

    fn trigger_alarm(&self) {
        let mut next = self.queue.borrow_mut().next_expiration(self.now());
        while !self.set_alarm(next) {
            next = self.queue.borrow_mut().next_expiration(self.now());
        }
    }
}

/// 中断到来时调用
pub fn on_interrupt() {
    DRIVER[hart_id()].check_alarm();
}