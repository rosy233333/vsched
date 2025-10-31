use crate_interface::{call_interface, def_interface};

#[def_interface]
pub trait SMPIf {
    fn get_cpu_id() -> usize;
}

#[inline]
pub(crate) fn get_cpu_id() -> usize {
    call_interface!(SMPIf::get_cpu_id())
}

// TODO: trait_interface目前不支持用接口传递常量。
// 未来可以考虑修改trait_interface库，以通过trait_interface传递`CPU_NUM`。
unsafe extern "Rust" {
    pub(crate) static CPU_NUM: usize;
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
