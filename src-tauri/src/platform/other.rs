//! Versions simplifiées pour les autres systèmes (utilisées pour tester
//! l'application hors Mac). L'application est pensée pour macOS.

use std::ffi::c_void;

pub fn clipboard_change_count() -> Option<i64> {
    None
}

pub fn clipboard_is_concealed() -> bool {
    false
}

pub fn write_text_concealed(text: &str) -> bool {
    arboard::Clipboard::new().and_then(|mut c| c.set_text(text.to_owned())).is_ok()
}

pub fn clear_clipboard() {
    if let Ok(mut c) = arboard::Clipboard::new() {
        let _ = c.clear();
    }
}

pub fn frontmost_app_pid() -> Option<i32> {
    None
}

pub fn activate_app(_pid: i32) {}

pub fn accessibility_trusted(_prompt: bool) -> bool {
    true
}

pub fn send_paste() -> bool {
    false
}

/// Pas de Touch ID hors Mac : variable COPIEP_REFUSE_AUTH=1 pour simuler un refus.
pub fn authenticate(_reason: &str) -> bool {
    std::env::var_os("COPIEP_REFUSE_AUTH").is_none()
}

#[allow(dead_code)]
pub fn setup_panel(_ns_window: *mut c_void) {}
