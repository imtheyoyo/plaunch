// Pas de console en version release sous Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod alarms;
mod app;
mod autostart;
mod backup;
mod config;
mod edit;
mod eject;
mod fuzzy;
mod hotcorner;
mod hotkey;
mod i18n;
mod icons;
mod instance;
mod launcher;
mod log;
mod monitor;
mod notes;
mod paths;
mod scan;
mod settings;
mod startmenu;
mod state;
mod tray;
mod update;
mod usage;
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
    log::install_panic_hook();
    // Assistant d'éjection lancé depuis le dossier temporaire.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some((drive, pid)) = eject::parse_helper_args(&args) {
        eject::run_helper(drive, pid);
        return Ok(());
    }
    // Après une mise à jour, l'ancienne instance met un instant à se fermer.
    let restarted = std::env::var_os(update::RESTART_ENV).is_some();
    let mut acquired = instance::acquire();
    for _ in 0..50 {
        if acquired || !restarted {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
        acquired = instance::acquire();
    }
    if !acquired {
        window::activate_existing();
        return Ok(());
    }
    update::cleanup();
    eject::cleanup_helper();
    log::info(&format!(
        "démarrage de PStart-rs {} ({})",
        env!("CARGO_PKG_VERSION"),
        std::env::current_exe()
            .map(|e| e.display().to_string())
            .unwrap_or_default()
    ));

    let file = find_config();
    let settings_path = file.as_deref().map(settings::path_for);
    let settings = settings_path
        .as_deref()
        .map(settings::load)
        .unwrap_or_default();
    i18n::set_language(settings.language);

    let size = file
        .as_deref()
        .and_then(|f| config::Config::load(f).ok())
        .and_then(|c| c.window_size())
        .unwrap_or([460.0, 760.0]);

    let icon = settings.icon(settings_path.as_deref());
    let width = if settings.compact {
        app::COMPACT_WIDTH
    } else {
        size[0].max(560.0)
    };
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(window::title())
            .with_inner_size([width, size[1]])
            .with_min_inner_size([300.0, 300.0])
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
        Box::new(move |cc| Ok(Box::new(app::PStartApp::new(cc, file, settings)))),
    )
}
