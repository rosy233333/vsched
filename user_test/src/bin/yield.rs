use task_management::task_api::*;
use user_test::*;
fn main() {
    env_logger::init();
    let vsched_map = map_vsched().unwrap();
    core::mem::forget(vsched_map);
    init_cpu_id();
    init_vsched();

    // 运行顺序： main task -> thread1 -> coroutine2 -> coroutine3 -> main task -> thread1 -> coroutine2 -> coroutine3
    // 打印顺序：(1) -> (2) -> (3) -> (4) -> (5) -> (6) -> (7) -> (8)
    let task1 = new(
        || {
            println!("(2) into thread1");
            yield_now();
            println!("(6) back to thread1 after yield");
        },
        "task__1".into(),
        config::TASK_STACK_SIZE,
    );
    let task2 = new_f(
        async {
            println!("(3) into coroutine2");
            yield_now_f().await;
            println!("(7) back to coroutine2 after yield");
        },
        "task__2".into(),
    );
    let task3 = new_f(
        async {
            println!("(4) into coroutine3");
            yield_now_f().await;
            println!("(8) back to coroutine3 after yield");
        },
        "task__3".into(),
    );
    spawn(task1);
    spawn(task2);
    spawn(task3);
    println!("(1) main task before yield");
    yield_now();
    println!("(5) back to main task");
    yield_now();
    exit(0)
}
