// Pas de console en version release sous Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod alarms;
mod app;
mod autostart;
mod config;
mod edit;
mod hotcorner;
mod hotkey;
mod icons;
mod instance;
mod launcher;
mod notes;
mod paths;
mod settings;
mod startmenu;
mod state;
mod tray;
mod window;
mod xml;

use std::path::PathBuf;
use std::sync::Arc;

/// Cherche le fichier de configuration : argument de ligne de commande,
/// sinon `PStart.xml` à côté de l'exécutable, sinon dans le dossier courant.
fn find_config() -> Option<PathBuf> {
    if let Some(arg) = std::env::args_os().nth(1) {
        return Some(PathBuf::from(arg));
    }
    let candidates = [
        std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|p| p.join("PStart.xml"))),
        std::env::current_dir().ok().map(|d| d.join("PStart.xml")),
    ];
    candidates.into_iter().flatten().find(|p| p.is_file())
}

fn main() -> eframe::Result {
    // Second lancement : on réveille la fenêtre existante et on s'arrête.
    if !instance::acquire() {
        window::activate_existing();
        return Ok(());
    }

    let file = find_config();
    let size = file
        .as_deref()
        .and_then(|f| config::Config::load(f).ok())
        .and_then(|c| c.window_size())
        .unwrap_or([460.0, 760.0]);

    let icon = icons::app_icon();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(window::TITLE)
            .with_inner_size([size[0].max(560.0), size[1]])
            .with_min_inner_size([360.0, 300.0])
            .with_drag_and_drop(true)
            .with_icon(Arc::new(eframe::egui::IconData {
                rgba: icon.rgba,
                width: icon.width,
                height: icon.height,
            })),
        ..Default::default()
    };
    eframe::run_native(
        "PStart-rs",
        options,
        Box::new(move |cc| Ok(Box::new(app::PStartApp::new(cc, file)))),
    )
}
