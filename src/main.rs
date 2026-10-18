// Pas de console en version release sous Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod launcher;
mod paths;
mod xml;

use std::path::PathBuf;

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
    let file = find_config();
    let size = file
        .as_deref()
        .and_then(|f| config::Config::load(f).ok())
        .and_then(|c| c.window_size())
        .unwrap_or([460.0, 760.0]);

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("PStart-rs")
            .with_inner_size([size[0].max(560.0), size[1]])
            .with_min_inner_size([360.0, 300.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "PStart-rs",
        options,
        Box::new(move |_cc| Ok(Box::new(app::PStartApp::new(file)))),
    )
}
