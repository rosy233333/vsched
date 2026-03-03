//! 初始化vsched，并基于vsched实现任务相关操作：
//!
//! - 线程的让出（[`yield_now`]）、阻塞（[`blocked_resched`]）和退出（[`exit`]）
//! - 协程的让出（[`yield_now_f`]）、阻塞（[`BlockedReschedFuture`]）和退出（[`exit_f`]）
//!
//! 本模块在上述操作中负责的部分为：任务状态与调度器状态的维护、协程接口的Future包装。
//!
//! 本模块没有对外API。

use core::{
    mem::ManuallyDrop,
    pin::Pin,
    task::{Context, Poll},
};

use base_task::TaskState;
use config::AxCpuMask;

use crate::{
    interface::{get_cpu_id, main_task_exit},
    task::{self, run_idle},
    task_inner_ext::{arcext_to_base, base_to_ext},
    wait_queue::{WaitQueue, WaitQueueGuard},
};

pub(crate) fn init_vsched() {
    let main_task = task::new_init("main".into());
    main_task.set_cpumask(AxCpuMask::one_shot(get_cpu_id()));
    let idle_task = task::new(|| run_idle(), "idle".into(), config::TASK_STACK_SIZE);
    idle_task.set_cpumask(AxCpuMask::one_shot(get_cpu_id()));
    libvsched::init_vsched(
        get_cpu_id(),
        arcext_to_base(idle_task),
        arcext_to_base(main_task),
    );
}

pub(crate) fn init_vsched_secondary() {
    let idle_task = task::new_init("idle".into());
    idle_task.set_cpumask(AxCpuMask::one_shot(get_cpu_id()));
    libvsched::init_vsched(
        get_cpu_id(),
        arcext_to_base(idle_task.clone()),
        arcext_to_base(idle_task),
    );
}

pub(crate) fn blocked_resched(mut wq_guard: WaitQueueGuard) {
    let curr = unsafe { base_to_ext(libvsched::current(get_cpu_id())) };
    assert!(curr.is_running());
    assert!(!curr.is_idle());

    curr.set_state(base_task::TaskState::Blocked);
    curr.set_in_wait_queue(true);
    wq_guard.push_back(curr.clone());
    drop(wq_guard);

    log::debug!("task blocked {:?}", curr.name());
    // 所有任务的恢复点都需要释放上一个任务的Arc引用，并清除其on_cpu标志。
    //
    // 此处的`libvsched::resched`之后为任务的恢复点之一。
    libvsched::resched(get_cpu_id());
    let prev_task =
        unsafe { base_to_ext(libvsched::take_prev_task_and_clear_on_cpu(get_cpu_id())) };
    if prev_task.state() == TaskState::Exited {
        let _prev_task_to_drop = unsafe { ManuallyDrop::into_inner(prev_task.into_arc()) };
    }
}

pub(crate) fn exit(exit_code: i32) -> ! {
    let curr = unsafe { base_to_ext(libvsched::current(get_cpu_id())) };
    assert!(curr.is_running());
    assert!(!curr.is_idle());
    log::debug!("{:?} is exited", curr.name());
    if curr.is_init() {
        main_task_exit(exit_code) // 原有的代码是返回0而非exit_code，暂不清楚原因。
    } else {
        curr.set_state(base_task::TaskState::Exited);
        curr.notify_exit(exit_code);
    }

    libvsched::resched(get_cpu_id());
    unreachable!()
}

#[inline]
pub(crate) fn yield_now() {
    // 所有任务的恢复点都需要释放上一个任务的Arc引用，并清除其on_cpu标志。
    //
    // 此处的`libvsched::yield_now`之后为任务的恢复点之一。
    libvsched::yield_now(get_cpu_id());
    let prev_task =
        unsafe { base_to_ext(libvsched::take_prev_task_and_clear_on_cpu(get_cpu_id())) };
    if prev_task.state() == TaskState::Exited {
        let _prev_task_to_drop = unsafe { ManuallyDrop::into_inner(prev_task.into_arc()) };
    }
}

/// Current coroutine task gives up the CPU time voluntarily, and switches to another
/// ready task.
#[inline]
pub(crate) async fn yield_now_f() {
    YieldFuture::new().await;
}

/// The `YieldFuture` used when yielding the current task and reschedule.
/// When polling this future, the current task will be put into the run queue
/// with `Ready` state and reschedule to the next task on the run queue.
///
/// The polling operation is as the same as the
/// `current_run_queue::<NoPreemptIrqSave>().yield_current()` function.
///
/// SAFETY:
/// Due to this future is constructed with `current_run_queue::<NoPreemptIrqSave>()`,
/// the operation about manipulating the RunQueue and the switching to next task is
/// safe(The `IRQ` and `Preempt` are disabled).
pub(crate) struct YieldFuture {
    flag: bool,
}

impl YieldFuture {
    pub(crate) fn new() -> Self {
        Self { flag: false }
    }
}

impl Unpin for YieldFuture {}

