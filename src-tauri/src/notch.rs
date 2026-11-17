//! The notch panel: a non-activating NSPanel pinned to the top centre of the
//! screen, over the notch when the Mac has one. It shows incoming messages
//! and lets the user answer without leaving what they are doing.
//!
//! Only public AppKit APIs are used (window level, collection behaviour,
//! `NSScreen.safeAreaInsets`).

use crate::state::AppState;
use notchat_core::store::Message;
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl};
use tauri_nspanel::objc2::MainThreadMarker;
use tauri_nspanel::objc2_app_kit::NSScreen;
use tauri_nspanel::objc2_foundation::{NSPoint, NSRect, NSSize};
use tauri_nspanel::{CollectionBehavior, ManagerExt, PanelBuilder, PanelLevel, StyleMask};

pub const LABEL: &str = "notch";
pub const EVENT_NOTCH: &str = "notch-message";
const WIDTH: f64 = 400.0;
const BODY_COMPACT: f64 = 64.0;
const BODY_EXPANDED: f64 = 132.0;
/// Top inset used on screens without a notch, where the panel is a pill
/// floating just under the top edge.
const PILL_INSET: f64 = 8.0;

/// The macro brings its own imports, so it gets a module to itself.
mod panel {
    use tauri_nspanel::tauri_panel;

    tauri_panel! {
        panel!(NotchPanel {
            config: {
                can_become_key_window: true,
                can_become_main_window: false,
                is_floating_panel: true
            }
        })
    }
}
use panel::NotchPanel;

#[derive(Debug, Clone, Serialize)]
pub struct NotchMessage {
    pub chat_jid: String,
    pub title: String,
    pub sender: String,
    pub text: String,
    pub kind: String,
    pub is_group: bool,
    /// Height of the hardware notch in points; 0 when the screen has none.
    pub notch_height: f64,
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let panel = PanelBuilder::<_, NotchPanel>::new(app, LABEL)
        .url(WebviewUrl::App("index.html#notch".into()))
        .size(tauri::Size::Logical(tauri::LogicalSize::new(
            WIDTH,
            BODY_COMPACT,
        )))
        .level(PanelLevel::Status)
        .style_mask(StyleMask::empty().borderless().nonactivating_panel())
        .collection_behavior(
            CollectionBehavior::new()
                .can_join_all_spaces()
                .stationary()
                .full_screen_auxiliary()
                .ignores_cycle(),
        )
        .has_shadow(true)
        .hides_on_deactivate(false)
        .no_activate(true)
        .corner_radius(20.0)
        .with_window(|w| {
            w.decorations(false)
                .resizable(false)
                .skip_taskbar(true)
                .visible(false)
                .shadow(true)
        })
        .build()?;
    panel.hide();
    Ok(())
}

/// The screen the panel belongs on and the height of its notch (0 if none).
/// Prefers a screen that has a notch; otherwise the main screen.
fn target_screen(mtm: MainThreadMarker) -> Option<(NSRect, f64)> {
    let screens = NSScreen::screens(mtm);
    let mut fallback = None;
    for screen in screens.iter() {
        let top = screen.safeAreaInsets().top;
        if top > 0.0 {
            return Some((screen.frame(), top));
        }
        if fallback.is_none() {
            fallback = Some((screen.frame(), 0.0));
        }
    }
    NSScreen::mainScreen(mtm)
        .map(|s| (s.frame(), 0.0))
        .or(fallback)
}

fn place(app: &AppHandle, expanded: bool, make_key: bool) -> Option<f64> {
    let mtm = MainThreadMarker::new()?;
    let panel = app.get_webview_panel(LABEL).ok()?;
    let (screen, notch) = target_screen(mtm)?;
    let inset = if notch > 0.0 { notch } else { PILL_INSET };
    let body = if expanded {
        BODY_EXPANDED
    } else {
        BODY_COMPACT
    };
    // With a notch the panel starts at the very top so it reads as the notch
    // growing; without one it floats just below the edge.
    let (height, top_gap) = if notch > 0.0 {
        (inset + body, 0.0)
    } else {
        (body, inset)
    };
    // AppKit coordinates: origin at the bottom-left of the screen.
    let frame = NSRect::new(
        NSPoint::new(
            screen.origin.x + (screen.size.width - WIDTH) / 2.0,
            screen.origin.y + screen.size.height - height - top_gap,
        ),
        NSSize::new(WIDTH, height),
    );
    panel.as_panel().setFrame_display(frame, true);
    if make_key {
        panel.show_and_make_key();
    } else {
        panel.order_front_regardless();
    }
    Some(notch)
}

/// Shows an incoming message in the panel.
pub fn notify(app: &AppHandle, state: &Arc<AppState>, message: &Message, is_group: bool) {
    let chat = state.store.get_chat(&message.chat_jid).ok().flatten();
    let title = chat
        .as_ref()
        .map(notchat_core::agent::chat_label)
        .unwrap_or_else(|| notchat_core::wa::user_part(&message.chat_jid).to_string());
    let mut payload = NotchMessage {
        chat_jid: message.chat_jid.clone(),
        title,
        sender: if is_group {
            message.sender_name.clone()
        } else {
            String::new()
        },
        text: message.text.chars().take(240).collect(),
        kind: message.kind.clone(),
        is_group,
        notch_height: 0.0,
    };
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(notch) = place(&handle, false, false) {
            payload.notch_height = notch;
            let _ = handle.emit_to(LABEL, EVENT_NOTCH, payload);
        }
    });
}

pub fn set_expanded(app: &AppHandle, expanded: bool) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        place(&handle, expanded, expanded);
    });
}

pub fn hide(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Ok(panel) = handle.get_webview_panel(LABEL) {
            panel.hide();
        }
    });
}

/// Brings the main window forward on a given chat.
pub fn open_chat(app: &AppHandle, jid: &str) {
    hide(app);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        let _ = window.emit("open-chat", jid);
    }
}
