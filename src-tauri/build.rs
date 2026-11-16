fn main() {
    embed_test_manifest_on_windows();
    tauri_build::build()
}

/// Windows MSVC에서 테스트 바이너리에도 앱 매니페스트를 넣는다.
///
/// Tauri는 Common Controls v6(TaskDialogIndirect 등)에 의존하는데,
/// tauri-build는 이를 선언한 매니페스트를 bin 타깃에만 포함한다.
/// 매니페스트가 없는 테스트 바이너리는 실행 즉시
/// `STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139)`로 종료되므로 직접 넣어 준다.
fn embed_test_manifest_on_windows() {
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os != "windows" || env != "msvc" {
        return;
    }
    let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("windows-test-manifest.xml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-tests=/MANIFESTINPUT:{}",
        manifest.display()
    );
}
