//! 阻塞队列的实现

use crate::{
    interface::get_cpu_id,
    sched::{BlockedReschedFuture, blocked_resched},
    task_inner_ext::{TaskRef, base_to_ext, ext_to_base},
};
use alloc::{collections::VecDeque, vec::Vec};
use kspin::{SpinNoIrq, SpinNoIrqGuard};

/// 阻塞队列，可用于阻塞线程或协程。
pub struct WaitQueue {
    /// 队列
    pub queue: SpinNoIrq<VecDeque<TaskRef>>,
}

/// 阻塞队列的锁保护引用。
pub type WaitQueueGuard<'a> = SpinNoIrqGuard<'a, VecDeque<TaskRef>>;

impl WaitQueue {
    /// Creates an empty wait queue.
    pub const fn new() -> Self {
        Self {
            queue: SpinNoIrq::new(VecDeque::new()),
        }
    }

    /// Creates an empty wait queue with space for at least `capacity` elements.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            queue: SpinNoIrq::new(VecDeque::with_capacity(capacity)),
        }
    }

    /// Cancel events by removing the task from the wait queue.
    /// If `from_timer_list` is true, try to remove the task from the timer list.
    fn cancel_events(&self, curr: &TaskRef, _from_timer_list: bool) {
        // A task can be wake up only one events (timer or `notify()`), remove
        // the event from another queue.
        if curr.in_wait_queue() {
            // wake up by timer (timeout).
            self.queue.lock().retain(|t| !curr.ptr_eq(t));
            curr.set_in_wait_queue(false);
        }

        // Try to cancel a timer event from timer lists.
        // Just mark task's current timer ticket ID as expired.
        #[cfg(feature = "irq")]
        if _from_timer_list {
            curr.timer_ticket_expired();
            // Note:
            //  this task is still not removed from timer list of target CPU,
            //  which may cause some redundant timer events because it still needs to
            //  go through the process of expiring an event from the timer list and invoking the callback.
            //  (it can be considered a lazy-removal strategy, it will be ignored when it is about to take effect.)
        }
    }

    /// Blocks the current task and put it into the wait queue, until other task
    /// notifies it.
    pub fn wait(&self) {
        let wq = self.queue.lock();
        let curr = unsafe { base_to_ext(libvsched::current(get_cpu_id())) };
        blocked_resched(wq);
        self.cancel_events(&curr, false);
    }

    /// Blocks the current coroutine task and put it into the wait queue, until other task
    /// notifies it.
    pub async fn wait_f(&self) {
        let curr = unsafe { base_to_ext(libvsched::current(get_cpu_id())) };
        BlockedReschedFuture::new(self).await;
        self.cancel_events(&curr, false);
    }

    /// Blocks the current task and put it into the wait queue, until the given
    /// `condition` becomes true.
    ///
    /// Note that even other tasks notify this task, it will not wake up until
    /// the condition becomes true.
    pub fn wait_until<F>(&self, condition: F)
    where
        F: Fn() -> bool,
    {
        let curr = unsafe { base_to_ext(libvsched::current(get_cpu_id())) };
        loop {
            let wq = self.queue.lock();
            if condition() {
                break;
            }
            blocked_resched(wq);
            // Preemption may occur here.
        }
        self.cancel_events(&curr, false);
    }

    /// Blocks the current coroutine task and put it into the wait queue, until the given
    /// `condition` becomes true.
    ///
    /// Note that even other tasks notify this task, it will not wake up until
    /// the condition becomes true.
    pub async fn wait_until_f<F>(&self, condition: F)
    where
        F: Fn() -> bool,
    {
        let curr = unsafe { base_to_ext(libvsched::current(get_cpu_id())) };
        loop {
            if condition() {
                break;
            }
            BlockedReschedFuture::new(self).await;
            // Preemption may occur here.
        }
        self.cancel_events(&curr, false);
    }

    /// Wakes up one task in the wait queue, usually the first one.
    ///
    /// If `resched` is true, the current task will be preempted when the
    /// preemption is enabled.
    pub fn notify_one(&self, resched: bool) -> bool {
        let mut wq = self.queue.lock();
        if let Some(task) = wq.pop_front() {
            unblock_one_task(task, resched);
            true
        } else {
            false
        }
    }

    /// Wakes all tasks in the wait queue.
    ///
    /// If `resched` is true, the current task will be preempted when the
    /// preemption is enabled.
    pub fn notify_all(&self, resched: bool) {
        while self.notify_one(resched) {
            // loop until the wait queue is empty
        }
    }

    /// Wake up the given task in the wait queue.
    ///
    /// If `resched` is true, the current task will be preempted when the
    /// preemption is enabled.
    pub fn notify_task(&mut self, resched: bool, task: &TaskRef) -> bool {
        let mut wq = self.queue.lock();
        if let Some(index) = wq.iter().position(|t| TaskRef::ptr_eq(t, task)) {
            unblock_one_task(wq.remove(index).unwrap(), resched);
            true
        } else {
            false
        }
    }

    /// Transfers up to `count` tasks from this wait queue to another wait queue.
    ///
    /// Note: If the current wait queue contains fewer than `count` tasks, all available tasks will be moved.
    ///
    /// ## Arguments
    /// * `count` - The maximum number of tasks to be moved.
    /// * `target` - The target wait queue to which tasks will be moved.
    ///
    /// ## Returns
    /// The number of tasks actually requeued.  
    pub fn requeue(&self, mut count: usize, target: &WaitQueue) -> usize {
        let tasks: Vec<_> = {
            let mut wq = self.queue.lock();
            count = count.min(wq.len());
            wq.drain(..count).collect()
        };
        if !tasks.is_empty() {
            let mut wq = target.queue.lock();
            wq.extend(tasks);
        }
        count
    }

    /// Returns the number of tasks in the wait queue.
    pub fn len(&self) -> usize {
        self.queue.lock().len()
    }

    /// Returns true if the wait queue is empty.
    pub fn is_empty(&self) -> bool {
        self.queue.lock().is_empty()
    }
}

fn unblock_one_task(task: TaskRef, resched: bool) {
    // Mark task as not in wait queue.
    task.set_in_wait_queue(false);
    log::debug!(
        "unblock task {:?}, is on cpu {}",
        task.name(),
        task.on_cpu()
    );
    // Select run queue by the CPU set of the task.
    // Use `NoOp` kernel guard here because the function is called with holding the
    // lock of wait queue, where the irq and preemption are disabled.
    libvsched::unblock_task(ext_to_base(task), resched, get_cpu_id(), get_cpu_id());
}
