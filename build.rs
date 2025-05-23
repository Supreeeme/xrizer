use std::env;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    for path in shaders::compile(&out_dir) {
        println!("cargo::rerun-if-changed={}", path.to_str().unwrap());
    }

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_os = target_os.as_str();

    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let target_arch = target_arch.as_str();

    let vrclient_name = match (target_os, target_arch) {
        ("windows", "x86_64") => "vrclient_x64",
        _ => "vrclient",
    };

    let platform_location = match (target_os, target_arch) {
        ("windows", _) => "bin/",
        ("linux", "x86") => "bin/",
        ("linux", "x86_64") => "bin/linux64/",
        ("linux", "aarch64") => "bin/linuxarm64/",
        ("android", "arm") => "bin/androidarm32/",
        ("android", "aarch64") => "bin/androidarm64/",
        ("macos", _) => "bin/osx32/",
        _ => "bin/unknown_plaform/",
    };

    println!("cargo::rustc-env=XRIZER_OPENVR_PLATFORM_DIR={platform_location}");
    println!("cargo::rustc-env=XRIZER_OPENVR_VRCLIENT_NAME={vrclient_name}");
}
