//! This mod specific the waker related with coroutine
//!

use super::task::{AsyncTaskRef, AsyncTask, wake_task};
use core::task::{RawWaker, RawWakerVTable, Waker};

const VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake, drop);

unsafe fn clone(p: *const ()) -> RawWaker {
    RawWaker::new(p, &VTABLE)
}

/// nop
unsafe fn wake(p: *const ()) { 
    wake_task(AsyncTaskRef::from_ptr(p as *const AsyncTask))
}

unsafe fn drop(_p: *const ()) {
    // nop
}

/// 
pub(crate) unsafe fn from_task(task_ref: AsyncTaskRef) -> Waker {
    Waker::from_raw(RawWaker::new(task_ref.as_task_raw_ptr() as _, &VTABLE))
}