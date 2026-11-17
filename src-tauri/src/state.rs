use notchat_core::policy::RateWindow;
use notchat_core::store::Store;
use notchat_core::wa::WaSession;
use serde::Serialize;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Link {
    #[default]
    Starting,
    Pairing,
    Connected,
    Disconnected,
    LoggedOut,
    Error,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Status {
    pub link: Link,
    pub qr_svg: Option<String>,
    pub pair_code: Option<String>,
    pub error: Option<String>,
}

pub struct AppState {
    pub store: Arc<Store>,
    pub key: String,
    pub data_dir: PathBuf,
    pub session: tokio::sync::Mutex<Option<WaSession>>,
    pub status: Mutex<Status>,
    /// Auto-replies sent per chat in the last hour.
    pub rate: Mutex<RateWindow>,
    /// Chats with an auto-reply currently scheduled.
    pub pending_auto: Mutex<HashSet<String>>,
    /// The running assistant turn, so it can be stopped.
    pub ai_task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    /// Notchat's own llama.cpp server, when running.
    pub managed: tokio::sync::Mutex<Option<notchat_core::managed::Server>>,
    pub managed_busy: std::sync::atomic::AtomicBool,
}

impl AppState {
    pub fn status(&self) -> Status {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn update_status(&self, f: impl FnOnce(&mut Status)) -> Status {
        let mut guard = self.status.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut guard);
        guard.clone()
    }

    pub fn session_db(&self) -> PathBuf {
        self.data_dir.join("session.db")
    }
}
