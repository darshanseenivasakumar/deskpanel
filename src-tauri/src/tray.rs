//! Tray icon + menu (CHECKLIST J1). Left click = same as the shortcut; right click = menu.
use tauri::menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use crate::app;

pub fn build(a: &tauri::App) -> tauri::Result<()> {
    let cfg = app::ctx().cfg.lock().unwrap().clone();
    let toggle = MenuItemBuilder::with_id("toggle", "Show / hide panel").build(a)?;
    let pop = MenuItemBuilder::with_id("pop", "Pop over apps\tWin+Alt+Space").build(a)?;
    let pill = CheckMenuItemBuilder::with_id("pill", "Live pill").checked(cfg.pill_enabled).build(a)?;
    let auto = CheckMenuItemBuilder::with_id("autostart", "Start with Windows").checked(cfg.autostart).build(a)?;
    let open = MenuItemBuilder::with_id("config", "Open config file").build(a)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit DeskPanel").build(a)?;
    let menu = MenuBuilder::new(a)
        .items(&[&toggle, &pop])
        .separator()
        .items(&[&pill, &auto])
        .separator()
        .items(&[&open, &quit])
        .build()?;

    let (pill_c, auto_c) = (pill.clone(), auto.clone());
    TrayIconBuilder::with_id("deskpanel")
        .icon(a.default_window_icon().cloned().ok_or(tauri::Error::InvalidIcon(std::io::Error::other("no icon")))?)
        .tooltip("DeskPanel")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |handle, e| {
            crate::log!("tray: {}", e.id().as_ref());
            match e.id().as_ref() {
                "toggle" => app::tray_toggle(),
                "pop" => app::tray_pop(),
                "pill" => app::set_pill_enabled(pill_c.is_checked().unwrap_or(true)),
                "autostart" => app::set_autostart(auto_c.is_checked().unwrap_or(true)),
                "config" => {
                    let _ = std::process::Command::new("notepad.exe").arg(crate::config::data_dir().join("config.json")).spawn();
                }
                "quit" => {
                    crate::log!("quit from tray");
                    handle.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|_, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                app::tray_pop();
            }
        })
        .build(a)?;
    Ok(())
}
