use std::sync::atomic::AtomicUsize;
use task_management::{
    task::run_idle,
    task_api::*,
    task_inner_ext::{ArcTaskRef, TaskRef},
};
use user_test::*;

const TASK_NUM: usize = 10;

fn main() {
    env_logger::init();
    libvsched::load_and_init();
    static BOOT_COUNT: AtomicUsize = AtomicUsize::new(1);
    for i in 0..(config::SMP - 1) {
        println!("spawn kernel thread {}", i);
        let _thread_handle = std::thread::spawn(|| {
            init_cpu_id();
            init_vsched_secondary();
            BOOT_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            run_idle();
        });
    }

    init_cpu_id();
    init_vsched();
    while BOOT_COUNT.load(std::sync::atomic::Ordering::Relaxed) < config::SMP {
        core::hint::spin_loop();
    }

    // let task = new(
    //     || {
    //         println!("into spawned task inner main thread spawned");
    //     },
    //     "main spawn_test".into(),
    //     config::TASK_STACK_SIZE,
    // );
    // spawn(task.clone());
    // task.join().unwrap();
    // println!("main task wait ok");
    let mut tasks: [Option<ArcTaskRef>; TASK_NUM] = Default::default();
    for i in (0..TASK_NUM).rev() {
        let prev_task = if i + 1 < TASK_NUM {
            Some(tasks[i + 1].as_ref().unwrap().clone())
        } else {
            None
        };
        tasks[i] = if i >= TASK_NUM / 2 {
            Some(new(
                move || {
                    println!("into thread {}", i);
                    yield_now();
                    println!("thread {} after yield", i);
                    if let Some(prev_task) = prev_task {
                        prev_task.join();
                    }
                    println!("({}) thread {} after join", TASK_NUM - i, i);
                },
                format!("task__{}", i),
                config::TASK_STACK_SIZE,
            ))
        } else {
            Some(new_f(
                async move {
                    println!("into coroutine {}", i);
                    yield_now_f().await;
                    println!("coroutine {} after yield", i);
                    if let Some(prev_task) = prev_task {
                        prev_task.join_f().await;
                    }
                    println!("({}) coroutine {} after join", TASK_NUM - i, i);
                },
                format!("task__{}", i),
            ))
        };
    }

    for task in tasks.iter() {
        spawn(task.as_ref().unwrap().clone());
    }

    tasks[0].as_ref().unwrap().join();

    println!("test ok!");

    exit(0)
}
