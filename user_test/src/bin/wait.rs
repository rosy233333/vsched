use task_management::task_inner_ext::ext_to_base;
use user_test::*;
fn main() {
    env_logger::init();
    let vsched_map = map_vsched().unwrap();
    core::mem::forget(vsched_map);
    init_vsched();
    let task1 = Task::new(
        || {
            println!("into spawned task inner");
        },
        "task__1".into(),
        config::TASK_STACK_SIZE,
    );
    let task1_clone = Task::clone_increase_sc(&task1);
    let task2 = Task::new(
        move || {
            println!("wait task start");
            task1_clone.task_ext().join();
            println!("wait task ok");
            Task::drop_decrease_sc(task1_clone);
        },
        "task__2".into(),
        config::TASK_STACK_SIZE,
    );
    vsched_apis::spawn(get_cpu_id(), ext_to_base(Task::clone_increase_sc(&task2)));
    vsched_apis::spawn(get_cpu_id(), ext_to_base(Task::clone_increase_sc(&task1)));

    vsched_apis::yield_now(get_cpu_id());

    println!("back to idle task");
    task1.join();
    task2.join();
    Task::drop_decrease_sc(task1);
    Task::drop_decrease_sc(task2);
    exit(0)
}
