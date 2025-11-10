#![feature(unsafe_cell_access)]
extern crate alloc;

mod vsched;
pub use vsched::*;

#[unsafe(no_mangle)]
pub static CPU_NUM: usize = config::SMP;
