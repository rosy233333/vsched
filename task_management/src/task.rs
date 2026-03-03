//! 对任务的操作封装，以及一些在线程/协程调度中使用的函数实现。

use core::{
    array,
    mem::{ManuallyDrop, MaybeUninit},
    ops::Deref,
};

use crate::{
    interface::get_cpu_id,
    sched::{exit_f, yield_now},
    task_inner_ext::{
        ArcTaskRef, AxTask, TaskInner, TaskRef, arcext_to_base, arcext_to_waker,
        arcwaker_to_arcext, base_to_arcext, base_to_ext,
    },
    wait_queue::WaitQueue,
};
use alloc::{
    boxed::Box, collections::vec_deque::VecDeque, string::String, sync::Arc, task::Wake, vec::Vec,
};
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

/// 用于idle任务的入口点
pub fn run_idle() {
    loop {
        yield_now();
    }
}

/// 对线程入口函数的包装
extern "C" fn task_entry() {
    // 所有任务的恢复点都需要释放上一个任务的Arc引用，并清除其on_cpu标志。
    //
    // 此处为任务的恢复点之一。
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

/// 每个CPU维护一个协程栈池，避免频繁分配和释放栈空间。
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

/// 协程调度主循环
fn coroutine_schedule() {
    use core::task::{Context, Waker};
    loop {
        // 所有任务的恢复点都需要释放上一个任务的Arc引用，并清除其on_cpu标志。
        //
        // 此处为任务的恢复点之一。
        let prev_task =
            unsafe { base_to_ext(libvsched::take_prev_task_and_clear_on_cpu(get_cpu_id())) };
        if prev_task.state() == TaskState::Exited {
            let _prev_task_to_drop = unsafe { ManuallyDrop::into_inner(prev_task.into_arc()) };
        }
        drop(prev_task);
        // let waker = Waker::noop();
        // let mut cx = Context::from_waker(waker);
        let curr = unsafe { base_to_ext(libvsched::current(get_cpu_id())) };
        let waker = arcext_to_waker(ManuallyDrop::into_inner(curr.into_arc().clone())); // 此处into_arc返回`ManuallyDrop<Arc<AxTask>>`，先clone再into_inner得到`Arc<AxTask>`，因此原有的`Arc<AxTask>`不会被释放。
        let mut cx = Context::from_waker(&waker);

        let fut = curr
            .inner()
            .future()
            .as_mut()
            .expect("The task should be a coroutine");
        let _res = fut.as_mut().poll(&mut cx);
        // // 该行要求协程在返回Pending或完全结束时，都需要将state从Running切换到其它状态（Ready, Blocked, Exited）。
        // assert!(!curr.is_running(), "{} is still running", curr.id_name());

        // 协程返回时的状态与协程操作的关系：
        //
        // - Ready: 协程让出，或者阻塞后（在返回前）被唤醒。
        // - Blocked: 协程阻塞在调度队列上。
        // - Exited: 协程结束。
        // - Running: 协程通过`await`一个`Future`而阻塞，因为这些`Future`不会主动维护协程的状态。
        //
        // 对Exited任务的回收下一任务即将运行时进行，因此此处只需特殊处理Running的情况。
        if curr.is_running() {
            // 设置当前任务状态
            curr.set_state(TaskState::Blocked);
            // 当前任务还未改变，因此在此处调用`libvsched::resched_f`可以正确设置当前任务和上一任务。
            // 后续代码可以正确处理下一任务和本任务相同的情况，因此此处可以不管`resched_f`的返回值。
            libvsched::resched_f(get_cpu_id());
        }

        let prev_task = curr;
        let stack = unsafe { &mut *prev_task.kernel_stack() }
            .take()
            .expect("The stack should be taken out after running.");
        let next_task = unsafe { base_to_ext(libvsched::current(get_cpu_id())) };
        let next_stack = unsafe { &mut *next_task.kernel_stack() };
        if next_stack.is_none() && !next_task.is_init() && !next_task.is_idle() {
            log::debug!("reuse stack");
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

/// 根据AxTask创建的Waker。
///
/// 在使用时，需要通过指针转化，在引用计数不变的情况下将ArcTaskRef与Arc<TaskWaker>相互转化。
#[repr(transparent)]
pub struct TaskWaker {
    task: AxTask,
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        // 修改任务状态、将任务放入就绪队列
        libvsched::unblock_task(
            arcext_to_base(arcwaker_to_arcext(self)),
            true,
            get_cpu_id(),
            get_cpu_id(),
        );
    }
}
