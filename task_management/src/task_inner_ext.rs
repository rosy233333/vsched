extern crate alloc;

use crate::wait_queue::WaitQueue;
use alloc::{boxed::Box, format, string::String, sync::Arc};
use base_task::{TaskStack, TaskState};
use config::{AxCpuMask, SMP};
use core::{
    cell::UnsafeCell,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicI32, Ordering},
};
use crossbeam::atomic::AtomicCell;
use log::debug;

pub type AxTask = scheduler::BaseTask<TaskInner>;
pub type TaskRef = scheduler::BaseTaskRef<TaskInner>;

/// 在vdso外部，inner和ext均为可见
///
/// 在vdso内部，仅有inner可见
///
/// 该设计使vdso调度器可以兼容具有不同类型ext的任务
#[repr(C)]
pub struct TaskInner {
    inner: base_task::TaskInner,
    ext: TaskInnerExt,
}

#[repr(C)]
pub struct TaskInnerExt {
    name: String,
    exit_code: AtomicI32,
    wait_for_exit: WaitQueue,
    entry: Option<*mut dyn FnOnce()>,
    /// CPU affinity mask.
    cpumask: AtomicCell<AxCpuMask>,
    // #[cfg(feature = "tls")]
    // tls: TlsArea,
    /// The future of coroutine task.
    future: UnsafeCell<Option<core::pin::Pin<Box<dyn Future<Output = ()> + Send + 'static>>>>,
}

impl TaskInnerExt {
    fn new_common(name: String) -> Self {
        Self {
            name,
            exit_code: AtomicI32::new(0),
            wait_for_exit: WaitQueue::new(),
            entry: None,
            cpumask: AtomicCell::new(AxCpuMask::full()),
            // #[cfg(feature = "tls")]
            // tls: TlsArea,
            future: UnsafeCell::new(None),
        }
    }
}

unsafe impl Send for TaskInner {}
unsafe impl Sync for TaskInner {}

/// 初始化
impl TaskInner {
    fn new_common(name: String) -> Self {
        Self {
            inner: base_task::TaskInner::new_common(),
            ext: TaskInnerExt::new_common(name),
        }
    }

    /// Creates an "init task" using the current CPU states, to use as the
    /// current task.
    ///
    /// As it is the current task, no other task can switch to it until it
    /// switches out.
    ///
    /// And there is no need to set the `entry`, `kstack` or `tls` fields, as
    /// they will be filled automatically when the task is switches out.
    pub fn new_init(name: String) -> Self {
        Self {
            inner: base_task::TaskInner::new_init(name == "idle"),
            ext: TaskInnerExt::new_common(name),
        }
    }

    /// Create a new task with the given entry function and stack size.
    ///
    /// - entry: 用户想要创建的任务函数
    /// - task_entry: 任务真正的入口点，通常包含初始化、调用entry和清理等逻辑
    pub fn new<F>(entry: F, task_entry: usize, name: String, stack_size: usize) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        let mut t = Self {
            inner: base_task::TaskInner::new(task_entry, name == "idle", stack_size),
            ext: TaskInnerExt::new_common(name),
        };
        debug!("new task: {}", t.id_name());

        t.ext.entry = Some(Box::into_raw(Box::new(entry)));
        t
    }

    pub fn new_f<F>(
        future: F,
        name: String,
        alloc_stack: fn() -> TaskStack,
        coroutine_schedule: fn(),
    ) -> Self
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let mut t = Self {
            inner: base_task::TaskInner::new_common(),
            ext: TaskInnerExt::new_common(name),
        };
        debug!("new coroutine task: {}", t.id_name());
        t.ext.future = UnsafeCell::new(Some(Box::pin(future)));
        t.set_alloc_stack_fn(alloc_stack as usize);
        t.set_coroutine_schedule(coroutine_schedule as usize);
        t
    }
}

