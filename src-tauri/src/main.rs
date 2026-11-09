#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod autostart;
mod broker;
mod config;
mod controls;
mod desktop;
mod log;
mod occlusion;
mod panelwnd;
mod tray;
mod wallpaper;

fn main() {
    log::init();
    // Release builds abort on panic; make sure the reason lands in the log first.
    std::panic::set_hook(Box::new(|info| crate::log!("PANIC: {info}")));
    // Remember who had focus before any of our windows exist (focus guard, CHECKLIST B6).
    app::seed_foreground(desktop::foreground());
    tauri::Builder::default()
        // Must be the first plugin: a second launch is forwarded here and just pops the panel.
        .plugin(tauri_plugin_single_instance::init(|_, _, _| app::on_second_instance()))
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            app::ready,
            app::anim_done,
            app::esc,
            app::close_panel,
            app::get_glass,
            app::get_geom,
            app::panel_state,
            app::fit_height,
            app::frame_stats,
            app::js_log,
            app::drag_start,
            app::get_backdrop,
            app::pill_region,
            app::panel_pressed,
            app::is_panel_visible,
            app::is_pill_shown,
            controls::ctl,
            controls::get_controls,
            controls::get_stats,
            controls::get_media_art,
        ])
        .setup(|a| app::setup(a))
        .run(tauri::generate_context!())
        .expect("error while running DeskPanel");
}
