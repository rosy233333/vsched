use alloc::collections::VecDeque;
use alloc::vec::Vec;

use base_task::{BaseTaskRef, TaskExtRef};
use std::sync::{Mutex, MutexGuard};

use crate::{BlockedReschedFuture, get_cpu_id};

// use crate::{CurrentTask, current_run_queue, select_run_queue};

/// A queue to store sleeping tasks.
///
/// # Examples
///
/// ```
/// use axtask::WaitQueue;
/// use core::sync::atomic::{AtomicU32, Ordering};
///
/// static VALUE: AtomicU32 = AtomicU32::new(0);
/// static WQ: WaitQueue = WaitQueue::new();
///
/// axtask::init_scheduler();
/// // spawn a new task that updates `VALUE` and notifies the main task
/// axtask::spawn(|| {
///     assert_eq!(VALUE.load(Ordering::Acquire), 0);
///     VALUE.fetch_add(1, Ordering::Release);
///     WQ.notify_one(true); // wake up the main task
/// });
///
/// WQ.wait(); // block until `notify()` is called
/// assert_eq!(VALUE.load(Ordering::Acquire), 1);
/// ```
pub struct WaitQueue {
    pub(crate) queue: Mutex<VecDeque<BaseTaskRef>>,
}

pub(crate) type WaitQueueGuard<'a> = MutexGuard<'a, VecDeque<BaseTaskRef>>;

