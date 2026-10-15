fn main() {
 println!("cargo:rerun-if-changed=src/macos_usb.c");
 if std::env::var("CARGO_CFG_TARGET_OS").as_deref()==Ok("macos") {
  cc::Build::new().file("src/macos_usb.c").flag("-Wno-deprecated-declarations").compile("mdi_macos_usb");
  println!("cargo:rustc-link-lib=framework=IOKit");println!("cargo:rustc-link-lib=framework=CoreFoundation");
 }
}
