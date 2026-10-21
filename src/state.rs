//! État partagé entre l'interface, l'icône de notification et le raccourci
//! global (ces deux derniers fonctionnent même quand la fenêtre est masquée).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use eframe::egui;

use crate::config::{self, Config};
use crate::hotcorner::Corner;
use crate::i18n::trf;
use crate::icons::IconImage;
use crate::launcher;
use crate::notes::Notes;
use crate::startmenu::StartItem;
use crate::xml::Element;

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
    /// Incrémenté à chaque modification de l'arborescence.
    pub structure_version: AtomicU64,
    /// Raccourcis du menu Démarrer (pour la recherche).
    pub start_menu: Mutex<Vec<StartItem>>,
    /// Coin actif : zone, délai (ms), taille (px), sur tous les écrans.
    pub hot_corner: Mutex<(Corner, u64, i32, bool)>,
    /// Forme des rappels.
    pub reminders: Mutex<crate::alarms::ReminderStyle>,
    /// Mise à jour intégrée.
    pub update: Mutex<crate::update::State>,
    /// Notes et alarmes.
    pub notes: Mutex<Option<Notes>>,
    /// Afficher la vue Notes au prochain affichage (menu de l'icône).
    pub show_notes: AtomicBool,
    /// Fermeture définitive demandée (menu « Quitter »).
    pub quitting: AtomicBool,
    /// Donner le focus à la recherche au prochain affichage.
    pub focus_search: AtomicBool,
    pub ctx: OnceLock<egui::Context>,
    /// États précédents de l'arborescence (annuler / rétablir).
    pub undo: Mutex<Vec<Element>>,
    pub redo: Mutex<Vec<Element>>,
    /// PLaunch.xml modifié par un autre programme : à recharger.
    pub file_changed: AtomicBool,
    /// Profil choisi dans le menu de l'icône, à ouvrir.
    pub switch_profile: Mutex<Option<std::path::PathBuf>>,
    /// Éjection demandée depuis le menu de l'icône.
    pub ask_eject: AtomicBool,
    /// Programmes ouverts : exécutable (clé `eject::path_key`) → processus.
    pub running: Mutex<HashMap<String, Vec<u32>>>,
    /// Temps d'utilisation (secondes) par chemin de raccourci.
    pub usage: Mutex<HashMap<String, u64>>,
    pub usage_dirty: AtomicBool,
    /// Ramener la fenêtre d'un programme déjà ouvert au lieu de le relancer.
    pub reuse_running: AtomicBool,
    pub track_usage: AtomicBool,
    /// Raccourcis clavier globaux des raccourcis : identifiant → position.
    pub entry_hotkeys: Mutex<HashMap<u32, Vec<usize>>>,
}

/// Nombre d'étapes d'annulation conservées.
const UNDO_DEPTH: usize = 50;

