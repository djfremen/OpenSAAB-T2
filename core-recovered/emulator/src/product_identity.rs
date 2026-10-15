// SPDX-License-Identifier: MPL-2.0
//! Program identity from packaged metadata; architecture is the compiled target.
use std::sync::LazyLock;
pub static VERSION: LazyLock<String> = LazyLock::new(|| {
    let metadata: serde_json::Value =
        serde_json::from_str(include_str!("../ui/recovered-core-identity.json"))
            .expect("packaged identity");
    let version = metadata["version"].as_str().expect("packaged version");
    assert_eq!(
        version,
        env!("CARGO_PKG_VERSION"),
        "package and program identity must agree"
    );
    version.to_owned()
});
pub fn architecture() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        other => other,
    }
}
pub fn display_name() -> &'static str {
    static NAME: LazyLock<String> = LazyLock::new(|| {
        format!(
            "OpenSAAB {} {}",
            *VERSION,
            match std::env::consts::ARCH {
                "aarch64" if cfg!(target_os = "macos") => "Apple silicon",
                "x86_64" if cfg!(target_os = "macos") => "Intel",
                _ => architecture(),
            }
        )
    });
    &NAME
}
#[cfg(test)]
mod tests {
    #[test]
    fn packaged_version_and_architecture_are_visible() {
        assert!(super::display_name().contains(env!("CARGO_PKG_VERSION")));
        assert!(!super::architecture().is_empty());
    }
}
