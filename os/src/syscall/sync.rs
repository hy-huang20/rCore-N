use crate::async_timer;
use crate::task::{
    add_task, block_current_and_run_next, current_task, TaskStatus,
};

/// sleep syscall
pub fn sys_sleep(ms: usize) -> isize {
    // debug!("sys_sleep after {} ms", ms);
    let tcb = current_task().unwrap();
    async_timer::after_ms(ms, move || {
        let mut task_inner = tcb.acquire_inner_lock();
        task_inner.task_status = TaskStatus::Ready;
        drop(task_inner);
        add_task(tcb);
    });
    block_current_and_run_next();
    0
}
