// SPDX-License-Identifier: MPL-2.0
use sha2::{Digest, Sha256};
use std::{env, fs, path::Path};
fn main() {
    let root = env::var("CARGO_MANIFEST_DIR").unwrap();
    let root = Path::new(&root);
    let pin = fs::read(root.join("SOURCE_SHA256")).expect("Missing shared workflow source pin");
    println!("cargo:rerun-if-changed=SOURCE_SHA256");
    for line in std::str::from_utf8(&pin).unwrap().lines() {
        let (expected, file) = line.split_once("  ").expect("Invalid source pin");
        assert!(!file.contains("..") && !Path::new(file).is_absolute(), "Invalid source path");
        let bytes = fs::read(root.join(file)).expect("Missing pinned shared source");
        assert_eq!(format!("{:x}", Sha256::digest(bytes)), expected, "Shared workflow changed: {file}; review and update the pin before building");
        println!("cargo:rerun-if-changed={file}");
    }
    let expected = fs::read_to_string(root.join("TRANSLATION_SOURCE_SHA256")).expect("Missing translation dependency pin");
    let translation = root.join("../adapter-translation/SOURCE_SHA256");
    let dependency_pin = fs::read(&translation).expect("Missing translation dependency");
    assert_eq!(format!("{:x}", Sha256::digest(&dependency_pin)), expected.trim(), "Translation dependency changed; review the workflow dependency pin");
    println!("cargo:rerun-if-changed=../adapter-translation/SOURCE_SHA256");
    println!("cargo:rustc-env=OPENSAAB_VLINKER_SOURCE_PIN={:x}", Sha256::digest(&pin));
}
