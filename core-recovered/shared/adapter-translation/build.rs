// SPDX-License-Identifier: MPL-2.0
use sha2::{Digest, Sha256};
use std::{env, fs, path::Path};
fn main() {
    let directory = env::var("CARGO_MANIFEST_DIR").unwrap();
    let root = Path::new(&directory);
    let pin = fs::read(root.join("SOURCE_SHA256")).expect("Missing adapter translation source pin");
    println!("cargo:rerun-if-changed=SOURCE_SHA256");
    let mut listed = Vec::new();
    for line in std::str::from_utf8(&pin).unwrap().lines() {
        let (expected, file) = line
            .split_once("  ")
            .expect("Invalid translation source pin");
        assert!(
            !file.contains("..") && !Path::new(file).is_absolute(),
            "Invalid source path"
        );
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(fs::read(root.join(file)).expect("Missing pinned source"))
            ),
            expected,
            "Translation source changed: {file}; review and update its pin"
        );
        listed.push(file.to_string());
        println!("cargo:rerun-if-changed={file}");
    }
    let mut required = vec!["Cargo.toml".to_string(), "build.rs".to_string()];
    required.extend(
        fs::read_dir(root.join("src"))
            .unwrap()
            .map(|e| format!("src/{}", e.unwrap().file_name().to_str().unwrap())),
    );
    listed.sort();
    required.sort();
    assert_eq!(
        listed, required,
        "Source pin must cover every translation source exactly once"
    );
    println!(
        "cargo:rustc-env=OPENSAAB_TRANSLATION_SOURCE_PIN={:x}",
        Sha256::digest(&pin)
    );
}
