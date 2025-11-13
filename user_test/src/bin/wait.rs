use task_management::task_api::*;
use user_test::*;
fn main() {
    env_logger::init();
    let vsched_map = map_vsched().unwrap();
    core::mem::forget(vsched_map);
    init_cpu_id();
    init_vsched();
    // task3等待task2，task2等待task1
    // 运行顺序：task3 -> task2 -> task1 -> task2 -> task3
    // 打印顺序：(1) -> (2) -> (3) -> (4) -> (5)
    let task1 = new(
        || {
            println!("(3) into spawned task inner");
        },
        "task__1".into(),
        config::TASK_STACK_SIZE,
    );
    let task1_clone = task1.clone();
    let task2 = new(
        move || {
            println!("(2) wait thread start");
            task1_clone.join();
            println!("(4) wait thread ok");
        },
        "task__2".into(),
        config::TASK_STACK_SIZE,
    );
    let task2_clone = task2.clone();
    let task3 = new_f(
        async move {
            println!("(1) wait coroutine start");
            task2_clone.join_f().await;
            println!("(5) wait coroutine ok");
        },
        "task__3".into(),
    );
    spawn(task3.clone());
    spawn(task2.clone());
    spawn(task1.clone());

    yield_now();

    task1.join();
    task2.join();
    task3.join();
    println!("back to idle task");
    exit(0)
}