// 使用Deref和DerefMut获取inner的字段
impl Deref for TaskInner {
    type Target = base_task::TaskInner;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl DerefMut for TaskInner {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// 获取ext中的字段
impl TaskInner {
    /// Gets the name of the task.
    pub fn name(&self) -> &str {
        self.ext.name.as_str()
    }

    pub fn id_name(&self) -> String {
        format!("Task({}, {:?})", self.inner.id().as_u64(), self.ext.name)
    }

    /// Gets the entry of the task.
    pub const fn entry(&self) -> &Option<*mut dyn FnOnce()> {
        &self.ext.entry
    }

    /// Gets the future of the task.
    pub const fn future(
        &self,
    ) -> &mut Option<core::pin::Pin<Box<dyn Future<Output = ()> + Send + 'static>>> {
        unsafe { &mut *(self.ext.future.get()) }
    }

    /// Gets the cpu affinity mask of the task.
    ///
    /// Returns the cpu affinity mask of the task in type [`AxCpuMask`].
    #[inline]
    pub fn cpumask(&self) -> AxCpuMask {
        self.ext.cpumask.load()
    }

    /// Sets the cpu affinity mask of the task.
    ///
    /// # Arguments
    /// `cpumask` - The cpu affinity mask to be set in type [`AxCpuMask`].
    #[inline]
    pub fn set_cpumask(&self, cpumask: AxCpuMask) {
        self.ext.cpumask.store(cpumask);
    }

    #[inline]
    pub fn select_run_queue_index(&self) -> usize {
        use core::sync::atomic::{AtomicUsize, Ordering};
        static RUN_QUEUE_INDEX: AtomicUsize = AtomicUsize::new(0);

        let cpumask = self.cpumask();
        assert!(!cpumask.is_empty(), "No available CPU for task execution");

        // Round-robin selection of the run queue index.
        loop {
            let index = unsafe { RUN_QUEUE_INDEX.fetch_add(1, Ordering::SeqCst) % SMP };
            if cpumask.get(index) {
                return index;
            }
        }
    }

    #[inline]
    pub fn exit_code(&self) -> i32 {
        self.ext.exit_code.load(Ordering::Acquire)
    }

    #[inline]
    pub fn set_exit_code(&self, exit_code: i32) {
        self.ext.exit_code.store(exit_code, Ordering::Release);
    }

    #[inline]
    pub fn wait_queue(&self) -> &WaitQueue {
        &self.ext.wait_for_exit
    }

    /// Notify all tasks that join on this task.
    pub fn notify_exit(&self, exit_code: i32) {
        self.ext.exit_code.store(exit_code, Ordering::Release);
        self.ext.wait_for_exit.notify_all(false);
    }

    pub fn join(&self) -> Option<i32> {
        self.ext
            .wait_for_exit
            .wait_until(|| self.inner.state() == TaskState::Exited);
        Some(self.ext.exit_code.load(Ordering::Acquire))
    }

    pub async fn join_f(&self) -> Option<i32> {
        self.ext
            .wait_for_exit
            .wait_until_f(|| self.inner.state() == TaskState::Exited)
            .await;
        Some(self.ext.exit_code.load(Ordering::Acquire))
    }
}

impl Drop for TaskInner {
    fn drop(&mut self) {
        debug!("drop task: {}", self.id_name());
    }
}

/// 用于将从调度器中获得的`base_task::TaskRef`转化为`TaskRef`引用（从而可访问ext字段）
///
/// 因为两种`TaskRef`内部都以指针方式存储，且除`ext`以外两种任务数据结构相同，因此可以直接使用`core::mem::transmute`转化
///
/// SAFETY: 目前，需要保证所有调度器中的TaskRef全部由该库提供（即使用了该库的ext字段）
#[inline]
pub unsafe fn base_to_ext(base_ref: base_task::TaskRef) -> TaskRef {
    unsafe { core::mem::transmute(base_ref) }
}

/// 用于将本库的`TaskRef`转化为调度器使用的`TaskRef`
///
/// 该转化会导致`ext`字段不可访问，直到使用`base_to_exted`转化回来
///
/// 因为两种`TaskRef`内部都以指针方式存储，且除`ext`以外两种任务数据结构相同，因此可以直接使用`core::mem::transmute`转化
#[inline]
pub fn ext_to_base(ext_ref: TaskRef) -> base_task::TaskRef {
    unsafe { core::mem::transmute(ext_ref) }
}

pub type ArcTaskRef = Arc<AxTask>;

#[inline]
pub unsafe fn base_to_arcext(base_ref: base_task::TaskRef) -> ArcTaskRef {
    ManuallyDrop::into_inner(unsafe { base_to_ext(base_ref) }.into_arc())
}

#[inline]
pub fn arcext_to_base(ext_ref: ArcTaskRef) -> base_task::TaskRef {
    let ext = TaskRef::new(Arc::into_raw(ext_ref));
    ext_to_base(ext)
}
