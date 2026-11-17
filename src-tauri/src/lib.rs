mod assistant;
mod autoreply;
mod commands;
mod logger;
mod managed;
mod notch;
mod notify;
mod session;
mod state;
mod sysnotify;
mod tray;

use notchat_core::keychain;
use notchat_core::store::Store;
use state::{AppState, Link, Status};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

fn load_key() -> notchat_core::Result<String> {
    // Development builds are re-signed on every compile, which would trigger
    // a Keychain prompt each run. Debug builds may take the key from the
    // environment instead; release builds always use the Keychain.
    #[cfg(debug_assertions)]
    if let Ok(key) = std::env::var("NOTCHAT_DEV_KEY") {
        if keychain::is_valid_key(&key) {
            return Ok(key);
        }
    }
    keychain::db_key()
}

fn init_state(app: &tauri::App) -> Result<AppState, Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    // Keep development data apart from a real install (different key source).
    #[cfg(debug_assertions)]
    let data_dir = data_dir.join("dev");
    std::fs::create_dir_all(&data_dir)?;
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o700))?;
        // Tighten files left by older versions (e.g. SQLite WAL/SHM files).
        for entry in std::fs::read_dir(&data_dir)?.flatten() {
            if entry.file_type().is_ok_and(|t| t.is_file()) {
                let _ =
                    std::fs::set_permissions(entry.path(), std::fs::Permissions::from_mode(0o600));
            }
        }
    }
    let key = load_key()?;
    let store = Arc::new(Store::open(&data_dir.join("notchat.db"), &key)?);
    Ok(AppState {
        store,
        key,
        data_dir,
        session: tokio::sync::Mutex::new(None),
        status: Mutex::new(Status::default()),
        rate: Mutex::default(),
        pending_auto: Mutex::default(),
        ai_task: Mutex::new(None),
        managed: tokio::sync::Mutex::new(None),
        managed_busy: Default::default(),
    })
}

pub fn run() {
    // Every file Notchat creates (databases, WAL/SHM, downloads) is private
    // to the user, including the ones opened by dependencies.
    // SAFETY: umask only changes this process's file-creation mask.
    unsafe {
        libc::umask(0o077);
    }
    logger::init();
    let result = tauri::Builder::default()
        .plugin(tauri_nspanel::init())
        .setup(|app| {
            let state = Arc::new(init_state(app)?);
            app.manage(state);
            if let Err(e) = tray::create(app.handle()) {
                log::error!("menu bar icon unavailable: {e}");
            }
            if let Err(e) = notch::create(app.handle()) {
                log::error!("notch panel unavailable: {e}");
            }
            tauri::async_runtime::spawn(managed::autostart(
                app.handle().clone(),
                app.state::<Arc<AppState>>().inner().clone(),
            ));
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = session::start(handle.clone(), None).await {
                    log::error!("session start failed: {e}");
                    let state = handle.state::<Arc<AppState>>();
                    let status = state.update_status(|s| {
                        s.link = Link::Error;
                        s.error = Some(e.to_string());
                    });
                    let _ = handle.emit(session::EVENT_STATUS, status);
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the main window hides it; Notchat keeps receiving
            // messages from the menu bar. Quit from the menu bar icon.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::start_session,
            commands::list_chats,
            commands::list_messages,
            commands::mark_read,
            commands::send_text,
            commands::logout,
            commands::detect_providers,
            commands::get_ai_config,
            commands::set_ai_config,
            commands::ai_history,
            commands::ai_send,
            commands::ai_stop,
            commands::ai_clear,
            commands::ai_resolve_action,
            commands::get_auto_reply,
            commands::set_auto_reply,
            commands::set_chat_auto_reply,
            commands::set_chat_instructions,
            commands::get_notify_mode,
            commands::set_notify_mode,
            commands::notch_expand,
            commands::notch_hide,
            commands::notch_open_chat,
            commands::notch_test,
            commands::suggest_reply,
            commands::enable_system_notifications,
            commands::managed_view,
            commands::managed_install,
        ])
        .build(tauri::generate_context!());
    match result {
        Ok(app) => app.run(|handle, event| {
            // Clicking the Dock icon brings the hidden window back.
            if let tauri::RunEvent::Reopen { .. } = event {
                tray::show_main(handle);
            }
        }),
        Err(e) => {
            eprintln!("notchat failed to start: {e}");
            std::process::exit(1);
        }
    }
}
