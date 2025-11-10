use build_vdso::*;

fn main() {
    let mut config = BuildConfig::new("../vsched", "vsched");
    config.out_dir = String::from("../output");
    build_vdso(&config);
}
