//! Icône de notification avec menu en cascade (groupes → raccourcis).

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tray_icon::menu::{
    CheckMenuItem, Icon as MenuIcon, IconMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem,
    PredefinedMenuItem, Submenu,
};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::autostart;
use crate::config;
use crate::i18n::{tr, trf};
use crate::icons;
use crate::state::Shared;
use crate::window;
use crate::xml::Element;

const ID_SHOW: &str = "show";
const ID_QUIT: &str = "quit";
const ID_NOTES: &str = "notes";
const ID_AUTOSTART: &str = "autostart";
const ID_EJECT: &str = "eject";
const ENTRY_PREFIX: &str = "e:";
const PROFILE_PREFIX: &str = "p:";

pub struct Tray {
    icon: TrayIcon,
    built_icons: u64,
    built_generation: u64,
    built_structure: u64,
}

impl Tray {
    pub fn new(shared: &Arc<Shared>, app: &icons::IconImage) -> Result<Tray, String> {
        install_handlers(shared.clone());
        let icon = to_tray_icon(app)?;
        let tray = TrayIconBuilder::new()
            .with_tooltip(window::title())
            .with_icon(icon)
            .with_menu(Box::new(build_menu(shared)))
            .with_menu_on_left_click(false)
            .build()
            .map_err(|e| trf("icône de notification : {}", &[&e]))?;
        Ok(Tray {
            icon: tray,
            built_icons: shared.icons_version.load(Ordering::SeqCst),
            built_generation: shared.generation.load(Ordering::SeqCst),
            built_structure: shared.structure_version.load(Ordering::SeqCst),
        })
    }

    /// Change l'icône affichée dans la zone de notification.
    pub fn set_icon(&self, app: &icons::IconImage) -> Result<(), String> {
        self.icon
            .set_icon(Some(to_tray_icon(app)?))
            .map_err(|e| e.to_string())
    }

    /// Reconstruit le menu si le fichier a été rechargé ou si de nouvelles
    /// icônes sont disponibles.
    pub fn refresh(&mut self, shared: &Shared) {
        let icons = shared.icons_version.load(Ordering::SeqCst);
        let generation = shared.generation.load(Ordering::SeqCst);
        let structure = shared.structure_version.load(Ordering::SeqCst);
        if icons != self.built_icons
            || generation != self.built_generation
            || structure != self.built_structure
        {
            self.icon.set_menu(Some(Box::new(build_menu(shared))));
            // La langue a pu changer.
            let _ = self.icon.set_tooltip(Some(window::title()));
            self.built_icons = icons;
            self.built_generation = generation;
            self.built_structure = structure;
        }
    }
}

fn to_tray_icon(app: &icons::IconImage) -> Result<tray_icon::Icon, String> {
    tray_icon::Icon::from_rgba(app.rgba.clone(), app.width, app.height).map_err(|e| e.to_string())
}

fn install_handlers(shared: Arc<Shared>) {
    let s = shared.clone();
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            s.focus_search.store(true, Ordering::SeqCst);
            window::toggle();
            s.repaint();
        }
    }));
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let id = event.id.0.as_str();
        match id {
            ID_SHOW => {
                shared.focus_search.store(true, Ordering::SeqCst);
                window::show();
                shared.repaint();
            }
            ID_QUIT => quit(&shared),
            ID_NOTES => {
                shared.show_notes.store(true, Ordering::SeqCst);
                window::show();
                shared.repaint();
            }
            ID_AUTOSTART => toggle_autostart(&shared),
            ID_EJECT => {
                shared.ask_eject.store(true, Ordering::SeqCst);
                window::show();
                shared.repaint();
            }
            _ => {
                if let Some(name) = id.strip_prefix(PROFILE_PREFIX) {
                    let file = shared
                        .config
                        .lock()
                        .unwrap()
                        .as_ref()
                        .map(|c| c.file.with_file_name(name));
                    *shared.switch_profile.lock().unwrap() = file;
                    shared.repaint();
                } else if let Some(path) = parse_entry_id(id) {
                    launch_from_menu(&shared, path);
                }
            }
        }
    }));
}

