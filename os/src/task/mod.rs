mod context;
mod manager;
mod pid;
mod pool;
mod processor;
mod switch;
mod task;

use crate::loader::get_app_data_by_name;
use alloc::{collections::vec_deque::VecDeque, sync::Arc};
use lazy_static::*;

use spin::{Mutex, lazy};
use switch::__switch2;

pub use context::TaskContext;
pub use pid::{find_task, pid_alloc, KernelStack, PidHandle};
pub use pool::{add_task, fetch_task, prioritize_task};
pub use processor::{
    current_task, current_trap_cx, current_user_token, hart_id, mmap, munmap, run_tasks, schedule,
    set_current_priority, take_current_task,
};
pub use task::{TaskControlBlock, TaskStatus};

lazy_static! {
    pub static ref WAIT_LOCK: Mutex<()> = Mutex::new(());
}

pub fn suspend_current_and_run_next() {
    // There must be an application running.
    let task = current_task().unwrap();
    let mut task_inner = task.acquire_inner_lock();
    task_inner.time_intr_count += 1;
    let task_cx_ptr = task_inner.get_task_cx_ptr();
    drop(task_inner);

    // jump to scheduling cycle
    schedule(task_cx_ptr);
}

pub fn exit_current_and_run_next(exit_code: i32) {
    // ++++++ hold initproc PCB lock here
    let mut initproc_inner = INITPROC.acquire_inner_lock();

    // take from Processor
    let task = take_current_task().unwrap();
    // **** hold current PCB lock
    let wl = WAIT_LOCK.lock();
    let mut inner = task.acquire_inner_lock();
    info!(
        "pid: {} exited with code {}, time intr: {}, cycle count: {}",
        task.pid.0, exit_code, inner.time_intr_count, inner.total_cpu_cycle_count
    );
    if let Some(trap_info) = &inner.user_trap_info {
        trap_info.remove_user_ext_int_map();
        use riscv::register::sie;
        unsafe {
            sie::clear_uext();
            sie::clear_usoft();
            sie::clear_utimer();
        }
    }

    // Change status to Zombie
    inner.task_status = TaskStatus::Zombie;
    // Record exit code
    inner.exit_code = exit_code;
    // do not move to its parent but under initproc

    for child in inner.children.iter() {
        child.acquire_inner_lock().parent = Some(Arc::downgrade(&INITPROC));
        initproc_inner.children.push(child.clone());
    }
    drop(initproc_inner);
    // ++++++ release parent PCB lock here

    inner.children.clear();
    // deallocate user space
    inner.memory_set.recycle_data_pages();
    drop(inner);
    // **** release current PCB lock
    // drop task manually to maintain rc correctly
    drop(task);
    drop(wl);
    // we do not have to save task context
    let mut _unused = Default::default();
    schedule(&mut _unused as *mut _);

    // let task = current_task().unwrap();
    // let task_inner = task.acquire_inner_lock();
    // if let Some(trap_info) = &task_inner.user_trap_info {
    //     trap_info.enable_user_ext_int();
    // }
}

lazy_static! {
    pub static ref INITPROC: Arc<TaskControlBlock> =
        TaskControlBlock::new(get_app_data_by_name("initproc").unwrap());
}

pub fn add_initproc() {
    debug!("add_initproc");
    add_task(INITPROC.clone());
}

// =================================

use core::sync::atomic::AtomicU32;
use core::future::Future;
use core::ptr::NonNull;
use core::task::{Context, Poll};
use core::pin::Pin;

use alloc::boxed::Box;

use crossbeam::atomic::AtomicCell;

use crate::timer::{AsyncTimer, AsyncTimerFuture};
use crate::waker::from_task;

/// 
#[repr(u32)]
pub enum AsyncTaskState {
    ///
    Ready = 1 << 0,
    ///
    Running = 1 << 1,
    ///
    Pending = 1 << 2,
}

/// The pointer of 'Task'
#[derive(Debug, Clone)]
pub struct AsyncTaskRef {
    ptr: NonNull<AsyncTask>,
}

