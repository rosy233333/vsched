use std::sync::Arc;

use task_management::{task_api::*, waker_queue::WakerQueue};
use user_test::*;
fn main() {
    env_logger::init();
    let vsched_map = map_vsched().unwrap();
    core::mem::forget(vsched_map);
    init_cpu_id();
    init_vsched();

    let queue = Arc::new(WakerQueue::new());
    let queue_clone = queue.clone();
    let task1 = new_f(
        async move {
            println!("task1: before wait");
            queue_clone.wait_f().await;
            println!("task1: after wait");
        },
        "task__1".into(),
    );
    let queue_clone = queue.clone();
    let task2 = new_f(
        async move {
            println!("task2: before wait");
            queue_clone.wait_f().await;
            println!("task2: after wait");
        },
        "task__2".into(),
    );
    let queue_clone = queue.clone();
    let task3 = new_f(
        async move {
            println!("task3: before wait");
            queue_clone.wait_f().await;
            println!("task3: after wait");
        },
        "task__3".into(),
    );
    spawn(task3.clone());
    spawn(task2.clone());
    spawn(task1.clone());

    yield_now();
    println!("notify all");
    queue.notify_all(true);
    yield_now();

    task1.join();
    task2.join();
    task3.join();
    println!("back to idle task");
    exit(0)
}