/// Lancement depuis le menu : si le programme tourne déjà, on demande s'il
/// faut en ouvrir une autre instance ou afficher celle qui est ouverte.
fn launch_from_menu(shared: &Arc<Shared>, path: Vec<usize>) {
    let Some((name, pids)) = shared.running_entry(&path) else {
        shared.launch_mode(&path, true);
        return;
    };
    let shared = shared.clone();
    // Boîte modale dans son propre thread : la boucle de l'interface et du
    // menu continue de tourner.
    std::thread::spawn(move || match ask_another_instance(&name) {
        Choice::NewInstance => shared.launch_mode(&path, true),
        Choice::ShowExisting => {
            if crate::eject::activate(&pids) {
                shared.set_status(trf("Déjà ouvert : {}", &[&name]));
            }
        }
        Choice::Cancel => {}
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
enum Choice {
    NewInstance,
    ShowExisting,
    Cancel,
}

#[cfg(windows)]
fn ask_another_instance(name: &str) -> Choice {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDNO, IDYES, MB_DEFBUTTON2, MB_ICONQUESTION, MB_SETFOREGROUND, MB_TOPMOST,
        MB_YESNOCANCEL,
    };
    let text = format!(
        "{}\n\n{}\n{}\n{}",
        trf("« {} » est déjà ouvert.", &[&name]),
        tr("Oui : lancer une autre instance"),
        tr("Non : afficher la fenêtre déjà ouverte"),
        tr("Annuler : ne rien faire"),
    );
    let text: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let caption: Vec<u16> = "PLaunch".encode_utf16().chain(Some(0)).collect();
    // SAFETY : chaînes terminées par un zéro ; fenêtre parente nulle.
    let r = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_YESNOCANCEL | MB_ICONQUESTION | MB_DEFBUTTON2 | MB_TOPMOST | MB_SETFOREGROUND,
        )
    };
    match r {
        IDYES => Choice::NewInstance,
        IDNO => Choice::ShowExisting,
        _ => Choice::Cancel,
    }
}

#[cfg(not(windows))]
fn ask_another_instance(_name: &str) -> Choice {
    Choice::ShowExisting
}

/// Quitte proprement : la fenêtre reçoit WM_CLOSE, ce qui laisse eframe
/// s'arrêter et retirer l'icône. Filet de sécurité si rien ne se passe.
pub fn quit(shared: &Shared) {
    shared.quitting.store(true, Ordering::SeqCst);
    shared.repaint();
    if !window::request_close() {
        std::process::exit(0);
    }
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(3));
        std::process::exit(0);
    });
}

fn build_menu(shared: &Shared) -> Menu {
    let menu = Menu::new();
    let _ = menu.append(&MenuItem::with_id(
        ID_SHOW,
        tr("Afficher PLaunch"),
        true,
        None,
    ));
    let _ = menu.append(&PredefinedMenuItem::separator());

    let guard = shared.config.lock().unwrap();
    if let Some(cfg) = guard.as_ref() {
        if let Some(files) = cfg.files() {
            let icons = shared.icons.lock().unwrap();
            // Favoris en tête de menu.
            let favorites = cfg.favorites();
            for e in &favorites {
                let _ = menu.append(&IconMenuItem::with_id(
                    entry_id(&e.path),
                    menu_label(&format!("★ {}", e.name)),
                    true,
                    menu_icon(&icons, &e.target),
                    None,
                ));
            }
            if !favorites.is_empty() {
                let _ = menu.append(&PredefinedMenuItem::separator());
            }
            let mut path = Vec::new();
            let items = build_items(cfg, files, &mut path, &icons);
            for item in &items {
                let _ = menu.append(item.as_ref());
            }
            if !items.is_empty() {
                let _ = menu.append(&PredefinedMenuItem::separator());
            }
        }
    }
    drop(guard);

    // Profils (s'il y en a plusieurs).
    let current = shared
        .config
        .lock()
        .unwrap()
        .as_ref()
        .map(|c| c.file.clone());
    if let Some(cur) = &current {
        let dir = cur.parent().map(|d| d.to_path_buf()).unwrap_or_default();
        let list = crate::profiles::list(&dir);
        if list.len() > 1 {
            let sub = Submenu::new(tr("Profil"), true);
            for p in &list {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let _ = sub.append(&CheckMenuItem::with_id(
                    format!("{PROFILE_PREFIX}{name}"),
                    menu_label(&crate::profiles::display_name(p)),
                    true,
                    p == cur,
                    None,
                ));
            }
            let _ = menu.append(&sub);
        }
    }
    let _ = menu.append(&MenuItem::with_id(ID_NOTES, tr("Notes…"), true, None));
    let _ = menu.append(&CheckMenuItem::with_id(
        ID_AUTOSTART,
        tr("Démarrer avec Windows"),
        true,
        autostart::is_enabled(),
        None,
    ));
    if let Some(drive) = shared.eject_drive() {
        let _ = menu.append(&MenuItem::with_id(
            ID_EJECT,
            trf("Éjecter la clé ({}:)…", &[&drive]),
            true,
            None,
        ));
    }
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&MenuItem::with_id(
        ID_QUIT,
        tr("Quitter PLaunch"),
        true,
        None,
    ));
    menu
}

