use crate_interface::{call_interface, def_interface};

#[def_interface]
pub trait SMPIf {
    fn get_cpu_id() -> usize;
}

#[inline]
pub(crate) fn get_cpu_id() -> usize {
    call_interface!(SMPIf::get_cpu_id())
}

#[def_interface]
pub trait TaskIf {
    /// 调度器主任务退出后，系统的行为
    fn main_task_exit(exit_code: i32) -> !;
}

#[inline]
pub(crate) fn main_task_exit(exit_code: i32) -> ! {
    call_interface!(TaskIf::main_task_exit(exit_code))
}
