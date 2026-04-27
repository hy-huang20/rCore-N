mod task;
mod timer;
mod time_driver;
mod queue;
mod waker;

use lazy_static::*;
use core::sync::atomic::Ordering;
use core::sync::atomic::AtomicBool;
use core::array;

/// os
use crate::config::CLOCK_FREQ;
use crate::timer::TICKS_PER_SEC;
use crate::task::hart_id;
use crate::task::suspend_current_and_run_next;
use crate::CPU_NUM;

/// async timer
use timer::Timer;

pub use task::poll as executor_poll;
pub use time_driver::on_interrupt as on_timer_interrupt;

pub fn after_ms(ms: usize, on_timeout: impl FnOnce() + Send + Sync + 'static) {
    task::spawn(async move {
        // Timer Future
        Timer::after_millis(ms).await;

        // call back
        on_timeout();
    });
}

pub fn after_ticks(ticks: usize, on_timeout: impl FnOnce() + Send + Sync + 'static) {
    task::spawn(async move {
        // Timer Future
        Timer::after(ticks).await;

        // call back
        on_timeout();
    });
}

/// ticks 是绝对时间
pub fn add_timer(ticks: usize, on_timeout: impl FnOnce() + Send + Sync + 'static) {
    let now = time_driver::now();
    let delay = ticks.saturating_sub(now);
    after_ticks(delay, on_timeout);
}

lazy_static! {
    pub static ref PENDING_OS_TICK: [AtomicBool; CPU_NUM] =
        array::from_fn(|_| AtomicBool::new(false));
}

/// 在 rust_main 中调用一次
/// 启动 os 时间片
pub fn start_os_tick() {
    after_ticks(CLOCK_FREQ / TICKS_PER_SEC, next_trigger);
}

fn handle_os_tick() {
    if PENDING_OS_TICK[hart_id()].swap(false, Ordering::Relaxed) {
        // 避免再 executor.poll() 中又执行 Executor::spawn()
        after_ticks(CLOCK_FREQ / TICKS_PER_SEC, next_trigger);
        // 避免在 executor.poll() 中途直接切换上下文
        suspend_current_and_run_next();
    }
}

fn next_trigger() {
    // 1. 这里有没有可能导致 executor poll 重入？
    // 为了避免重入，应不允许在 on_timeout 回调中调用 Executor::spawn()
    // async_timer::after_ticks(CLOCK_FREQ / TICKS_PER_SEC, next_trigger);
    
    // 2. 避免在这里直接 suspend_current_and_run_next() 切换上下文
    // 应该等到 executor.poll() 过程结束再紧接着切换上下文
    // suspend_current_and_run_next();

    // 解决方法：设置标志位，等 executor.poll() 结束后紧接着检查标志位
    PENDING_OS_TICK[hart_id()].store(true, Ordering::Relaxed);
}

// 在 os trap_handler 中调用
// 代替原有的 timer_interrupt_handler 实现
pub fn async_timer_interrupt_handler() {
    // 放弃 embassy-executor 的中断模式
    // 参考林晨 async-uart-driver 的设计模式
    // 将 wake 工作和 poll 工作放在同一个中断上下文中执行
    on_timer_interrupt();
    executor_poll();
    // executor.poll() 后处理 os 时间片相关
    handle_os_tick();
}
