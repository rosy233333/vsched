use build_vdso::*;

fn main() {
    println!("cargo:rerun-if-changed=.");
    println!("cargo::rerun-if-env-changed=SMP");
    println!("cargo::rerun-if-env-changed=RQ_CAP");
    let mut config = BuildConfig::new("../vsched", "vsched");
    config.out_dir = String::from("../output");
    build_vdso(&config);
}
