#![feature(unsafe_cell_access)]
extern crate alloc;

// mod task;
mod vsched;
// mod wait_queue;

// pub use task::*;
pub use vsched::*;
// pub use wait_queue::*;

#[unsafe(no_mangle)]
pub static CPU_NUM: usize = config::SMP;