unsafe impl Send for AsyncTaskRef {}
unsafe impl Sync for AsyncTaskRef {}

impl AsyncTaskRef {
    /// From a 'TaskRef' to a 'Task' raw_pointer
    pub fn as_task_raw_ptr(&self) -> *const AsyncTask {
        self.ptr.as_ptr()
    }

    /// From a 'Task' raw_pointer to a 'TaskRef'
    pub(crate) unsafe fn from_ptr(ptr: *const AsyncTask) -> Self {
        Self {
            ptr: NonNull::new(ptr as *mut AsyncTask).unwrap(),
        }
    }

    /// poll the task
    #[inline(always)]
    pub fn poll(self) -> Poll<()> {
        unsafe {
            let waker = from_task(self.clone());
            let mut cx: Context<'_> = Context::from_waker(&waker);
            let task = AsyncTask::from_ref(self);
            let future = &mut *task.fut.as_ptr();
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(res) => Poll::Ready(res),
                Poll::Pending => {
                    task.state.store(AsyncTaskState::Pending as u32, core::sync::atomic::Ordering::Relaxed);
                    task.driver.register_waker(waker);
                    Poll::Pending
                },
            }
        }
    }
}


pub struct AsyncTask {
    /// detail value shown in 'AsyncTaskRef'
    pub(crate) state: AtomicU32,
    /// The task future
    pub fut: AtomicCell<Pin<Box<dyn Future<Output = ()> + 'static + Send + Sync>>>,
    /// driver
    pub driver: Arc<AsyncTimer>,
}

impl AsyncTask {
    /// Create a new Task 
    pub fn new(
        fut: Pin<Box<dyn Future<Output = ()> + 'static + Send + Sync>>,
        driver: Arc<AsyncTimer>,
    ) -> AsyncTaskRef {
        let task = Arc::new(Self{
            state: AtomicU32::new(AsyncTaskState::Ready as u32),
            fut: AtomicCell::new(fut),
            driver,
        });
        task.as_ref()
    }

    /// 
    pub fn as_ref(self: Arc<Self>) -> AsyncTaskRef {
        unsafe { AsyncTaskRef::from_ptr(Arc::into_raw(self))}
    }

    /// 
    pub fn from_ref(task_ref: AsyncTaskRef) -> Arc<Self> {
        let raw_ptr = task_ref.as_task_raw_ptr();
        unsafe { Arc::from_raw(raw_ptr) }
    }
}

/// Wake a task by a 'AsyncTaskRef'
#[inline(always)]
pub fn wake_task(task_ref: AsyncTaskRef) {
    unsafe {
        // 修改 Task 状态，等到接收到串口中断时，执行器会执行里面现有的就绪 Future
        let raw_ptr = task_ref.as_task_raw_ptr();
        (*raw_ptr).state.store(AsyncTaskState::Ready as u32, core::sync::atomic::Ordering::Relaxed);
    }
}

#[derive(Default)]
pub struct AsyncTimerExecutor {
    tasks: Mutex<VecDeque<Arc<AsyncTask>>>,
}

impl AsyncTimerExecutor {
    pub fn is_empty(&self) -> bool {
        self.tasks.lock().is_empty()
    }

    pub fn push_task(&self, task: Arc<AsyncTask>) {
        self.tasks.lock().push_back(task);
    }

    pub fn pop_runnable_task(&self) -> Option<Arc<AsyncTask>> {
        let mut tasks = self.tasks.lock();
        for i in 0..tasks.len() {
            let task = tasks.pop_front().unwrap();
            let tstate = task.state.load(core::sync::atomic::Ordering::Relaxed);
            if tstate == AsyncTaskState::Ready as u32 {
                return Some(task)
            }
            tasks.push_back(task);
        }
        None
    }

    pub fn run_until_idle(&self) -> bool {
        while let Some(task) = self.pop_runnable_task() {
            task.state.store(AsyncTaskState::Pending as u32, core::sync::atomic::Ordering::Relaxed);
            let task_ref = task.clone().as_ref();
            if task_ref.poll() == Poll::Pending {
                self.push_task(task)
            }
        }
        !self.is_empty()
    }
}