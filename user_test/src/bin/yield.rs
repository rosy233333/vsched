use task_management::task_api::*;
use user_test::*;
fn main() {
    env_logger::init();
    let vsched_map = map_vsched().unwrap();
    core::mem::forget(vsched_map);
    init_cpu_id();
    init_vsched();
    // vsched_apis::spawn(
    //     get_cpu_id(),
    //     arcext_to_base(task::new_f(
    //         async {
    //             println!("into spawned task inner");
    //             yield_now_f().await;
    //             println!("back to spawned task after yield");
    //         },
    //         "spawn_test".into(),
    //     )),
    // );
    spawn(new_f(
        async {
            println!("into spawned task inner");
            yield_now_f().await;
            println!("back to spawned task after yield");
        },
        "spawn_test".into(),
    ));
    yield_now();
    println!("back to idle task");
    yield_now();
    exit(0)
}