impl Shared {
    /// Message d'erreur : barre d'état et journal.
    pub fn error(&self, s: impl Into<String>) {
        let s = s.into();
        crate::log::error(&s);
        self.set_status(s);
    }

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
        self.launch_mode(path, false);
    }

    /// Nom et processus de l'entrée située à `path`, si elle tourne déjà.
    pub fn running_entry(&self, path: &[usize]) -> Option<(String, Vec<u32>)> {
        let guard = self.config.lock().unwrap();
        let cfg = guard.as_ref()?;
        let el = cfg.item(path).filter(|e| e.name == "file")?;
        let target = crate::eject::path_key(&cfg.resolve(el));
        let pids = self.running.lock().unwrap().get(&target).cloned()?;
        Some((config::display_name(el).to_string(), pids))
    }

    /// Lance l'entrée ; si elle tourne déjà (et sauf `force_new`), ramène
    /// sa fenêtre au premier plan.
    pub fn launch_mode(&self, path: &[usize], force_new: bool) {
        if !force_new && self.reuse_running.load(Ordering::SeqCst) {
            if let Some((name, pids)) = self.running_entry(path) {
                if crate::eject::activate(&pids) {
                    self.set_status(trf("Déjà ouvert : {}", &[&name]));
                    return;
                }
            }
        }
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
            let opts = config::EntryOptions::of(el);
            let how = launcher::Launch {
                params: Some(&opts.parameters),
                workdir: workdir.as_deref(),
                window: opts.window,
                admin: opts.admin,
            };
            match launcher::launch_with(&target, &how) {
                Ok(()) => {
                    let now = chrono::Local::now().format(config::DATE_FORMAT).to_string();
                    if let Some(el) = cfg.item_mut(path) {
                        config::record_run(el, &now);
                    }
                    match cfg.save() {
                        Ok(()) => trf("Lancé : {}", &[&name]),
                        Err(e) => trf(
                            "Lancé : {} (statistiques non enregistrées : {})",
                            &[&name, &e],
                        ),
                    }
                }
                Err(e) => {
                    crate::log::error(&format!("lancement de {target} : {e}"));
                    trf("Échec : {}", &[&e])
                }
            }
        };
        self.set_status(msg);
    }

    /// Applique une modification à la configuration puis l'enregistre.
    /// En cas d'erreur, rien n'est enregistré et le message est affiché.
    pub fn modify<R>(&self, f: impl FnOnce(&mut Config) -> Result<R, String>) -> Option<R> {
        let result = {
            let mut guard = self.config.lock().unwrap();
            let cfg = guard.as_mut()?;
            let before = cfg.root.clone();
            match f(cfg).and_then(|r| cfg.save().map(|()| r)) {
                Ok(r) => {
                    let mut undo = self.undo.lock().unwrap();
                    undo.push(before);
                    if undo.len() > UNDO_DEPTH {
                        undo.remove(0);
                    }
                    self.redo.lock().unwrap().clear();
                    Ok(r)
                }
                Err(e) => {
                    // Rien n'est gardé d'une modification qui a échoué.
                    cfg.root = before;
                    Err(e)
                }
            }
        };
        match result {
            Ok(r) => {
                self.structure_version.fetch_add(1, Ordering::SeqCst);
                self.repaint();
                Some(r)
            }
            Err(e) => {
                self.error(trf("Modification impossible : {}", &[&e]));
                None
            }
        }
    }

    /// Annule (`redo = false`) ou rétablit la dernière modification.
    pub fn undo_redo(&self, redo: bool) -> bool {
        let (from, to) = if redo {
            (&self.redo, &self.undo)
        } else {
            (&self.undo, &self.redo)
        };
        let Some(state) = from.lock().unwrap().pop() else {
            return false;
        };
        let result = {
            let mut guard = self.config.lock().unwrap();
            let Some(cfg) = guard.as_mut() else {
                return false;
            };
            let current = std::mem::replace(&mut cfg.root, state);
            match cfg.save() {
                Ok(()) => {
                    to.lock().unwrap().push(current);
                    Ok(())
                }
                Err(e) => {
                    let state = std::mem::replace(&mut cfg.root, current);
                    from.lock().unwrap().push(state);
                    Err(e)
                }
            }
        };
        match result {
            Ok(()) => {
                self.structure_version.fetch_add(1, Ordering::SeqCst);
                self.repaint();
                true
            }
            Err(e) => {
                self.error(e);
                false
            }
        }
    }

    /// Lettre du lecteur de PLaunch.xml s'il peut être éjecté.
    pub fn eject_drive(&self) -> Option<char> {
        let guard = self.config.lock().unwrap();
        let file = guard.as_ref()?.file.to_string_lossy().into_owned();
        let drive = crate::paths::drive_letter(&file)?;
        crate::eject::can_eject(drive).then_some(drive)
    }

    /// Oublie l'historique (nouveau fichier chargé).
    pub fn clear_history(&self) {
        self.undo.lock().unwrap().clear();
        self.redo.lock().unwrap().clear();
    }

    /// Lance un fichier externe (résultat du menu Démarrer), sans statistiques.
    pub fn launch_external(&self, name: &str, path: &str) {
        let workdir = crate::paths::parent(path);
        match launcher::launch(path, workdir.as_deref()) {
            Ok(()) => self.set_status(trf("Lancé : {}", &[&name])),
            Err(e) => self.error(trf("Échec : {}", &[&e])),
        }
    }

    pub fn next_generation(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }
}
