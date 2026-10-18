//! Icône de notification avec menu en cascade (groupes → raccourcis).

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tray_icon::menu::{
    Icon as MenuIcon, IconMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem,
    Submenu,
};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::config;
use crate::icons;
use crate::state::Shared;
use crate::window;
use crate::xml::Element;

const ID_SHOW: &str = "show";
const ID_QUIT: &str = "quit";
const ENTRY_PREFIX: &str = "e:";

pub struct Tray {
    icon: TrayIcon,
    built_icons: u64,
    built_generation: u64,
}

impl Tray {
    pub fn new(shared: &Arc<Shared>) -> Result<Tray, String> {
        install_handlers(shared.clone());
        let app = icons::app_icon();
        let icon = tray_icon::Icon::from_rgba(app.rgba, app.width, app.height)
            .map_err(|e| e.to_string())?;
        let tray = TrayIconBuilder::new()
            .with_tooltip(window::TITLE)
            .with_icon(icon)
            .with_menu(Box::new(build_menu(shared)))
            .with_menu_on_left_click(false)
            .build()
            .map_err(|e| format!("icône de notification : {e}"))?;
        Ok(Tray {
            icon: tray,
            built_icons: shared.icons_version.load(Ordering::SeqCst),
            built_generation: shared.generation.load(Ordering::SeqCst),
        })
    }

    /// Reconstruit le menu si le fichier a été rechargé ou si de nouvelles
    /// icônes sont disponibles.
    pub fn refresh(&mut self, shared: &Shared) {
        let icons = shared.icons_version.load(Ordering::SeqCst);
        let generation = shared.generation.load(Ordering::SeqCst);
        if icons != self.built_icons || generation != self.built_generation {
            self.icon.set_menu(Some(Box::new(build_menu(shared))));
            self.built_icons = icons;
            self.built_generation = generation;
        }
    }
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
            _ => {
                if let Some(path) = parse_entry_id(id) {
                    shared.launch(&path);
                }
            }
        }
    }));
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
        "Afficher PStart-rs",
        true,
        None,
    ));
    let _ = menu.append(&PredefinedMenuItem::separator());

    let guard = shared.config.lock().unwrap();
    if let Some(cfg) = guard.as_ref() {
        if let Some(files) = cfg.files() {
            let icons = shared.icons.lock().unwrap();
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

    let _ = menu.append(&MenuItem::with_id(ID_QUIT, "Quitter PStart-rs", true, None));
    menu
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
                    let _ = sub.append(&MenuItem::new("(vide)", false, None));
                }
                for item in &items {
                    let _ = sub.append(item.as_ref());
                }
                out.push(Box::new(sub));
            }
            "file" => {
                let target = cfg.resolve(child);
                let icon = icons.get(&target).and_then(|img| {
                    let small = img.downscale(16);
                    MenuIcon::from_rgba(small.rgba, small.width, small.height).ok()
                });
                out.push(Box::new(IconMenuItem::with_id(
                    entry_id(path),
                    menu_label(config::display_name(child)),
                    true,
                    icon,
                    None,
                )));
            }
            _ => {}
        }
        path.pop();
    }
    out
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
