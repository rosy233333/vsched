#![feature(unsafe_cell_access)]

use std::{cell::RefCell, sync::atomic::AtomicUsize};

use crate_interface::impl_interface;
use libvsched::{MappingFlags, MemIf};
use memmap2::{Mmap, MmapMut};
use task_management::interface::{SMPIf, TaskIf};
extern crate alloc;

// mod vsched;
// pub use vsched::*;

#[unsafe(no_mangle)]
pub static CPU_NUM: usize = config::SMP;

static CPU_ID_ALLOCATOR: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    pub static CPU_ID: RefCell<usize> = RefCell::new(0);
}

pub fn get_cpu_id() -> usize {
    CPU_ID.with(|cpu_id| *cpu_id.borrow())
}

pub fn init_cpu_id() {
    CPU_ID.set(CPU_ID_ALLOCATOR.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
}

struct SMPIfImpl;

#[impl_interface]
impl SMPIf for SMPIfImpl {
    fn get_cpu_id() -> usize {
        get_cpu_id()
    }
}

struct TaskIfImpl;

#[impl_interface]
impl TaskIf for TaskIfImpl {
    fn main_task_exit(exit_code: i32) -> ! {
        unsafe { libc::exit(exit_code) }
    }
}

struct MemIfImpl;

#[impl_interface]
impl MemIf for MemIfImpl {
    #[doc = " 分配用于vDSO和vVAR的空间，返回指向首地址的指针。"]
    #[doc = ""]
    #[doc = " 若需要实现vDSO和vVAR在多地址空间的共享，则需要在分配时使这块空间可被共享。"]
    fn alloc(size: usize) -> *mut u8 {
        let map = MmapMut::map_anon(size).expect("Failed to allocate memory for vDSO and vVAR");
        let ptr = map.as_ptr() as *mut u8;
        std::mem::forget(map);
        ptr
    }

    #[doc = " 从`alloc`返回的空间中，设置其中一块的访问权限。"]
    #[doc = ""]
    #[doc = " `flags`可能包含：READ、WRITE、EXECUTE、USER。"]
    fn protect(addr: *mut u8, len: usize, flags: MappingFlags) {
        let mut libc_flag = libc::PROT_READ;
        if flags.contains(MappingFlags::EXECUTE) {
            libc_flag |= libc::PROT_EXEC;
        }
        if flags.contains(MappingFlags::WRITE) {
            libc_flag |= libc::PROT_WRITE;
        }
        unsafe {
            if libc::mprotect(addr as _, len, libc_flag) == libc::MAP_FAILED as _ {
                panic!("vdso: mprotect res failed");
            }
        };
    }
}
