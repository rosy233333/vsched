use task_management::task_inner_ext::ext_to_base;
use user_test::*;
fn main() {
    env_logger::init();
    let vsched_map = map_vsched().unwrap();
    core::mem::forget(vsched_map);
    init_vsched();
    vsched_apis::spawn(
        get_cpu_id(),
        ext_to_base(Task::new_f(
            async {
                println!("into spawned task inner");
                yield_now_f().await;
                println!("back to spawned task after yield");
            },
            "spawn_test".into(),
        )),
    );
    vsched_apis::yield_now(get_cpu_id());
    println!("back to idle task");
    vsched_apis::yield_now(get_cpu_id());
    exit(0)
}
