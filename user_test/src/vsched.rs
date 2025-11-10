// use base_task::{PerCPU, TaskExtRef, TaskRef, TaskState};
use base_task::{PerCPU, TaskState};
use config::AxCpuMask;
use core::pin::Pin;
use core::str::from_utf8;
use core::task::{Context, Poll};
use crate_interface::impl_interface;
use libvsched::VvarData;
use memmap2::MmapMut;
use page_table_entry::MappingFlags;
use std::cell::RefCell;
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex};
use std::thread_local;
use std::{collections::VecDeque, sync::atomic::AtomicUsize};
use task_management::interface::{SMPIf, TaskIf};
use task_management::{
    task_inner_ext::{TaskRef, base_to_ext, ext_to_base},
    wait_queue::{WaitQueue, WaitQueueGuard},
};
// pub use vsched_apis::*;

use xmas_elf::program::SegmentData;

const VSCHED: &[u8] = include_bytes_aligned::include_bytes_aligned!(8, "../../output/libvsched.so");

static CPU_ID_ALLOCATOR: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    pub static CPU_ID: RefCell<usize> = RefCell::new(0);
}

pub fn get_cpu_id() -> usize {
    CPU_ID.with(|cpu_id| *cpu_id.borrow())
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

pub struct Vsched {
    #[allow(unused)]
    map: MmapMut,
}

impl Vsched {
    pub fn percpu(&self, index: usize) -> &PerCPU {
        let base = self.map.as_ptr() as *const VvarData;
        unsafe { (&*base).data.0[index].as_ref_unchecked().assume_init_ref() }
    }
}

const A: usize = core::mem::size_of::<VvarData>();
const B: usize = core::mem::align_of::<VvarData>();

pub fn map_vsched() -> Result<Vsched, ()> {
    let mut vsched_map = MmapMut::map_anon(VSCHED_DATA_SIZE + 0x40000).unwrap();
    log::info!(
        "vsched_map base: [{:p}, {:p}]",
        vsched_map.as_ptr(),
        unsafe { vsched_map.as_ptr().add(VSCHED_DATA_SIZE + 0x40000) }
    );

    log::debug!(
        "VVAR: VA:{:?}, {:#x}, {:?}",
        vsched_map.as_ptr(),
        core::mem::size_of::<VvarData>(),
        MappingFlags::READ | MappingFlags::WRITE | MappingFlags::USER,
    );
    unsafe {
        if libc::mprotect(
            vsched_map.as_ptr() as _,
            core::mem::size_of::<VvarData>(),
            libc::PROT_READ | libc::PROT_WRITE,
        ) == libc::MAP_FAILED as _
        {
            log::error!("vvar: mprotect res failed");
            return Err(());
        }
    };
    let vvar = vsched_map.as_ptr() as *const u8 as *mut u8 as *mut () as *mut VvarData;
    unsafe { vvar.write(VvarData::default()) };

    let vsched_so = &mut vsched_map[VSCHED_DATA_SIZE..];

    let vsched_elf = xmas_elf::ElfFile::new(VSCHED).expect("Error parsing app ELF file.");
    if let Some(interp) = vsched_elf
        .program_iter()
        .find(|ph| ph.get_type() == Ok(xmas_elf::program::Type::Interp))
    {
        let interp = match interp.get_data(&vsched_elf) {
            Ok(SegmentData::Undefined(data)) => data,
            _ => panic!("Invalid data in Interp Elf Program Header"),
        };

        let interp_path = from_utf8(interp).expect("Interpreter path isn't valid UTF-8");
        // remove trailing '\0'
        let _interp_path = interp_path.trim_matches(char::from(0)).to_string();
        log::debug!("Interpreter path: {:?}", _interp_path);
    }
    let elf_base_addr = Some(vsched_so.as_ptr() as usize);
    // let relocate_pairs = elf_parser::get_relocate_pairs(&elf, elf_base_addr);
    let segments = elf_parser::get_elf_segments(&vsched_elf, elf_base_addr);
    let relocate_pairs = elf_parser::get_relocate_pairs(&vsched_elf, elf_base_addr);
    for segment in segments {
        if segment.size == 0 {
            log::warn!(
                "Segment with size 0 found, skipping: {:?}, {:#x}, {:?}",
                segment.vaddr,
                segment.size,
                segment.flags
            );
            continue;
        }
        log::debug!(
            "{:?}, {:#x}, {:?}",
            segment.vaddr,
            segment.size,
            segment.flags
        );
        let mut flag = libc::PROT_READ;
        if segment.flags.contains(MappingFlags::EXECUTE) {
            flag |= libc::PROT_EXEC;
        }
        if segment.flags.contains(MappingFlags::WRITE) {
            flag |= libc::PROT_WRITE;
        }
        if let Some(data) = segment.data {
            assert!(data.len() <= segment.size);
            let src = data.as_ptr();
            let dst = segment.vaddr.as_usize() as *mut u8;
            let count = data.len();
            unsafe {
                core::ptr::copy_nonoverlapping(src, dst, count);
                if segment.size > count {
                    core::ptr::write_bytes(dst.add(count), 0, segment.size - count);
                }
            }
        } else {
            unsafe { core::ptr::write_bytes(segment.vaddr.as_usize() as *mut u8, 0, segment.size) };
        }

        unsafe {
            if libc::mprotect(segment.vaddr.as_usize() as _, segment.size, flag)
                == libc::MAP_FAILED as _
            {
                log::error!(
                    "mprotect res failed: addr: {:#x}, size: {}, prot: {}",
                    segment.vaddr.as_usize(),
                    segment.size,
                    flag
                );
                return Err(());
            }
        };
    }

    for relocate_pair in relocate_pairs {
        let src: usize = relocate_pair.src.into();
        let dst: usize = relocate_pair.dst.into();
        let count = relocate_pair.count;
        log::info!(
            "Relocate: src: 0x{:x}, dst: 0x{:x}, count: {}",
            src,
            dst,
            count
        );
        unsafe { core::ptr::copy_nonoverlapping(src.to_ne_bytes().as_ptr(), dst as *mut u8, count) }
    }

    unsafe { libvsched::init_vdso_vtable(elf_base_addr.unwrap() as _) };

    log::info!("vsched mapped successfully");
    Ok(Vsched { map: vsched_map })
}

pub fn init_cpu_id() {
    CPU_ID.set(CPU_ID_ALLOCATOR.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
}

const VSCHED_DATA_SIZE: usize = config::SMP
    * ((core::mem::size_of::<PerCPU>() + config::PAGES_SIZE_4K - 1)
        & (!(config::PAGES_SIZE_4K - 1)));
