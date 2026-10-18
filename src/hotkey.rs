//! Raccourcis clavier globaux : celui qui affiche ou masque la fenêtre, et
//! ceux des raccourcis (`<globalhotkey>`) qui lancent un programme même
//! fenêtre masquée.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

use crate::config::{Config, EntryOptions, ItemPath};
use crate::i18n::trf;
use crate::state::Shared;
use crate::window;

pub struct Hotkeys {
    manager: GlobalHotKeyManager,
    entries: Vec<HotKey>,
    /// Version de l'arborescence au dernier enregistrement.
    built: Option<u64>,
}

/// Lit une combinaison (« Ctrl+Alt+V »).
pub fn parse(spec: &str) -> Result<HotKey, String> {
    spec.trim()
        .parse()
        .map_err(|e| trf("raccourci « {} » invalide : {}", &[&spec, &e]))
}

/// Crée le gestionnaire et enregistre le raccourci d'affichage (s'il y en a
/// un). Renvoie aussi l'erreur éventuelle de ce raccourci, sans bloquer
/// ceux des programmes.
pub fn init(main: Option<&str>, shared: Arc<Shared>) -> Result<(Hotkeys, Option<String>), String> {
    let manager = GlobalHotKeyManager::new().map_err(|e| e.to_string())?;
    let mut problem = None;
    let mut main_id = None;
    if let Some(spec) = main {
        match parse(spec).and_then(|hk| {
            manager
                .register(hk)
                .map(|()| hk.id())
                .map_err(|e| trf("raccourci « {} » indisponible : {}", &[&spec, &e]))
        }) {
            Ok(id) => main_id = Some(id),
            Err(e) => problem = Some(e),
        }
    }
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        if event.state != HotKeyState::Pressed {
            return;
        }
        if Some(event.id) == main_id {
            shared.focus_search.store(true, Ordering::SeqCst);
            window::toggle();
            shared.repaint();
            return;
        }
        let path = shared.entry_hotkeys.lock().unwrap().get(&event.id).cloned();
        if let Some(path) = path {
            shared.launch(&path);
        }
    }));
    Ok((
        Hotkeys {
            manager,
            entries: Vec::new(),
            built: None,
        },
        problem,
    ))
}

/// Raccourcis clavier déclarés dans le fichier : (combinaison, position).
pub fn declared(cfg: &Config) -> Vec<(String, ItemPath)> {
    cfg.flatten()
        .into_iter()
        .filter_map(|e| {
            let el = cfg.item(&e.path)?;
            let hk = EntryOptions::of(el).hotkey;
            (!hk.is_empty()).then_some((hk, e.path))
        })
        .collect()
}

impl Hotkeys {
    /// Réenregistre les raccourcis des programmes si l'arborescence a changé.
    /// Renvoie les problèmes rencontrés (combinaison invalide ou déjà prise).
    pub fn sync(&mut self, shared: &Shared) -> Vec<String> {
        let version = shared.structure_version.load(Ordering::SeqCst)
            + shared.generation.load(Ordering::SeqCst) * 1_000_000;
        if self.built == Some(version) {
            return Vec::new();
        }
        self.built = Some(version);
        let _ = self.manager.unregister_all(&self.entries);
        self.entries.clear();
        let wanted = match shared.config.lock().unwrap().as_ref() {
            Some(cfg) => declared(cfg),
            None => Vec::new(),
        };
        let mut map = HashMap::new();
        let mut problems = Vec::new();
        for (spec, path) in wanted {
            match parse(&spec) {
                Ok(hk) if map.contains_key(&hk.id()) => {
                    problems.push(trf("raccourci « {} » utilisé deux fois", &[&spec]));
                }
                Ok(hk) => match self.manager.register(hk) {
                    Ok(()) => {
                        map.insert(hk.id(), path);
                        self.entries.push(hk);
                    }
                    Err(e) => {
                        problems.push(trf("raccourci « {} » indisponible : {}", &[&spec, &e]))
                    }
                },
                Err(e) => problems.push(e),
            }
        }
        *shared.entry_hotkeys.lock().unwrap() = map;
        problems
    }
}
