use anyhow::anyhow;
use std::env;
use vergen_gitcl::{Emitter, GitclBuilder};

fn main() -> Result<(), anyhow::Error> {
    let out_dir = env::var("OUT_DIR").unwrap();
    for path in shaders::compile(&out_dir) {
        println!("cargo::rerun-if-changed={}", path.to_str().unwrap());
    }

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_os = target_os.as_str();

    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let target_arch = target_arch.as_str();

    // Object name and platform directory logic is generally based on a couple of pieces of openvr_api code:
    //  - platform directory names: https://github.com/ValveSoftware/openvr/blob/ae46a8dd0172580648c8922658a100439115d3eb/src/vrcore/pathtools_public.h#L127-L157
    //  - general logic and special cases: https://github.com/ValveSoftware/openvr/blob/ae46a8dd0172580648c8922658a100439115d3eb/src/openvr_api_public.cpp#L128-L144
    // The android and macos platforms have been omitted, since we are currently uninterested in supporting them.

    let vrclient_name = match (target_os, target_arch) {
        ("windows", "x86_64") => "vrclient_x64",
        _ => "vrclient",
    };

    let platform_location = match (target_os, target_arch) {
        ("windows", "x86") | ("windows", "x86_64") => "bin/",
        ("linux", "x86") => "bin/",
        ("linux", "x86_64") => "bin/linux64/",
        ("linux", "aarch64") => "bin/linuxarm64/",
        _ => return Err(anyhow!("Unsupported platform: {target_os}/{target_arch}")),
    };

    println!("cargo::rustc-env=XRIZER_OPENVR_PLATFORM_DIR={platform_location}");
    println!("cargo::rustc-env=XRIZER_OPENVR_VRCLIENT_NAME={vrclient_name}");

    let builder = GitclBuilder::default().describe(true, true, None).build()?;
    Emitter::default().add_instructions(&builder)?.emit()
}
