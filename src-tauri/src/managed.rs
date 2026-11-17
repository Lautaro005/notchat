//! App side of the managed llama.cpp: install with progress events, start it
//! on launch and point the AI settings at it.

use crate::assistant::KEY_AI_CONFIG;
use crate::state::AppState;
use notchat_core::ai::{AiConfig, ProviderKind};
use notchat_core::managed::{self, Paths};
use notchat_core::{Error, Result};
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

pub const EVENT_PROGRESS: &str = "managed-progress";
pub const EVENT_STATE: &str = "managed-state";

#[derive(Debug, Clone, Serialize)]
pub struct ModelView {
    pub id: String,
    pub name: String,
    pub note: String,
    pub size: u64,
    pub min_ram_gb: u32,
    pub recommended: bool,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManagedView {
    pub server_installed: bool,
    pub running_model: Option<String>,
    pub busy: bool,
    pub ram_gb: u32,
    pub models: Vec<ModelView>,
}

#[derive(Debug, Clone, Serialize)]
struct Progress {
    /// "server", "model" or "starting"
    stage: &'static str,
    done: u64,
    total: u64,
}

pub fn ram_gb() -> u32 {
    let mut size: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    // SAFETY: sysctlbyname writes at most `len` bytes into `size`.
    let rc = unsafe {
        libc::sysctlbyname(
            c"hw.memsize".as_ptr(),
            (&mut size as *mut u64).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc == 0 {
        (size / (1024 * 1024 * 1024)) as u32
    } else {
        0
    }
}

pub async fn view(state: &AppState, lang_es: bool) -> Result<ManagedView> {
    let catalog = managed::catalog()?;
    let paths = Paths::new(&state.data_dir);
    let running_model = state
        .managed
        .lock()
        .await
        .as_mut()
        .and_then(|s| s.is_running().then(|| s.model_id.clone()));
    Ok(ManagedView {
        server_installed: paths.server_installed(&catalog.llama_server),
        running_model,
        busy: state
            .managed_busy
            .load(std::sync::atomic::Ordering::Relaxed),
        ram_gb: ram_gb(),
        models: catalog
            .models
            .iter()
            .map(|m| ModelView {
                id: m.id.clone(),
                name: m.name.clone(),
                note: if lang_es {
                    m.note_es.clone()
                } else {
                    m.note_en.clone()
                },
                size: m.size,
                min_ram_gb: m.min_ram_gb,
                recommended: m.recommended,
                installed: paths.model_installed(m),
            })
            .collect(),
    })
}

/// Installs what is missing for `model_id`, starts the server and selects it.
pub async fn install_and_use(app: AppHandle, state: Arc<AppState>, model_id: String) -> Result<()> {
    use std::sync::atomic::Ordering;
    if state.managed_busy.swap(true, Ordering::SeqCst) {
        return Err(Error::Ai("an installation is already running".into()));
    }
    let _ = app.emit(EVENT_STATE, ());
    let result = run_install(&app, &state, &model_id).await;
    state.managed_busy.store(false, Ordering::SeqCst);
    let _ = app.emit(EVENT_STATE, ());
    result
}

async fn run_install(app: &AppHandle, state: &Arc<AppState>, model_id: &str) -> Result<()> {
    let catalog = managed::catalog()?;
    let model = catalog
        .models
        .iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| Error::Invalid("model".into()))?;
    let paths = Paths::new(&state.data_dir);
    let emit = |stage: &'static str| {
        let app = app.clone();
        move |done: u64, total: u64| {
            let _ = app.emit(EVENT_PROGRESS, Progress { stage, done, total });
        }
    };
    managed::install_server(&paths, &catalog.llama_server, &emit("server")).await?;
    managed::install_model(&paths, model, &emit("model")).await?;
    emit("starting")(0, 0);
    start(state, model_id).await
}

/// (Re)starts the server with `model_id` and updates the AI settings.
pub async fn start(state: &Arc<AppState>, model_id: &str) -> Result<()> {
    let catalog = managed::catalog()?;
    let model = catalog
        .models
        .iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| Error::Invalid("model".into()))?;
    let paths = Paths::new(&state.data_dir);
    let mut slot = state.managed.lock().await;
    if let Some(old) = slot.take() {
        old.stop().await;
    }
    let server = managed::start(&paths, model).await?;
    let mut cfg: AiConfig = state.store.get_json(KEY_AI_CONFIG);
    cfg.provider = Some(ProviderKind::Managed);
    cfg.base_url = server.base_url.clone();
    cfg.model = model.id.clone();
    state.store.set_json(KEY_AI_CONFIG, &cfg)?;
    *slot = Some(server);
    Ok(())
}

/// On launch: if the managed server is the selected provider, start it.
pub async fn autostart(app: AppHandle, state: Arc<AppState>) {
    let cfg: AiConfig = state.store.get_json(KEY_AI_CONFIG);
    if cfg.provider != Some(ProviderKind::Managed) || cfg.model.is_empty() {
        return;
    }
    if let Err(e) = start(&state, &cfg.model).await {
        log::warn!("managed llama.cpp did not start: {e}");
    }
    let _ = app.emit(EVENT_STATE, ());
    let _ = app.emit(crate::assistant::EVENT_AI_CHANGED, ());
}

pub async fn running_url(state: &AppState) -> Option<String> {
    state
        .managed
        .lock()
        .await
        .as_mut()
        .and_then(|s| s.is_running().then(|| s.base_url.clone()))
}
