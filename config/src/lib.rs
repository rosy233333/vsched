#![no_std]

#[rustfmt::skip]
mod mut_cfgs {
    include!(concat!(env!("OUT_DIR"), "/mut_cfgs.rs"));
}

// pub use mut_cfgs::*;

pub const DATA_SEC_MASK: usize = 0xFFFF_FFFF_FFFF_F000;
pub const TASK_STACK_SIZE: usize = 0x40000;
pub const PAGES_SIZE_4K: usize = 0x1000;
pub type AxCpuMask = cpumask::CpuMask<SMP>;

pub const SMP: usize = 2; // 只用于测试
pub const RQ_CAP: usize = 256; // 只用于测试
