//! Update checks (at launch and every three hours, unless turned off) and
//! in-place installation of a verified release.

use crate::state::AppState;
use notchat_core::update::{self, Release};
use notchat_core::{Error, Result};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

type St<'a> = State<'a, Arc<AppState>>;

pub const KEY_UPDATE_AUTO: &str = "update_auto";
pub const EVENT_UPDATE: &str = "update-available";
const EVERY: Duration = Duration::from_secs(3 * 3600);

fn auto_enabled(state: &AppState) -> bool {
    state
        .store
        .get_setting(KEY_UPDATE_AUTO)
        .ok()
        .flatten()
        .is_none_or(|v| v != "false")
}

fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub async fn background(app: AppHandle) {
    // Let the window and the WhatsApp session start first.
    tokio::time::sleep(Duration::from_secs(20)).await;
    loop {
        let state = app.state::<Arc<AppState>>().inner().clone();
        if auto_enabled(&state) {
            match update::check(current()).await {
                Ok(Some(release)) => {
                    log::info!("update available: {}", release.version);
                    let _ = app.emit(EVENT_UPDATE, &release);
                }
                Ok(None) => log::info!("no update available"),
                Err(e) => log::warn!("update check failed: {e}"),
            }
        }
        tokio::time::sleep(EVERY).await;
    }
}

#[tauri::command]
pub fn app_version() -> String {
    current().to_string()
}

#[tauri::command]
pub fn get_update_auto(state: St<'_>) -> bool {
    auto_enabled(&state)
}

#[tauri::command]
pub fn set_update_auto(state: St<'_>, enabled: bool) -> Result<()> {
    state
        .store
        .set_setting(KEY_UPDATE_AUTO, if enabled { "true" } else { "false" })
}

#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<Release>> {
    let release = update::check(current()).await?;
    if let Some(r) = &release {
        let _ = app.emit(EVENT_UPDATE, r);
    }
    Ok(release)
}

/// The `.app` bundle this process runs from, if any.
fn bundle_path() -> Result<std::path::PathBuf> {
    let exe = std::env::current_exe()?;
    let bundle = exe
        .ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .ok_or_else(|| Error::Invalid("updates install only into the packaged app".into()))?;
    Ok(bundle.to_path_buf())
}

/// Downloads, verifies and installs `version`, then restarts Notchat.
#[tauri::command]
pub async fn install_update(app: AppHandle, state: St<'_>, version: String) -> Result<()> {
    if !update::is_newer(&version, current()) {
        return Err(Error::Invalid("version".into()));
    }
    let bundle = bundle_path()?;
    if state.update_busy.swap(true, Ordering::SeqCst) {
        return Err(Error::Invalid("an update is already installing".into()));
    }
    let result = async {
        let dir = state.data_dir.join("update");
        let _ = std::fs::remove_dir_all(&dir);
        let new_app = update::download(&version, &dir).await?;
        // Swap the bundles after this process exits, then reopen the app.
        // Paths go in as arguments, never into the script text.
        let script = r#"
while kill -0 "$1" 2>/dev/null; do sleep 0.3; done
rm -rf "$2.old"
mv "$2" "$2.old" && mv "$3" "$2" && rm -rf "$2.old"
xattr -dr com.apple.quarantine "$2" 2>/dev/null
open "$2"
"#;
        std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(script)
            .arg("notchat-update")
            .arg(std::process::id().to_string())
            .arg(&bundle)
            .arg(&new_app)
            .spawn()?;
        Ok::<_, Error>(())
    }
    .await;
    if let Err(e) = result {
        state.update_busy.store(false, Ordering::SeqCst);
        return Err(e);
    }
    log::info!("update {version} verified; restarting");
    app.exit(0);
    Ok(())
}
