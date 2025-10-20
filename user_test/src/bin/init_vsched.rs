use user_test::*;
fn main() {
    env_logger::init();
    let vsched_map = map_vsched().unwrap();
    init_vsched();
    println!("{:?}", vsched_map.percpu(get_cpu_id()).idle_task);
    exit(0)
}
