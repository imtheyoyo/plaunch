//! État partagé entre l'interface, l'icône de notification et le raccourci
//! global (ces deux derniers fonctionnent même quand la fenêtre est masquée).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use eframe::egui;

use crate::config::{self, Config};
use crate::icons::IconImage;
use crate::launcher;

#[derive(Default)]
pub struct Shared {
    pub config: Mutex<Option<Config>>,
    pub status: Mutex<String>,
    /// Icônes extraites, indexées par chemin cible résolu.
    pub icons: Mutex<HashMap<String, IconImage>>,
    /// Incrémenté à la fin de chaque extraction complète des icônes.
    pub icons_version: AtomicU64,
    /// Incrémenté à chaque (re)chargement du fichier.
    pub generation: AtomicU64,
    /// Fermeture définitive demandée (menu « Quitter »).
    pub quitting: AtomicBool,
    /// Donner le focus à la recherche au prochain affichage.
    pub focus_search: AtomicBool,
    pub ctx: OnceLock<egui::Context>,
}

impl Shared {
    pub fn set_status(&self, s: impl Into<String>) {
        *self.status.lock().unwrap() = s.into();
        self.repaint();
    }

    pub fn repaint(&self) {
        if let Some(ctx) = self.ctx.get() {
            ctx.request_repaint();
        }
    }

    /// Lance l'entrée située à `path`, met à jour ses statistiques et
    /// enregistre le fichier.
    pub fn launch(&self, path: &[usize]) {
        let msg = {
            let mut guard = self.config.lock().unwrap();
            let Some(cfg) = guard.as_mut() else { return };
            let Some(el) = cfg.item(path) else { return };
            if el.name != "file" {
                return;
            }
            let name = config::display_name(el).to_string();
            let target = cfg.resolve(el);
            let workdir = cfg.working_dir(el);
            match launcher::launch(&target, workdir.as_deref()) {
                Ok(()) => {
                    let now = chrono::Local::now().format(config::DATE_FORMAT).to_string();
                    if let Some(el) = cfg.item_mut(path) {
                        config::record_run(el, &now);
                    }
                    match cfg.save() {
                        Ok(()) => format!("Lancé : {name}"),
                        Err(e) => format!("Lancé : {name} (statistiques non enregistrées : {e})"),
                    }
                }
                Err(e) => format!("Échec : {e}"),
            }
        };
        self.set_status(msg);
    }

    pub fn next_generation(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }
}
