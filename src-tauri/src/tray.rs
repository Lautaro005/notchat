//! Menu bar icon: open the window, pause/resume auto-reply (the global kill
//! switch) and quit. Notchat keeps running when its window is closed.

use crate::autoreply::KEY_AUTO_REPLY;
use crate::state::AppState;
use notchat_core::policy::AutoReplyConfig;
use std::sync::Arc;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager};

pub const EVENT_AUTO_CHANGED: &str = "auto-changed";

fn labels(es: bool) -> (&'static str, &'static str, &'static str) {
    if es {
        ("Abrir Notchat", "Respuesta automática", "Salir de Notchat")
    } else {
        ("Open Notchat", "Auto-reply", "Quit Notchat")
    }
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let es = sys_locale_is_es();
    let (open, auto, quit) = labels(es);
    let state = app.state::<Arc<AppState>>().inner().clone();
    let cfg: AutoReplyConfig = state.store.get_json(KEY_AUTO_REPLY);

    let open_item = MenuItem::with_id(app, "open", open, true, None::<&str>)?;
    let auto_item = CheckMenuItem::with_id(app, "auto", auto, true, cfg.enabled, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", quit, true, Some("Cmd+Q"))?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open_item, &auto_item, &sep, &quit_item])?;

    let auto_for_events = auto_item.clone();
    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .icon_as_template(true)
        .tooltip("Notchat")
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "auto" => {
                let state = app.state::<Arc<AppState>>().inner().clone();
                let mut cfg: AutoReplyConfig = state.store.get_json(KEY_AUTO_REPLY);
                let ai: notchat_core::ai::AiConfig =
                    state.store.get_json(crate::assistant::KEY_AI_CONFIG);
                // Turning it on needs a model; turning it off is always allowed.
                cfg.enabled = !cfg.enabled && ai.is_ready();
                let _ = state.store.set_json(KEY_AUTO_REPLY, &cfg);
                let _ = auto_for_events.set_checked(cfg.enabled);
                let _ = app.emit(EVENT_AUTO_CHANGED, ());
            }
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = tray_icon() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    app.manage(TrayAuto(auto_item));
    Ok(())
}

/// Keeps the menu checkmark in sync when auto-reply changes elsewhere.
pub struct TrayAuto(pub CheckMenuItem<tauri::Wry>);

pub fn sync_auto(app: &AppHandle, enabled: bool) {
    if let Some(item) = app.try_state::<TrayAuto>() {
        let _ = item.0.set_checked(enabled);
    }
}

fn tray_icon() -> Option<tauri::image::Image<'static>> {
    tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))
        .ok()
        .map(|i| i.to_owned())
}

pub fn sys_locale_is_es() -> bool {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .map(|v| v.to_ascii_lowercase().starts_with("es"))
        .unwrap_or_else(apple_language_is_es)
}

fn apple_language_is_es() -> bool {
    std::process::Command::new("/usr/bin/defaults")
        .args(["read", "-g", "AppleLanguages"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| {
            s.split(['"', ',', '(', ')', '\n', ' '])
                .find(|p| !p.trim().is_empty())
                .map(|p| p.to_ascii_lowercase().starts_with("es"))
        })
        .unwrap_or(false)
}
