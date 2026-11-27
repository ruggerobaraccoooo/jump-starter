fn main() {
    // JUMP_CHECK_ONLY=1 skips Tauri's resource step (icon embedding needs llvm-rc when
    // cross-checking the Windows code from a Mac). Real builds always run it.
    if std::env::var("JUMP_CHECK_ONLY").is_ok() {
        println!("cargo:rustc-check-cfg=cfg(desktop)");
        println!("cargo:rustc-cfg=desktop");
        println!("cargo:rustc-check-cfg=cfg(mobile)");
        println!("cargo:rustc-env=TAURI_ENV_TARGET_TRIPLE={}", std::env::var("TARGET").unwrap());
        return;
    }
    tauri_build::build()
}
