use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

#[derive(Serialize, Deserialize)]
struct CachedShader {
    spirv: PathBuf,
    last_modified: SystemTime,
}

pub fn compile(out_dir: &str) -> Vec<PathBuf> {
    let shaders = [
        ("overlay.vert", "vert_overlay.spv"),
        ("overlay.frag", "frag_overlay.spv"),
    ];

    let cache_path = PathBuf::from(out_dir).join("shader_cache.json");
    let mut old_cache: HashMap<PathBuf, CachedShader> = std::fs::read(&cache_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let mut new_cache = HashMap::new();

    let shader_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let out_dir = PathBuf::from(out_dir);
    for (source, output) in shaders {
        let source = shader_dir.join(source);
        let output = out_dir.join(output);

        let mut add_to_cache = |last_modified| {
            new_cache.insert(
                source.clone(),
                CachedShader {
                    spirv: output.clone(),
                    last_modified,
                },
            )
        };

        if let Some(cached) = old_cache.remove(&source)
            && Path::new(&cached.spirv) == output && !modified_since(&source, cached.last_modified)
            {
                add_to_cache(cached.last_modified);
                continue;
            }

        compile_shader(&source, &output);
        add_to_cache(SystemTime::now());
    }

    let cache = std::fs::File::create(cache_path).expect("Couldn't create cache file");
    serde_json::to_writer(cache, &new_cache).expect("Couldn't write cache");
    new_cache.into_keys().collect()
}

fn compile_shader(input: &Path, output: &Path) {
    let success = Command::new("glslc")
        .arg(input)
        .arg("-o")
        .arg(output)
        .spawn()
        .expect("failed to launch glslc")
        .wait()
        .expect("glslc wasn't running")
        .success();
    assert!(success, "failed to compile shader {input:?}");
}

fn modified_since(file: &Path, last_modified: SystemTime) -> bool {
    std::fs::metadata(file)
        .map(|m| m.modified().expect("can't get last modified time") > last_modified)
        .unwrap_or(true)
}
