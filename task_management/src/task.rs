use core::{
    array,
    mem::{ManuallyDrop, MaybeUninit},
    ops::Deref,
};

use crate::{
    interface::get_cpu_id,
    sched::{exit_f, yield_now},
    task_inner_ext::{ArcTaskRef, AxTask, TaskInner, TaskRef, base_to_ext},
    wait_queue::WaitQueue,
};
use alloc::{boxed::Box, collections::vec_deque::VecDeque, string::String, sync::Arc, vec::Vec};
use base_task::{TaskStack, TaskState};
use config::SMP;
use kspin::SpinNoIrq;

pub(crate) fn new<F>(entry: F, name: String, stack_size: usize) -> ArcTaskRef
where
    F: FnOnce() + Send + 'static,
{
    let t = TaskInner::new(entry, task_entry as usize, name, stack_size);
    Arc::new(AxTask::new(t))
}

pub(crate) fn new_f<F>(future: F, name: String) -> ArcTaskRef
where
    F: Future<Output = ()> + Send + 'static,
{
    let t = TaskInner::new_f(
        async move {
            future.await;
            exit_f(0).await;
        },
        name.clone(),
        alloc_stack_for_coroutine,
        coroutine_schedule,
    );
    Arc::new(AxTask::new(t))
}

pub(crate) fn new_init(name: String) -> ArcTaskRef {
    let t = TaskInner::new_init(name.clone());
    t.set_state(TaskState::Running);
    Arc::new(AxTask::new(t))
}

// pub(crate) fn new_gc(name: String, stack_size: usize) -> ArcTaskRef {
//     let t = TaskInner::new(gc_entry, task_entry as usize, name, stack_size);
//     Arc::new(AxTask::new(t))
// }

// pub(crate) static EXITED_TASKS: SpinNoIrq<VecDeque<TaskRef>> = SpinNoIrq::new(VecDeque::new());
// pub(crate) static WAIT_FOR_EXIT: WaitQueue = WaitQueue::new();

// pub(crate) fn gc_entry() {
//     loop {
//         let mut exited_tasks = EXITED_TASKS.lock();
//         let n = exited_tasks.len();
//         for _ in 0..n {
//             if let Some(task) = exited_tasks.pop_front() {
//                 let task_arc = ManuallyDrop::into_inner(task.into_arc());
//                 if Arc::strong_count(&task_arc) > 1 {
//                     exited_tasks.push_back(task);
//                 } else {
//                     drop(task_arc);
//                 }
//             }
//         }
//         drop(exited_tasks);
//         WAIT_FOR_EXIT.wait();
//     }
// }

pub fn run_idle() {
    loop {
        yield_now();
    }
}

extern "C" fn task_entry() {
    // clear prev task's on cpu flag and drop it if it is exited。
    let prev_task =
        unsafe { base_to_ext(libvsched::take_prev_task_and_clear_on_cpu(get_cpu_id())) };
    if prev_task.state() == TaskState::Exited {
        let _prev_task_to_drop = unsafe { ManuallyDrop::into_inner(prev_task.into_arc()) };
    }
    drop(prev_task);
    let task = unsafe { base_to_ext(libvsched::current(get_cpu_id())) };
    if let Some(entry) = task.entry() {
        unsafe { Box::from_raw(*entry)() };
    }
    crate::sched::exit(0);
}

struct PerCPUStackPool([SpinNoIrq<Vec<TaskStack>>; SMP]);

impl PerCPUStackPool {
    pub(crate) const fn new() -> Self {
        Self([const { SpinNoIrq::new(Vec::new()) }; SMP])
    }
}

unsafe impl Send for PerCPUStackPool {}
unsafe impl Sync for PerCPUStackPool {}

impl Deref for PerCPUStackPool {
    type Target = SpinNoIrq<Vec<TaskStack>>;

    fn deref(&self) -> &Self::Target {
        &self.0[get_cpu_id()]
    }
}

static COROUTINE_STACK_POOL: PerCPUStackPool = PerCPUStackPool::new();

/// Alloc a stack for running a coroutine.
/// If the `COROUTINE_STACK_POOL` is empty,
/// it will alloc a new stack on the allocator.
fn alloc_stack_for_coroutine() -> TaskStack {
    log::debug!("alloc stack");
    COROUTINE_STACK_POOL
        .lock()
        .pop()
        .unwrap_or_else(|| TaskStack::alloc(config::TASK_STACK_SIZE))
}

/// Recycle the stack after the coroutine running to a certain stage.
fn recycle_stack_of_coroutine(stack: TaskStack) {
    log::debug!("recycle stack");
    COROUTINE_STACK_POOL.lock().push(stack)
}

fn coroutine_schedule() {
    use core::task::{Context, Waker};
    loop {
        // clear prev task's on cpu flag and drop it if it is exited。
        let prev_task =
            unsafe { base_to_ext(libvsched::take_prev_task_and_clear_on_cpu(get_cpu_id())) };
        if prev_task.state() == TaskState::Exited {
            let _prev_task_to_drop = unsafe { ManuallyDrop::into_inner(prev_task.into_arc()) };
        }
        drop(prev_task);
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        let curr = unsafe { base_to_ext(libvsched::current(get_cpu_id())) };

        let fut = curr
            .inner()
            .future()
            .as_mut()
            .expect("The task should be a coroutine");
        let _res = fut.as_mut().poll(&mut cx);
        // 该行要求协程在返回Pending或完全结束时，都需要将state从Running切换到其它状态（Ready, Blocked, Exited）。
        assert!(!curr.is_running(), "{} is still running", curr.id_name());
        let prev_task = curr;
        let stack = unsafe { &mut *prev_task.kernel_stack() }
            .take()
            .expect("The stack should be taken out after running.");
        let next_task = unsafe { base_to_ext(libvsched::current(get_cpu_id())) };
        let next_stack = unsafe { &mut *next_task.kernel_stack() };
        if next_stack.is_none() && !next_task.is_init() && !next_task.is_idle() {
            next_stack.replace(stack);
        } else {
            unsafe {
                let prev_ctx_ptr = prev_task.ctx_mut_ptr();
                let next_ctx_ptr = next_task.ctx_mut_ptr();
                recycle_stack_of_coroutine(stack);
                (*prev_ctx_ptr).switch_to(&*next_ctx_ptr);
                panic!("Should never reach here.");
            }
        }
    }
}
