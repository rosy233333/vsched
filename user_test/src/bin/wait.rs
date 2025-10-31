use task_management::task_api::*;
use user_test::*;
fn main() {
    env_logger::init();
    let vsched_map = map_vsched().unwrap();
    core::mem::forget(vsched_map);
    init_cpu_id();
    init_vsched();
    let task1 = new(
        || {
            println!("into spawned task inner");
        },
        "task__1".into(),
        config::TASK_STACK_SIZE,
    );
    let task1_clone = task1.clone();
    let task2 = new(
        move || {
            println!("wait task start");
            task1_clone.join();
            println!("wait task ok");
        },
        "task__2".into(),
        config::TASK_STACK_SIZE,
    );
    // vsched_apis::spawn(get_cpu_id(), arcext_to_base(task2.clone()));
    // vsched_apis::spawn(get_cpu_id(), arcext_to_base(task1.clone()));
    spawn(task2.clone());
    spawn(task1.clone());

    yield_now();

    println!("back to idle task");
    task1.join();
    task2.join();
    exit(0)
}
