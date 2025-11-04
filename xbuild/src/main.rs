use nanoserde::DeJson;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};

// https://doc.rust-lang.org/cargo/reference/external-tools.html#json-messages
#[derive(DeJson)]
struct Artifact {
    target: ArtifactTarget,
    filenames: Vec<String>,
}

#[derive(DeJson)]
struct ArtifactTarget {
    name: String,
    crate_types: Vec<String>,
}

#[derive(DeJson, Debug)]
struct BuildScriptExecution {
    env: Vec<[String; 2]>,
    package_id: String,
}

enum Message {
    CompilerArtifact(Artifact),
    BuildScriptExecuted(BuildScriptExecution),
    Unknown,
}

fn main() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut cmd = Command::new(cargo)
        .args([
            "build",
            "--message-format",
            "json-render-diagnostics",
            "-p",
            "xrizer",
        ])
        .args(std::env::args_os().skip(1))
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to call cargo");

    let stdout = cmd.stdout.take().unwrap();
    let mut stdout = BufReader::new(stdout);

    let mut lib_path: Option<String> = None;
    let mut platform_dir: Option<String> = None;
    let mut vrclient_name: Option<String> = None;
    let mut line = String::new();

    while stdout.read_line(&mut line).expect("Failed to read line") > 0 {
        let msg = Message::deserialize_json(&line).unwrap();
        line.clear();

        match msg {
            Message::CompilerArtifact(a) => {
                let target = a.target;
                if !(target.name == "xrizer" && target.crate_types.contains(&"cdylib".into())) {
                    continue;
                }

                lib_path = Some(
                    a.filenames
                        .into_iter()
                        .find(|p| p.ends_with(".so"))
                        .unwrap(),
                )
            }
            Message::BuildScriptExecuted(b) => {
                if !b.package_id.contains("xrizer#") && !b.package_id.contains("xrizer@") {
                    continue;
                }
                for [name, value] in b.env {
                    match name.as_str() {
                        "XRIZER_OPENVR_PLATFORM_DIR" => platform_dir = Some(value),
                        "XRIZER_OPENVR_VRCLIENT_NAME" => vrclient_name = Some(value),
                        _ => {}
                    }
                }
            }
            Message::Unknown => {}
        }
    }

    if !cmd.wait().expect("waiting for build failed").success() {
        std::process::exit(1);
    }
    let lib_path = PathBuf::from(lib_path.expect("lib path missing"));
    let platform_dir = platform_dir.expect("openvr platform directory should be known");
    let vrclient_name = vrclient_name.expect("vrclient name should be known");

    let parent = lib_path.parent().unwrap();
    let platform_path = parent.join(platform_dir);
    match std::fs::create_dir_all(&platform_path) {
        Ok(_) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        err => {
            eprintln!("Failed to create directory '{platform_path:?}': {err:?}");
            std::process::exit(1);
        }
    }

    let vrclient_path = platform_path.join(vrclient_name).with_extension(
        lib_path
            .extension()
            .expect("build shared library should have an extension"),
    );
    match std::os::unix::fs::symlink(&lib_path, vrclient_path) {
        Ok(_) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        err => {
            eprintln!("Failed to create vrclient symlink: {err:?}");
            std::process::exit(1);
        }
    }

    // Generate openvrpaths.vrpath file from template
    // This file tells OpenVR/SteamVR where to find the runtime (xrizer in this case)
    // The template contains placeholders that we replace with actual paths
    let template_path =
        PathBuf::from(std::env!("CARGO_MANIFEST_DIR")).join("openvrpaths.vrpath.in");
    let output_path = parent.join("openvrpaths.vrpath");

    // Use XRIZER_INSTALL_PREFIX environment variable if set, otherwise use the parent directory
    let install_prefix = std::env::var("XRIZER_INSTALL_PREFIX")
        .unwrap_or_else(|_| parent.to_str().unwrap().to_string());

    match std::fs::read_to_string(&template_path) {
        Ok(template_content) => {
            // Replace ${XRIZER_INSTALL_PREFIX} with the install prefix
            let content = template_content.replace("${XRIZER_INSTALL_PREFIX}", &install_prefix);

            if let Err(e) = std::fs::write(&output_path, content.as_bytes()) {
                eprintln!("Failed to create openvrpaths.vrpath: {e}");
            }
        }
        Err(e) => {
            eprintln!("Failed to read template file {template_path:?}: {e}");
        }
    }

    // This file seems to prevent Steam from overwriting xrizer as a runtime path in the openvrpaths.
    let version = parent.join("bin/version.txt");
    match std::fs::File::create(version) {
        Ok(_) => (),
        err => {
            eprintln!("Failed to create bin/linux64 directory: {err:?}");
            std::process::exit(1);
        }
    }
}

impl DeJson for Message {
    fn de_json(
        state: &mut nanoserde::DeJsonState,
        input: &mut std::str::Chars,
    ) -> Result<Self, nanoserde::DeJsonErr> {
        state.curly_open(input)?;
        let key = String::de_json(state, input)?;
        if key != "reason" {
            return Ok(Self::Unknown);
        }
        state.colon(input)?;
        let reason = String::de_json(state, input)?;
        match reason.as_str() {
            "compiler-artifact" => {
                let fixed: String = ['{', state.cur].into_iter().chain(input).collect();
                let msg = Artifact::deserialize_json(&fixed).unwrap();
                Ok(Self::CompilerArtifact(msg))
            }
            "build-script-executed" => {
                let fixed: String = ['{', state.cur].into_iter().chain(input).collect();
                let msg = BuildScriptExecution::deserialize_json(&fixed).unwrap();
                Ok(Self::BuildScriptExecuted(msg))
            }
            _ => Ok(Self::Unknown),
        }
    }
}
