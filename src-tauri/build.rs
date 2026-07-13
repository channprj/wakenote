fn main() {
    // ggml-metal (via whisper-rs-sys) compiles `@available` checks that recent
    // Apple clang lowers to `___isPlatformVersionAtLeast`. That symbol lives in
    // clang's compiler-rt (`libclang_rt.osx.a`), which Rust's `-nodefaultlibs`
    // link line omits — so the final binary/test fails to link. Add it back.
    #[cfg(target_os = "macos")]
    link_clang_compiler_rt();

    tauri_build::build();
}

#[cfg(target_os = "macos")]
fn link_clang_compiler_rt() {
    use std::path::Path;
    use std::process::Command;

    let Ok(output) = Command::new("clang").arg("-print-runtime-dir").output() else {
        println!("cargo:warning=`clang -print-runtime-dir` failed; skipping compiler-rt link");
        return;
    };
    let dir = String::from_utf8_lossy(&output.stdout);
    let rt = Path::new(dir.trim()).join("libclang_rt.osx.a");
    if rt.exists() {
        // `rustc-link-arg` applies to bins, tests, and examples (not the lib
        // rlib) — exactly the targets that perform a final link.
        println!("cargo:rustc-link-arg={}", rt.display());
    } else {
        println!(
            "cargo:warning=libclang_rt.osx.a not found at {}",
            rt.display()
        );
    }
}
