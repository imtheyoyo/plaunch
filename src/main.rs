// Pas de console en version release sous Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod alarms;
mod app;
mod autostart;
mod backup;
mod config;
mod drivecheck;
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
mod menutheme;
mod monitor;
mod notes;
mod paths;
mod profiles;
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

/// Premier argument de la ligne de commande, hors options du serveur MCP
/// (`--mcp…`, voir docs/MCP.md).
fn first_arg() -> Option<std::ffi::OsString> {
    std::env::args_os()
        .skip(1)
        .find(|a| !a.to_str().is_some_and(|s| s.starts_with("--mcp")))
}

/// Cherche le fichier de configuration : argument de ligne de commande,
/// sinon `PLaunch.xml` à côté de l'exécutable, sinon dans le dossier courant.
fn find_config() -> Option<PathBuf> {
    if let Some(arg) = first_arg() {
        return Some(PathBuf::from(arg));
    }
    let candidates = [
        std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|p| p.join("PLaunch.xml"))),
        std::env::current_dir().ok().map(|d| d.join("PLaunch.xml")),
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
    // Passerelle stdio pour les clients MCP qui lancent un programme
    // (Claude Desktop) : relaie vers l'instance pilotable, lancée au besoin.
    #[cfg(feature = "mcp")]
    if args.iter().any(|a| a == "--mcp-stdio") {
        let mcp_args = args.iter().map(|a| {
            if a == "--mcp-stdio" {
                "--mcp"
            } else {
                a.as_str()
            }
        });
        // Port par défaut de PLaunch : 47800 (egui_mcp::DEFAULT_PORT).
        let (port, token) =
            egui_mcp::parse_args(mcp_args).unwrap_or((egui_mcp::DEFAULT_PORT, None));
        let launch = || {
            let mut cmd = std::process::Command::new(std::env::current_exe()?);
            cmd.arg(format!("--mcp={port}"));
            if let Some(t) = &token {
                cmd.arg(format!("--mcp-token={t}"));
            }
            cmd.stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map(|_| ())
        };
        let _ = egui_mcp::stdio::run_bridge(port, token.as_deref(), launch);
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
        if args.iter().any(|a| a.starts_with("--mcp")) {
            log::error("mode MCP : une instance de PLaunch est déjà ouverte, la fermer d'abord");
        }
        window::activate_existing();
        return Ok(());
    }
    update::cleanup();
    eject::cleanup_helper();
    log::info(&format!(
        "démarrage de PLaunch {} ({})",
        env!("CARGO_PKG_VERSION"),
        std::env::current_exe()
            .map(|e| e.display().to_string())
            .unwrap_or_default()
    ));

    let mut file = find_config();
    let settings_path = file.as_deref().map(settings::path_for);
    let settings = settings_path
        .as_deref()
        .map(settings::load)
        .unwrap_or_default();
    // Avant la création du menu de notification.
    menutheme::apply(settings.theme);
    // Sans fichier demandé : dernier profil utilisé dans ce dossier.
    if first_arg().is_none() {
        if let (Some(f), Some(last)) = (&file, &settings.last_profile) {
            let candidate = f.with_file_name(last);
            if candidate.is_file() && profiles::is_profile(&candidate) {
                file = Some(candidate);
            }
        }
    }
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
        "PLaunch",
        options,
        Box::new(move |cc| Ok(Box::new(app::PLaunchApp::new(cc, file, settings)))),
    )
}
