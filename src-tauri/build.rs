fn main() {
    // The debug fallback in `ocr::tesseract` looks for
    // `binaries/scriptorium-tesseract-<triple>`.
    let target = std::env::var("TARGET").expect("cargo sets TARGET for build scripts");
    println!("cargo:rustc-env=SCRIPTORIUM_TARGET_TRIPLE={target}");
    tauri_build::build()
}
