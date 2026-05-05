use core::sync::atomic::{AtomicBool, Ordering};
use alloc::sync::Arc;

use crate::async_timer;
use crate::task::{
    add_task, suspend_current_and_run_next, current_task, TaskStatus,
};

/// sleep syscall
pub fn sys_sleep(ms: usize) -> isize {
    // debug!("sys_sleep after {} ms", ms);
    let is_timeout_r = Arc::new(AtomicBool::new(false));
    let is_timeout_w = Arc::clone(&is_timeout_r);
    async_timer::after_ms(ms, move || {
        is_timeout_w.store(true, Ordering::Relaxed);
    });
    // 为了不在中断上下文 add_task() 获取 TASK_POOL 锁也只能这么妥协实现了
    while !is_timeout_r.load(Ordering::Relaxed) {
        suspend_current_and_run_next();
    }
    0
}
