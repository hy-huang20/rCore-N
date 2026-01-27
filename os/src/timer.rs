use crate::config::{CLOCK_FREQ, CPU_NUM};
use crate::sbi::set_timer;
use crate::task::hart_id;
use alloc::{collections::BTreeMap, sync::Arc, vec::Vec};
use crossbeam::epoch::Atomic;
use lazy_static::*;
use riscv::register::time;
use spin::Mutex;

const TICKS_PER_SEC: usize = 100;
const MSEC_PER_SEC: usize = 1000;
pub const USEC_PER_SEC: usize = 1_000_000;

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

#[allow(dead_code)]
impl TimeVal {
    pub fn new() -> Self {
        TimeVal { sec: 0, usec: 0 }
    }
}

#[allow(unused_variables)]
pub fn get_time(mut ts: Vec<*mut usize>, tz: usize) -> isize {
    let t = time::read();
    unsafe {
        *ts[0] = t / CLOCK_FREQ;
        *ts[1] = (t % CLOCK_FREQ) * 1000000 / CLOCK_FREQ;
        trace!("t {} sec {} usec {}", t, *ts[0], *ts[1]);
    }

    0
}

#[allow(dead_code)]
pub fn get_time_ms() -> usize {
    time::read() / (CLOCK_FREQ / MSEC_PER_SEC)
}

#[allow(dead_code)]
pub fn get_time_us() -> usize {
    time::read() * USEC_PER_SEC / CLOCK_FREQ
}

pub fn set_next_trigger() {
    // set_timer(time::read() + CLOCK_FREQ / TICKS_PER_SEC);
    set_virtual_timer(time::read() + CLOCK_FREQ / TICKS_PER_SEC, 0);
}

lazy_static! {
    pub static ref TIMER_MAP: [Arc<Mutex<BTreeMap<usize, usize>>>; CPU_NUM] = Default::default();
    pub static ref ASYNC_TIMER: Mutex<Arc<AsyncTimer>> = Mutex::new(Arc::new(AsyncTimer::new()));
}

pub static DEBUG_ONCE: AtomicBool = AtomicBool::new(false);

pub fn set_virtual_timer(mut time: usize, pid: usize) {
    if time < time::read() {
        warn!("Time travel!");
        // return;
    }
    if !DEBUG_ONCE.load(core::sync::atomic::Ordering::Relaxed) {
        debug!("set_virtual_timer");
    }
    let mut timer_map = TIMER_MAP[hart_id()].lock();
    while timer_map.contains_key(&time) {
        time += 1;
    }
    timer_map.insert(time, pid);
    if let Some((timer_min, _)) = timer_map.first_key_value() {
        if time == *timer_min {
            set_timer(time);

            let async_timer = ASYNC_TIMER.lock().clone();
            async_timer.set_async_timer(time);
        }
    }
}

// ==========================================

use alloc::collections::VecDeque;
use alloc::boxed::Box;
use xmas_elf::sections::{Rel, Rela};
use core::{convert::Infallible, pin::Pin};
use core::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicUsize};
use core::sync::atomic::Ordering::Relaxed;
use core::future::Future;
use core::task::{Context, Poll, Waker};

use crate::task::{AsyncTimerExecutor, AsyncTask};
use crate::waker::from_task;

pub struct AsyncTimer {
    last_time: AtomicU32,
    wakers: Mutex<VecDeque<Waker>>,
    executor: AsyncTimerExecutor,
}

impl AsyncTimer {
    pub fn new() -> Self {
        AsyncTimer { 
            last_time: AtomicU32::new(0),
            wakers: Mutex::new(VecDeque::new()), 
            executor: AsyncTimerExecutor::default(), 
        }
    }

    /// 在 os 的 trap_handler 中调用
    pub fn interrupt_handler(&self, new_time: usize) {
        if !DEBUG_ONCE.load(core::sync::atomic::Ordering::Relaxed) {
            debug!("AsyncTimer::interrupt_handler");
        }
        self.last_time.store(new_time as u32, Relaxed);
        while let Some(waker) = self.wakers.lock().pop_front(){
            waker.wake();
        };
        // poll future
        self.executor.run_until_idle();
    }

    pub fn set_async_timer(self: Arc<Self>, time: usize) {
        if !DEBUG_ONCE.load(core::sync::atomic::Ordering::Relaxed) {
            debug!("AsyncTimer::set_async_timer");
        }
        let timer_future = AsyncTimerFuture {
            time,
            driver: self.clone(),
        };
        let task = AsyncTask::new(Box::pin(timer_future), self.clone());
        self.register_waker(unsafe {
            from_task(task.clone())
        });
        self.executor.push_task(AsyncTask::from_ref(task));
    }

    pub fn register_waker(&self, waker: Waker) {
        self.wakers.lock().push_back(waker)
    }

    pub fn try_get_async_timer(&self) -> Option<usize> {
        Some(self.last_time.load(Relaxed) as usize)
    }
}

pub struct AsyncTimerFuture {
    time: usize,
    driver: Arc<AsyncTimer>,
}

unsafe impl Send for AsyncTimerFuture {}
unsafe impl Sync for AsyncTimerFuture {}

impl Future for AsyncTimerFuture {
    type Output = ();
    fn poll(self: core::pin::Pin<&mut Self>, cx: &mut core::task::Context<'_>) -> core::task::Poll<Self::Output> {
        if !DEBUG_ONCE.load(core::sync::atomic::Ordering::Relaxed) {
            debug!("AsyncTimerFuture::poll");
            // 通过输出查看每次 poll 时 sp 是否变化
            let current_sp: usize;
            unsafe {
                use core::arch::asm;
                asm!("mv {}, sp", out(reg) current_sp);
            }
            debug!("future poll, sp: {:#x}", current_sp);
        }
        if let Some(cur_time) = self.driver.try_get_async_timer() {
            if cur_time >= self.time {
                return Poll::Ready(());
            }
        }
        Poll::Pending
    }
}