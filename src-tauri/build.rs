include!("src/command_names.rs");

fn main() {
    // tauri-build's resource file carries the Windows manifest (Common Controls v6, which the
    // dialog plugin imports) into the app binary only: a test binary without it dies with
    // STATUS_ENTRYPOINT_NOT_FOUND. The linker embeds it in every executable of this package
    // instead, as Tauri does for its own tests.
    let attributes = tauri_build::Attributes::new()
        .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest())
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    if let Err(error) = tauri_build::try_build(attributes) {
        panic!("tauri-build failed: {error:#}");
    }
    embed_windows_manifest();
}

fn embed_windows_manifest() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os != "windows" || target_env != "msvc" {
        return;
    }
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/windows-app-manifest.xml");
    println!("cargo:rerun-if-changed={manifest}");
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{manifest}");
}
