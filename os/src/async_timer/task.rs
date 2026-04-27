use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use core::array;
use core::cell::UnsafeCell;
use core::future::Future;
use core::mem::ManuallyDrop;
use core::pin::Pin;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicU32, Ordering};
use core::task::{Context, Poll};
use lazy_static::lazy_static;
use riscv::register::sip;
use spin::Mutex;
use crossbeam::atomic::AtomicCell;

use super::waker::from_task;
use crate::config::CPU_NUM;
use crate::task::hart_id;

lazy_static! {
    static ref EXECUTORS: [Executor; CPU_NUM] = array::from_fn(|_| Executor::default());
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState {
    Ready = 1 << 0,
    Running = 1 << 1,
    Pending = 1 << 2,
}

/// The pointer of 'Task'
#[derive(Debug, Clone)]
pub struct TaskRef {
    ptr: NonNull<TaskHeader>,
}

unsafe impl Send for TaskRef {}
unsafe impl Sync for TaskRef {}

impl TaskRef {
    /// From a 'TaskRef' to a 'TaskHeader' raw_pointer
    pub fn as_task_raw_ptr(&self) -> *const TaskHeader {
        self.ptr.as_ptr()
    }

    /// From a 'TaskHeader' raw_pointer to a 'TaskRef'
    pub(crate) unsafe fn from_ptr(ptr: *const TaskHeader) -> Self {
        Self {
            ptr: NonNull::new(ptr as *mut TaskHeader).unwrap(),
        }
    }

    /// Polls the task once with the executor-managed waker.
    #[inline(always)]
    pub fn poll(self) -> Poll<()> {
        let waker = unsafe { from_task(self.clone()) };
        let mut cx = Context::from_waker(&waker);
        let task = TaskHeader::from_ref(self);

        task.state
            .store(TaskState::Running as u32, Ordering::Relaxed);

        let poll_result = unsafe { (&mut *task.future.as_ptr()).as_mut().poll(&mut cx) };

        match poll_result {
            Poll::Ready(()) => Poll::Ready(()),
            Poll::Pending => {
                // task.state.store(TaskState::Pending as u32, Ordering::Relaxed);
                // 这里不要直接将 task.state 设置为 Pending
                // 否则如果出现第一次 poll 就需要立刻处理的情况
                // 原本已被 wake_task() 修改为 Ready 的状态会被这里覆盖为 Pending
                // 从而丢失 wake
                task.state.compare_exchange(
                    TaskState::Running as u32, 
                    TaskState::Pending as u32,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                );
                Poll::Pending
            }
        }
    }
}

pub struct TaskHeader {
    /// State observed by the executor and timer wakeups.
    pub(crate) state: AtomicU32,
    /// The future is only polled by the executor on a single hart.
    future: AtomicCell<Pin<Box<dyn Future<Output = ()> + Send + Sync + 'static>>>,
}

unsafe impl Send for TaskHeader {}
unsafe impl Sync for TaskHeader {}

impl TaskHeader {
    /// Create a new Task 
    pub fn new(
        fut: Pin<Box<dyn Future<Output = ()> + 'static + Send + Sync>>,
    ) -> TaskRef {
        let task = Arc::new(Self{
            state: AtomicU32::new(TaskState::Ready as u32),
            future: AtomicCell::new(fut),
        });
        task.as_ref()
    }

    /// 
    pub fn as_ref(self: Arc<Self>) -> TaskRef {
        unsafe { TaskRef::from_ptr(Arc::into_raw(self))}
    }

    /// 
    pub fn from_ref(task_ref: TaskRef) -> Arc<Self> {
        let raw_ptr = task_ref.as_task_raw_ptr();
        unsafe { Arc::from_raw(raw_ptr) }
    }
}

// #[inline(always)]
// fn __pender() {
//     unsafe { sip::set_ssoft() }
// }

/// Wake a task through a `TaskRef`.
#[inline(always)]
pub fn wake_task(task_ref: TaskRef) {
    unsafe {
        let raw_ptr = task_ref.as_task_raw_ptr();
        let old = (*raw_ptr).state.swap(TaskState::Ready as u32, Ordering::Relaxed);
        // if old != TaskState::Ready as u32 {
        //     __pender();
        // }
    }
}

#[inline]
pub fn current_executor() -> &'static Executor {
    &EXECUTORS[hart_id()]
}

pub fn spawn<F>(fut: F) -> Arc<TaskHeader>
where
    F: Future<Output = ()> + Send + Sync + 'static,
{
    current_executor().spawn(Box::pin(fut))
}

/// Called from the software interrupt handler.
pub fn poll() -> bool {
    current_executor().poll()
}

#[derive(Default)]
pub struct Executor {
    tasks: Mutex<VecDeque<Arc<TaskHeader>>>,
}

impl Executor {
    pub fn is_empty(&self) -> bool {
        self.tasks.lock().is_empty()
    }

    pub fn push_task(&self, task: Arc<TaskHeader>) {
        self.tasks.lock().push_back(task);
    }

    pub fn spawn(
        &self,
        fut: Pin<Box<dyn Future<Output = ()> + Send + Sync + 'static>>,
    ) -> Arc<TaskHeader> {
        let task = TaskHeader::from_ref(TaskHeader::new(fut));
        self.push_task(task.clone());
        // __pender();
        // 这里执行 poll 有没有可能导致 executor poll 重入？
        self.poll();
        task
    }

    fn pop_runnable_task(&self) -> Option<Arc<TaskHeader>> {
        let mut tasks = self.tasks.lock();
        for _ in 0..tasks.len() {
            let task = tasks.pop_front().unwrap();
            let state = task.state.load(Ordering::Relaxed);
            if state == TaskState::Ready as u32 {
                return Some(task);
            }
            tasks.push_back(task);
        }
        None
    }

    /// Runs tasks until there is no ready task left.
    ///
    /// Returns `true` if there are still pending tasks waiting on a wakeup.
    pub fn poll(&self) -> bool {
        while let Some(task) = self.pop_runnable_task() {
            let task_ref = task.clone().as_ref();
            if task_ref.poll() == Poll::Pending {
                self.push_task(task);
            }
        }
        !self.is_empty()
    }
}
