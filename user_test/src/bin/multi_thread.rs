use std::sync::atomic::AtomicUsize;

use task_management::{
    sched::{exit, init_vsched, init_vsched_secondary},
    task::{self, run_idle},
    task_inner_ext::{arcext_to_base, ext_to_base},
};
use user_test::*;
fn main() {
    env_logger::init();
    let vsched_map = map_vsched().unwrap();
    core::mem::forget(vsched_map);
    static BOOT_COUNT: AtomicUsize = AtomicUsize::new(1);
    let _thread_handle = std::thread::spawn(|| {
        init_cpu_id();
        init_vsched_secondary();
        BOOT_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        run_idle();
    });

    init_cpu_id();
    init_vsched();
    while BOOT_COUNT.load(std::sync::atomic::Ordering::Relaxed) < config::SMP {
        core::hint::spin_loop();
    }
    let task = task::new(
        || {
            println!("into spawned task inner main thread spawned");
        },
        "main spawn_test".into(),
        config::TASK_STACK_SIZE,
    );
    vsched_apis::spawn(get_cpu_id(), arcext_to_base(task.clone()));
    task.join().unwrap();
    println!("main task wait ok");
    exit(0)
}
