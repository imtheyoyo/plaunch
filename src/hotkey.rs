//! Raccourci clavier global pour afficher ou masquer la fenêtre.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

use crate::i18n::trf;
use crate::state::Shared;
use crate::window;

/// Enregistre le raccourci. Le gestionnaire renvoyé doit rester en vie.
pub fn register(spec: &str, shared: Arc<Shared>) -> Result<GlobalHotKeyManager, String> {
    let hotkey: HotKey = spec
        .parse()
        .map_err(|e| trf("raccourci « {} » invalide : {}", &[&spec, &e]))?;
    let manager = GlobalHotKeyManager::new().map_err(|e| e.to_string())?;
    let id = hotkey.id();
    manager
        .register(hotkey)
        .map_err(|e| trf("raccourci « {} » indisponible : {}", &[&spec, &e]))?;
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        if event.id == id && event.state == HotKeyState::Pressed {
            shared.focus_search.store(true, Ordering::SeqCst);
            window::toggle();
            shared.repaint();
        }
    }));
    Ok(manager)
}
