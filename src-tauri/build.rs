fn main() {
    // Embed our own manifest: explicit asInvoker + per-monitor-v2 DPI (Tauri's default has neither).
    let windows = tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest"));
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows)).expect("failed to run tauri-build");
}
