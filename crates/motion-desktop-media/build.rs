//! Bake producer identity into the program instead of reopening an executable
//! path that may have been replaced while an older process is still running.
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

fn sources(path: &Path, output: &mut Vec<PathBuf>, visited: &mut usize, depth: usize) {
    *visited += 1;
    assert!(
        *visited <= 65536 && depth <= 64,
        "producer source traversal exceeds bound"
    );
    println!("cargo:rerun-if-changed={}", path.display());
    let metadata = fs::symlink_metadata(path).expect("inspect producer source");
    assert!(
        !metadata.file_type().is_symlink(),
        "producer sources must be regular repository files"
    );
    if metadata.is_dir() {
        for entry in fs::read_dir(path).expect("read producer directory") {
            let entry = entry.expect("read producer entry");
            let name = entry.file_name();
            if matches!(name.to_str(), Some("target" | ".git" | "node_modules")) {
                continue;
            }
            sources(&entry.path(), output, visited, depth + 1);
        }
    } else {
        // Hash every regular input, including embedded resources and templates.
        // An extension allowlist silently misses new renderer input formats.
        assert!(metadata.is_file(), "producer input must be a regular file");
        output.push(path.to_path_buf());
        assert!(output.len() <= 32768, "producer source count exceeds bound");
    }
}
fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let root = manifest
        .parent()
        .and_then(Path::parent)
        .expect("media crate inside workspace");
    let mut files = Vec::new();
    let mut visited = 0;
    for relative in [
        "Cargo.toml",
        "Cargo.lock",
        "crates",
        "desktop/app",
        "desktop/ui",
    ] {
        sources(&root.join(relative), &mut files, &mut visited, 0);
    }
    files.sort();
    files.dedup();
    let mut hash = Sha256::new();
    hash.update(b"motion-studio/native-disk-producer/v1\0");
    let compiler = Command::new(env::var_os("RUSTC").expect("compiler path"))
        .args(["--version", "--verbose"])
        .output()
        .expect("read compiler identity");
    assert!(compiler.status.success(), "compiler identity failed");
    hash.update(&compiler.stdout);
    let mut settings: Vec<_> = env::vars()
        .filter(|(k, _)| {
            k.starts_with("CARGO_FEATURE_")
                || k.starts_with("CARGO_CFG_TARGET_")
                || matches!(
                    k.as_str(),
                    "DEBUG" | "OPT_LEVEL" | "CARGO_ENCODED_RUSTFLAGS"
                )
        })
        .collect();
    settings.sort();
    for (name, value) in settings {
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name);
        hash.update((value.len() as u64).to_le_bytes());
        hash.update(value);
    }
    let mut buffer = [0u8; 64 << 10];
    let mut total = 0u64;
    for path in files {
        let relative = path
            .strip_prefix(root)
            .expect("repository source")
            .to_str()
            .expect("UTF8 source name")
            .replace('\\', "/");
        hash.update((relative.len() as u64).to_le_bytes());
        hash.update(relative);
        let mut file = fs::File::open(path).expect("open producer source");
        let length = file.metadata().expect("source metadata").len();
        assert!(length <= 64 << 20, "producer source size exceeds bound");
        hash.update(length.to_le_bytes());
        let mut read_bytes = 0u64;
        loop {
            let read = file.read(&mut buffer).expect("hash producer source");
            if read == 0 {
                break;
            }
            read_bytes += read as u64;
            total += read as u64;
            assert!(
                read_bytes <= length && total <= 512 << 20,
                "producer sources changed or exceed bound"
            );
            hash.update(&buffer[..read]);
        }
        assert_eq!(read_bytes, length, "producer source changed while hashing");
    }
    println!(
        "cargo:rustc-env=MOTION_PRERENDER_PRODUCER={:x}",
        hash.finalize()
    );
}
