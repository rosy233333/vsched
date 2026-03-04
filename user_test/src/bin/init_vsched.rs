use task_management::task_api::*;
use user_test::*;
fn main() {
    env_logger::init();
    libvsched::load_and_init();
    init_cpu_id();
    init_vsched();
    println!("{:?}", vsched_map.percpu(get_cpu_id()).idle_task);
    exit(0)
}