impl Future for YieldFuture {
    type Output = ();
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Self { flag } = self.get_mut();
        if !(*flag) {
            *flag = !*flag;
            let curr = unsafe { base_to_ext(libvsched::current(get_cpu_id())) };
            log::trace!("task yield: {}", curr.id_name());
            assert!(curr.is_running());
            if libvsched::yield_f(get_cpu_id()) {
                Poll::Pending
            } else {
                Poll::Ready(())
            }
        } else {
            Poll::Ready(())
        }
    }
}

/// Due not manually release the `current_run_queue.state`,
/// otherwise it will cause double release.
impl Drop for YieldFuture {
    fn drop(&mut self) {}
}

/// Exits the current coroutine task.
pub(crate) async fn exit_f(exit_code: i32) {
    ExitFuture::new(exit_code).await;
}

/// The `ExitFuture` used when exiting the current task
/// with the specified exit code, which is always return `Poll::Pending`.
///
/// The polling operation is as the same as the
/// `current_run_queue::<NoPreemptIrqSave>().exit_current()` function.
///
/// SAFETY: as the same as the `YieldFuture`. However, It wrap the `CurrentRunQueueRef`
/// with `ManuallyDrop`, otherwise the `IRQ` and `Preempt` state of other
/// tasks(maybe `main` or `gc` task) which recycle the exited task(which used this future)
/// will be error due to automatically drop the `CurrentRunQueueRef.
/// The `CurrentRunQueueRef` should never be drop.
pub(crate) struct ExitFuture {
    exit_code: i32,
}

impl ExitFuture {
    pub(crate) fn new(exit_code: i32) -> Self {
        Self { exit_code }
    }
}

impl Unpin for ExitFuture {}

impl Future for ExitFuture {
    type Output = ();
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Self { exit_code } = self.get_mut();
        let exit_code = *exit_code;
        let curr = unsafe { base_to_ext(libvsched::current(get_cpu_id())) };
        log::debug!("task exit: {}, exit_code={}", curr.id_name(), exit_code);
        assert!(curr.is_running(), "task is not running: {:?}", curr.state());
        assert!(!curr.is_idle());
        curr.set_state(TaskState::Exited);

        // Notify the joiner task.
        curr.notify_exit(exit_code);

        assert!(libvsched::resched_f(get_cpu_id()));
        Poll::Pending
    }
}

/// The `BlockedReschedFuture` used when blocking the current task.
///
/// When polling this future, current task will be put into the wait queue and reschedule,
/// the state of current task will be marked as `Blocked`, set the `in_wait_queue` flag as true.
///
/// Note:
///
///     1. When polling this future, the wait queue is locked.
///     2. When polling this future, the current task is in the running state.
///     3. When polling this future, the current task is not the idle task.
///     4. The lock of the wait queue will be released explicitly after current task is pushed into it.
///
/// SAFETY:
/// as the same as the `YieldFuture`. Due to the `WaitQueueGuard` is not implemented
/// the `Send` trait, this future must hold the reference about the `WaitQueue` instead
/// of the `WaitQueueGuard`.
pub(crate) struct BlockedReschedFuture<'a> {
    wq: &'a WaitQueue,
    flag: bool,
}

impl<'a> BlockedReschedFuture<'a> {
    pub fn new(wq: &'a WaitQueue) -> Self {
        Self { wq, flag: false }
    }
}

impl<'a> Unpin for BlockedReschedFuture<'a> {}

impl<'a> Future for BlockedReschedFuture<'a> {
    type Output = ();
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Self { wq, flag } = self.get_mut();
        if !(*flag) {
            *flag = !*flag;
            let mut wq_guard = wq.queue.lock();
            let curr = unsafe { base_to_ext(libvsched::current(get_cpu_id())) };
            assert!(curr.is_running());
            assert!(!curr.is_idle());
            // we must not block current task with preemption disabled.
            // Current expected preempt count is 2.
            // 1 for `NoPreemptIrqSave`, 1 for wait queue's `SpinNoIrq`.
            #[cfg(feature = "preempt")]
            assert!(curr.can_preempt(2));

            // Mark the task as blocked, this has to be done before adding it to the wait queue
            // while holding the lock of the wait queue.
            curr.set_state(TaskState::Blocked);
            curr.set_in_wait_queue(true);

            wq_guard.push_back(curr.clone());
            // Drop the lock of wait queue explictly.
            drop(wq_guard);

            // Current task's state has been changed to `Blocked` and added to the wait queue.
            // Note that the state may have been set as `Ready` in `unblock_task()`,
            // see `unblock_task()` for details.

            log::debug!("task block: {}", curr.id_name());
            // assert!(libvsched::resched_f(get_cpu_id()));
            if libvsched::resched_f(get_cpu_id()) {
                // resched is needed
                Poll::Pending
            } else {
                // resched is not needed, which means the task has been unblocked and is ready to run.
                Poll::Ready(())
            }
            // Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}

impl<'a> Drop for BlockedReschedFuture<'a> {
    fn drop(&mut self) {}
}
