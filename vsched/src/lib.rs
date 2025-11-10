//! `SharedScheduler` via vDSO.
#![no_std]
#![feature(unsafe_cell_access)]

mod api;
mod sched;
pub use api::*;

pub use base_task::PerCPU;
use config::SMP;
use core::{cell::UnsafeCell, mem::MaybeUninit};
use vdso_helper::vvar_data;

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
