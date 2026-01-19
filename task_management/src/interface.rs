//! 需要其它模块实现的接口
//!
//! 通过[`crate_interface`](https://docs.rs/crate_interface/latest/crate_interface/)实现接口的定义和调用，
//! 因此其它模块也需通过`crate_interface`来实现这些接口。

use crate_interface::{call_interface, def_interface};

/// 与多核相关的接口
#[def_interface]
pub trait SMPIf {
    /// 获取当前CPU的ID
    fn get_cpu_id() -> usize;
}

#[inline]
pub(crate) fn get_cpu_id() -> usize {
    call_interface!(SMPIf::get_cpu_id())
}

/// 与任务调度相关的接口
#[def_interface]
pub trait TaskIf {
    /// 调度器主任务退出后，系统的行为
    fn main_task_exit(exit_code: i32) -> !;
}

#[inline]
pub(crate) fn main_task_exit(exit_code: i32) -> ! {
    call_interface!(TaskIf::main_task_exit(exit_code))
}