impl WaitQueue {
    /// Creates an empty wait queue.
    pub const fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
        }
    }

    /// Creates an empty wait queue with space for at least `capacity` elements.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            queue: Mutex::new(VecDeque::with_capacity(capacity)),
        }
    }

    /// Cancel events by removing the task from the wait queue.
    /// If `from_timer_list` is true, try to remove the task from the timer list.
    fn cancel_events(&self, curr: &BaseTaskRef, _from_timer_list: bool) {
        // A task can be wake up only one events (timer or `notify()`), remove
        // the event from another queue.
        if curr.task_ext().in_wait_queue() {
            // wake up by timer (timeout).
            self.queue.lock().unwrap().retain(|t| !curr.ptr_eq(t));
            curr.task_ext().set_in_wait_queue(false);
        }

        // Try to cancel a timer event from timer lists.
        // Just mark task's current timer ticket ID as expired.
        #[cfg(feature = "irq")]
        if _from_timer_list {
            curr.task_ext().timer_ticket_expired();
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
        let wq = self.queue.lock().unwrap();
        let curr = vsched_apis::current(get_cpu_id());
        crate::vsched::blocked_resched(wq);
        self.cancel_events(&curr, false);
    }

    /// Blocks the current coroutine task and put it into the wait queue, until other task
    /// notifies it.
    pub async fn wait_f(&self) {
        let curr = vsched_apis::current(get_cpu_id());
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
        let curr = vsched_apis::current(get_cpu_id());
        loop {
            let wq = self.queue.lock().unwrap();
            if condition() {
                break;
            }
            crate::vsched::blocked_resched(wq);
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
        let curr = vsched_apis::current(get_cpu_id());
        loop {
            if condition() {
                break;
            }
            BlockedReschedFuture::new(self).await;
            // Preemption may occur here.
        }
        self.cancel_events(&curr, false);
    }

    // /// Blocks the current task and put it into the wait queue, until other tasks
    // /// notify it, or the given duration has elapsed.
    // #[cfg(feature = "irq")]
    // pub fn wait_timeout(&self, dur: core::time::Duration) -> bool {
    //     let mut rq = current_run_queue::<NoPreemptIrqSave>();
    //     let curr = crate::current();
    //     let deadline = axhal::time::wall_time() + dur;
    //     debug!(
    //         "task wait_timeout: {} deadline={:?}",
    //         curr.id_name(),
    //         deadline
    //     );
    //     crate::timers::set_alarm_wakeup(deadline, curr.clone());

    //     rq.blocked_resched(self.queue.lock());

    //     let timeout = curr.in_wait_queue(); // still in the wait queue, must have timed out

    //     // Always try to remove the task from the timer list.
    //     self.cancel_events(curr, true);
    //     timeout
    // }

    // /// Blocks the current coroutine task and put it into the wait queue, until other tasks
    // /// notify it, or the given duration has elapsed.
    // #[cfg(feature = "irq")]
    // pub async fn wait_timeout_f(&self, dur: core::time::Duration) -> bool {
    //     let rq = current_run_queue::<NoPreemptIrqSave>();
    //     let curr = crate::current();
    //     let deadline = axhal::time::wall_time() + dur;
    //     debug!(
    //         "task wait_timeout: {} deadline={:?}",
    //         curr.id_name(),
    //         deadline
    //     );
    //     crate::timers::set_alarm_wakeup(deadline, curr.clone());

    //     crate::run_queue::BlockedReschedFuture::new(rq, self).await;

    //     let timeout = curr.in_wait_queue(); // still in the wait queue, must have timed out

    //     // Always try to remove the task from the timer list.
    //     self.cancel_events(curr, true);
    //     timeout
    // }

    // /// Blocks the current task and put it into the wait queue, until the given
    // /// `condition` becomes true, or the given duration has elapsed.
    // ///
    // /// Note that even other tasks notify this task, it will not wake up until
    // /// the above conditions are met.
    // #[cfg(feature = "irq")]
    // pub fn wait_timeout_until<F>(&self, dur: core::time::Duration, condition: F) -> bool
    // where
    //     F: Fn() -> bool,
    // {
    //     let curr = crate::current();
    //     let deadline = axhal::time::wall_time() + dur;
    //     debug!(
    //         "task wait_timeout: {}, deadline={:?}",
    //         curr.id_name(),
    //         deadline
    //     );
    //     crate::timers::set_alarm_wakeup(deadline, curr.clone());

    //     let mut timeout = true;
    //     loop {
    //         let mut rq = current_run_queue::<NoPreemptIrqSave>();
    //         if axhal::time::wall_time() >= deadline {
    //             break;
    //         }
    //         let wq = self.queue.lock();
    //         if condition() {
    //             timeout = false;
    //             break;
    //         }

    //         rq.blocked_resched(wq);
    //         // Preemption may occur here.
    //     }
    //     // Always try to remove the task from the timer list.
    //     self.cancel_events(curr, true);
    //     timeout
    // }

    // /// Blocks the current coroutine task and put it into the wait queue, until the given
    // /// `condition` becomes true, or the given duration has elapsed.
    // ///
    // /// Note that even other tasks notify this task, it will not wake up until
    // /// the above conditions are met.
    // #[cfg(feature = "irq")]
    // pub async fn wait_timeout_until_f<F>(&self, dur: core::time::Duration, condition: F) -> bool
    // where
    //     F: Fn() -> bool,
    // {
    //     let curr = crate::current();
    //     let deadline = axhal::time::wall_time() + dur;
    //     debug!(
    //         "task wait_timeout: {}, deadline={:?}",
    //         curr.id_name(),
    //         deadline
    //     );
    //     crate::timers::set_alarm_wakeup(deadline, curr.clone());

    //     let mut timeout = true;
    //     loop {
    //         let rq = current_run_queue::<NoPreemptIrqSave>();
    //         if axhal::time::wall_time() >= deadline {
    //             break;
    //         }
    //         if condition() {
    //             timeout = false;
    //             break;
    //         }

    //         crate::run_queue::BlockedReschedFuture::new(rq, self).await;
    //         // Preemption may occur here.
    //     }
    //     // Always try to remove the task from the timer list.
    //     self.cancel_events(curr, true);
    //     timeout
    // }

    /// Wakes up one task in the wait queue, usually the first one.
    ///
    /// If `resched` is true, the current task will be preempted when the
    /// preemption is enabled.
    pub fn notify_one(&self, resched: bool) -> bool {
        let mut wq = self.queue.lock().unwrap();
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
    pub fn notify_task(&mut self, resched: bool, task: &BaseTaskRef) -> bool {
        let mut wq = self.queue.lock().unwrap();
        if let Some(index) = wq.iter().position(|t| BaseTaskRef::ptr_eq(t, task)) {
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
            let mut wq = self.queue.lock().unwrap();
            count = count.min(wq.len());
            wq.drain(..count).collect()
        };
        if !tasks.is_empty() {
            let mut wq = target.queue.lock().unwrap();
            wq.extend(tasks);
        }
        count
    }

    /// Returns the number of tasks in the wait queue.
    pub fn len(&self) -> usize {
        self.queue.lock().unwrap().len()
    }

    /// Returns true if the wait queue is empty.
    pub fn is_empty(&self) -> bool {
        self.queue.lock().unwrap().is_empty()
    }
}

fn unblock_one_task(task: BaseTaskRef, resched: bool) {
    // Mark task as not in wait queue.
    task.task_ext().set_in_wait_queue(false);
    log::debug!(
        "unblock task {:?}, is on cpu {}",
        task.task_ext().name(),
        task.on_cpu()
    );
    // Select run queue by the CPU set of the task.
    // Use `NoOp` kernel guard here because the function is called with holding the
    // lock of wait queue, where the irq and preemption are disabled.
    vsched_apis::unblock_task(task, resched, get_cpu_id());
}
