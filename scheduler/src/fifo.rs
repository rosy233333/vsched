use crate::BaseScheduler;
use core::fmt::Debug;
use core::ops::Deref;
use core::ptr::NonNull;
use heapless::mpmc::MpMcQueue;

pub struct FifoTask<T> {
    inner: T,
}

impl<T> FifoTask<T> {
    /// Creates a new [`RRTask`] from the inner task struct.
    pub const fn new(inner: T) -> Self {
        Self { inner }
    }

    /// Returns a reference to the inner task struct.
    pub const fn inner(&self) -> &T {
        &self.inner
    }
}

impl<T> Deref for FifoTask<T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[repr(C)]
pub struct FiFoTaskRef<T> {
    inner: NonNull<FifoTask<T>>,
    clone_fn: Option<extern "C" fn(*const FifoTask<T>)>,
    weak_clone_fn: Option<extern "C" fn(*const FifoTask<T>) -> WeakFiFoTaskRef<T>>,
    drop_fn: Option<extern "C" fn(*const FifoTask<T>)>,
    strong_count_fn: Option<extern "C" fn(*const FifoTask<T>) -> usize>,
}

unsafe impl<T> Send for FiFoTaskRef<T> {}
unsafe impl<T> Sync for FiFoTaskRef<T> {}

impl<T> Clone for FiFoTaskRef<T> {
    fn clone(&self) -> Self {
        let ptr = self.inner.as_ptr();
        (self.clone_fn.unwrap())(ptr);
        Self {
            inner: self.inner.clone(),
            clone_fn: self.clone_fn.clone(),
            weak_clone_fn: self.weak_clone_fn.clone(),
            drop_fn: self.drop_fn.clone(),
            strong_count_fn: self.strong_count_fn.clone(),
        }
    }
}

impl<T> Drop for FiFoTaskRef<T> {
    fn drop(&mut self) {
        let ptr = self.inner.as_ptr();
        // unsafe { core::arch::asm!("mv t0, {ptr}", "ebreak", ptr = in(reg )ptr as usize) }
        (self.drop_fn.unwrap())(ptr);
    }
}

impl<T> FiFoTaskRef<T> {
    pub fn new(
        inner: NonNull<FifoTask<T>>,
        clone_fn: extern "C" fn(*const FifoTask<T>),
        weak_clone_fn: extern "C" fn(*const FifoTask<T>) -> WeakFiFoTaskRef<T>,
        drop_fn: extern "C" fn(*const FifoTask<T>),
        strong_count_fn: extern "C" fn(*const FifoTask<T>) -> usize,
    ) -> Self {
        Self {
            inner,
            clone_fn: Some(clone_fn),
            weak_clone_fn: Some(weak_clone_fn),
            drop_fn: Some(drop_fn),
            strong_count_fn: Some(strong_count_fn),
        }
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        self.inner.as_ptr() == other.inner.as_ptr()
    }

    pub fn strong_count(&self) -> usize {
        (self.strong_count_fn.unwrap())(self.inner.as_ptr())
    }

    pub fn weak_clone(&self) -> WeakFiFoTaskRef<T> {
        (self.weak_clone_fn.unwrap())(self.inner.as_ptr())
    }

    pub fn is_empty(&self) -> bool {
        self.inner == NonNull::dangling()
    }
}

impl<T> Deref for FiFoTaskRef<T> {
    type Target = FifoTask<T>;
    fn deref(&self) -> &Self::Target {
        unsafe { self.inner.as_ref() }
    }
}

impl<T: Debug> Debug for FifoTask<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FiFoTask")
            .field("inner", self.inner())
            .finish()
    }
}

impl<T: Debug> Debug for FiFoTaskRef<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FiFoTaskRef")
            .field("inner", unsafe { self.inner.as_ref() })
            .finish()
    }
}

#[repr(C)]
pub struct WeakFiFoTaskRef<T> {
    inner: NonNull<FifoTask<T>>,
}

impl<T> WeakFiFoTaskRef<T> {
    pub fn new(inner: NonNull<FifoTask<T>>) -> Self {
        Self { inner }
    }
}

impl<T> Deref for WeakFiFoTaskRef<T> {
    type Target = FifoTask<T>;
    fn deref(&self) -> &Self::Target {
        unsafe { self.inner.as_ref() }
    }
}

/// A simple FIFO (First-In-First-Out) cooperative scheduler.
///
/// When a task is added to the scheduler, it's placed at the end of the ready
/// queue. When picking the next task to run, the head of the ready queue is
/// taken.
///
/// As it's a cooperative scheduler, it does nothing when the timer tick occurs.
///
/// It internally uses a linked list as the ready queue.
pub struct FifoScheduler<T, const CAPACITY: usize> {
    ready_queue: MpMcQueue<FiFoTaskRef<T>, CAPACITY>,
}

impl<T, const CAPACITY: usize> FifoScheduler<T, CAPACITY> {
    /// Creates a new empty [`FifoScheduler`].
    pub const fn new() -> Self {
        Self {
            ready_queue: MpMcQueue::new(),
        }
    }
    /// get the name of scheduler
    pub fn scheduler_name() -> &'static str {
        "FIFO"
    }
}

impl<T, const CAPACITY: usize> BaseScheduler for FifoScheduler<T, CAPACITY> {
    type SchedItem = FiFoTaskRef<T>;

    fn init(&mut self) {}

    fn add_task(&self, task: Self::SchedItem) {
        let _ = self.ready_queue.enqueue(task);
    }

    fn pick_next_task(&self) -> Option<Self::SchedItem> {
        self.ready_queue.dequeue()
    }

    fn put_prev_task(&self, prev: Self::SchedItem, _preempt: bool) {
        let _ = self.ready_queue.enqueue(prev);
    }

    fn task_tick(&self, _current: &Self::SchedItem) -> bool {
        false // no reschedule
    }

    fn set_priority(&self, _task: &Self::SchedItem, _prio: isize) -> bool {
        false
    }
}
