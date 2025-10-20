use user_test::*;
fn main() {
    env_logger::init();
    let vsched_map = map_vsched().unwrap();
    core::mem::forget(vsched_map);
    init_vsched();
    let task1 = Task::new_f(
        async {
            println!("into spawned task inner");
        },
        "task__1".into(),
    );
    let task1_clone = Task::clone_increase_sc(&task1);
    let task2 = Task::new_f(
        async move {
            println!("wait task start");
            task1_clone.task_ext().join_f().await;
            println!("wait task ok");
            Task::drop_decrease_sc(task1_clone);
        },
        "task__2".into(),
    );
    vsched_apis::spawn(get_cpu_id(), Task::clone_increase_sc(&task2));
    vsched_apis::spawn(get_cpu_id(), Task::clone_increase_sc(&task1));

    vsched_apis::yield_now(get_cpu_id());

    println!("back to idle task");
    task1.task_ext().join();
    task2.task_ext().join();
    Task::drop_decrease_sc(task1);
    Task::drop_decrease_sc(task2);
    exit(0)
}
