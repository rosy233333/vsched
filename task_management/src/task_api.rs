//! 任务相关API。
//!
//! 本模块不包含任务的阻塞操作，阻塞操作参见[`wait_queue`](crate::wait_queue)模块。

use crate::{
    interface::get_cpu_id,
    task_inner_ext::{ArcTaskRef, arcext_to_base},
};
use alloc::string::String;

/// 在主CPU上初始化调度器。
///
/// 调用此函数前，应先正确映射好vDSO和vVAR内存区域，并调用[`libvsched::init_vdso_vtable`]函数。
///
/// 调用此函数后，当前执行流也视为一个任务（主任务），并会继续运行当前执行流。
#[inline]
pub fn init_vsched() {
    crate::sched::init_vsched()
}

/// 在副CPU上初始化调度器。
///
/// 调用此函数前，应先正确映射好vDSO和vVAR内存区域，并调用[`libvsched::init_vdso_vtable`]函数。
#[inline]
pub fn init_vsched_secondary() {
    crate::sched::init_vsched_secondary()
}

/// 以`entry`为入口函数创建线程。
///
/// 返回对任务的引用，不运行该任务。
#[inline]
pub fn new<F>(entry: F, name: String, stack_size: usize) -> ArcTaskRef
where
    F: FnOnce() + Send + 'static,
{
    crate::task::new(entry, name, stack_size)
}

/// 以`future`创建协程。
///
/// 返回对任务的引用，不运行该任务。
#[inline]
pub fn new_f<F>(future: F, name: String) -> ArcTaskRef
where
    F: Future<Output = ()> + Send + 'static,
{
    crate::task::new_f(future, name)
}

/// 在当前CPU上运行任务。
#[inline]
pub fn spawn(task_ref: ArcTaskRef) {
    libvsched::spawn(get_cpu_id(), arcext_to_base(task_ref));
}

/// 以`exit_code`退出当前线程。
#[inline]
pub fn exit(exit_code: i32) -> ! {
    crate::sched::exit(exit_code)
}

/// 以`exit_code`退出当前协程。
#[inline]
pub async fn exit_f(exit_code: i32) {
    crate::sched::exit_f(exit_code).await
}

/// 让出当前线程。
#[inline]
pub fn yield_now() {
    crate::sched::yield_now()
}

/// 让出当前协程。
#[inline]
pub async fn yield_now_f() {
    crate::sched::yield_now_f().await
}
