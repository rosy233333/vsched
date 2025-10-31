use crate::{
    interface::get_cpu_id,
    task_inner_ext::{ArcTaskRef, arcext_to_base},
};
use alloc::string::String;

#[inline]
pub fn init_vsched() {
    crate::sched::init_vsched()
}

#[inline]
pub fn init_vsched_secondary() {
    crate::sched::init_vsched_secondary()
}

#[inline]
pub fn new<F>(entry: F, name: String, stack_size: usize) -> ArcTaskRef
where
    F: FnOnce() + Send + 'static,
{
    crate::task::new(entry, name, stack_size)
}

#[inline]
pub fn new_f<F>(future: F, name: String) -> ArcTaskRef
where
    F: Future<Output = ()> + Send + 'static,
{
    crate::task::new_f(future, name)
}

#[inline]
pub fn spawn(task_ref: ArcTaskRef) {
    vsched_apis::spawn(get_cpu_id(), arcext_to_base(task_ref));
}

#[inline]
pub fn exit(exit_code: i32) -> ! {
    crate::sched::exit(exit_code)
}

#[inline]
pub async fn exit_f(exit_code: i32) {
    crate::sched::exit_f(exit_code).await
}

/// 所有线程的恢复点都需要调用`clear_prev_task_on_cpu`。
///
/// 此处的`vsched_apis::yield_now`之后为线程的恢复点之一。
#[inline]
pub fn yield_now() {
    crate::sched::yield_now()
}

/// Current coroutine task gives up the CPU time voluntarily, and switches to another
/// ready task.
#[inline]
pub async fn yield_now_f() {
    crate::sched::yield_now_f().await
}
