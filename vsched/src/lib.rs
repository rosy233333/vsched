//! 基于vDSO的任务调度器（的vDSO部分）
//!
//! 本模块中的接口并未涵盖完整的任务调度流程，
//! 完整的任务调度流程请见[README.md](https://github.com/rosy233333/vsched/blob/refact/README.md)

#![no_std]
#![feature(unsafe_cell_access)]
#![deny(missing_docs)]

mod api;
mod sched;
pub use api::*;

pub use base_task::PerCPU;
use config::SMP;
use core::{cell::UnsafeCell, mem::MaybeUninit};
use vdso_helper::vvar_data;

/// vVAR数据区，用于存储每个CPU的调度器（调度队列、当前任务等）
pub struct VvarDataInner(pub [UnsafeCell<MaybeUninit<PerCPU>>; SMP]);

impl Default for VvarDataInner {
    fn default() -> Self {
        Self([const { UnsafeCell::new(MaybeUninit::uninit()) }; SMP])
    }
}

unsafe impl Sync for VvarDataInner {}

vvar_data! {
    data: VvarDataInner,
}
