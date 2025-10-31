use task_management::{
    sched::{exit, init_vsched},
    task,
    task_inner_ext::{arcext_to_base, ext_to_base},
};
use user_test::*;
fn main() {
    env_logger::init();
    let vsched_map = map_vsched().unwrap();
    core::mem::forget(vsched_map);
    init_cpu_id();
    init_vsched();
    // Due to the init_vsched will spawn the `gc` and `idle` task to the scheduler,
    // the number must be not greater than `RQ_CAP - 2`.
    for _ in 0..(config::RQ_CAP - 2) {
        vsched_apis::spawn(
            get_cpu_id(),
            arcext_to_base(task::new(
                || {
                    println!("into spawned task inner");
                },
                "spawn_test".into(),
                config::TASK_STACK_SIZE,
            )),
        );
    }
    println!("spawn test ok");
    exit(0)
}