fn toggle_autostart(shared: &Shared) {
    let enable = !autostart::is_enabled();
    let file = shared
        .config
        .lock()
        .unwrap()
        .as_ref()
        .map(|c| c.file.clone());
    match autostart::set(enable, file.as_deref()) {
        Ok(()) if enable => shared.set_status(tr("Démarrage avec Windows activé")),
        Ok(()) => shared.set_status(tr("Démarrage avec Windows désactivé")),
        Err(e) => shared.set_status(trf("Démarrage avec Windows : {}", &[&e])),
    }
    // Reconstruit le menu pour refléter l'état réel.
    shared.structure_version.fetch_add(1, Ordering::SeqCst);
}

fn build_items(
    cfg: &config::Config,
    el: &Element,
    path: &mut Vec<usize>,
    icons: &std::collections::HashMap<String, icons::IconImage>,
) -> Vec<Box<dyn IsMenuItem>> {
    let mut out: Vec<Box<dyn IsMenuItem>> = Vec::new();
    for (i, child) in el.children.iter().enumerate() {
        path.push(i);
        match child.name.as_str() {
            "files" => {
                let sub = Submenu::new(menu_label(config::display_name(child)), true);
                let items = build_items(cfg, child, path, icons);
                if items.is_empty() {
                    let _ = sub.append(&MenuItem::new(tr("(vide)"), false, None));
                }
                for item in &items {
                    let _ = sub.append(item.as_ref());
                }
                out.push(Box::new(sub));
            }
            "separator" => out.push(Box::new(PredefinedMenuItem::separator())),
            "file" => {
                let target = cfg.resolve(child);
                out.push(Box::new(IconMenuItem::with_id(
                    entry_id(path),
                    menu_label(config::display_name(child)),
                    true,
                    menu_icon(icons, &target),
                    None,
                )));
            }
            _ => {}
        }
        path.pop();
    }
    out
}

fn menu_icon(
    icons: &std::collections::HashMap<String, icons::IconImage>,
    target: &str,
) -> Option<MenuIcon> {
    icons.get(target).and_then(|img| {
        let small = img.downscale(16);
        MenuIcon::from_rgba(small.rgba, small.width, small.height).ok()
    })
}

/// Sous Windows, `&` introduit un raccourci clavier dans les menus.
fn menu_label(name: &str) -> String {
    name.replace('&', "&&")
}

fn entry_id(path: &[usize]) -> String {
    let parts: Vec<String> = path.iter().map(|i| i.to_string()).collect();
    format!("{ENTRY_PREFIX}{}", parts.join("."))
}

fn parse_entry_id(id: &str) -> Option<Vec<usize>> {
    let rest = id.strip_prefix(ENTRY_PREFIX)?;
    rest.split('.').map(|p| p.parse().ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_ids_round_trip() {
        let id = entry_id(&[3, 0, 12]);
        assert_eq!(id, "e:3.0.12");
        assert_eq!(parse_entry_id(&id), Some(vec![3, 0, 12]));
        assert_eq!(parse_entry_id("show"), None);
        assert_eq!(parse_entry_id("e:1.x"), None);
    }

    #[test]
    fn ampersands_are_escaped() {
        assert_eq!(menu_label("Tag&Rename"), "Tag&&Rename");
    }
}
