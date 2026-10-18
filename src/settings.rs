//! Réglages propres à PStart-rs, stockés dans `pstart-rs.ini` à côté du
//! fichier `PStart.xml` (pour ne jamais ajouter de balises inconnues de
//! PStart dans son propre fichier).

use std::fs;
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "pstart-rs.ini";

const DEFAULT_FILE: &str = "\
# Réglages de PStart-rs
#
# Raccourci clavier global pour afficher ou masquer la fenêtre.
# Exemples : Ctrl+Shift+Space, Ctrl+Alt+P, Shift+Super+S. Laisser vide pour désactiver.
hotkey=Ctrl+Shift+Space

# Icône dans la zone de notification (true/false).
tray=true

# Le bouton de fermeture masque la fenêtre au lieu de quitter (si tray=true).
close_to_tray=true

# Démarrer masqué dans la zone de notification (si tray=true).
start_hidden=false
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub hotkey: Option<String>,
    pub tray: bool,
    pub close_to_tray: bool,
    pub start_hidden: bool,
}

impl Default for Settings {
    fn default() -> Self {
        parse(DEFAULT_FILE)
    }
}

/// Chemin du fichier de réglages associé à un `PStart.xml`.
pub fn path_for(config_file: &Path) -> PathBuf {
    config_file
        .parent()
        .map(|p| p.join(FILE_NAME))
        .unwrap_or_else(|| PathBuf::from(FILE_NAME))
}

/// Lit les réglages ; crée le fichier avec les valeurs par défaut s'il
/// n'existe pas encore.
pub fn load(path: &Path) -> Settings {
    match fs::read_to_string(path) {
        Ok(text) => parse(&text),
        Err(_) => {
            // Lecteur en lecture seule : on garde simplement les défauts.
            let _ = fs::write(path, DEFAULT_FILE.replace('\n', "\r\n"));
            Settings::default()
        }
    }
}

pub fn parse(text: &str) -> Settings {
    let mut s = Settings {
        hotkey: None,
        tray: true,
        close_to_tray: true,
        start_hidden: false,
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim().to_ascii_lowercase().as_str() {
            "hotkey" => s.hotkey = (!value.is_empty()).then(|| value.to_string()),
            "tray" => s.tray = boolean(value, s.tray),
            "close_to_tray" => s.close_to_tray = boolean(value, s.close_to_tray),
            "start_hidden" => s.start_hidden = boolean(value, s.start_hidden),
            _ => {}
        }
    }
    s
}

fn boolean(v: &str, default: bool) -> bool {
    match v.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "oui" | "on" => true,
        "false" | "0" | "no" | "non" | "off" => false,
        _ => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let s = Settings::default();
        assert_eq!(s.hotkey.as_deref(), Some("Ctrl+Shift+Space"));
        assert!(s.tray && s.close_to_tray && !s.start_hidden);
    }

    #[test]
    fn parse_values() {
        let s = parse("hotkey=\r\ntray = non\r\n; commentaire\r\nstart_hidden=1\r\n");
        assert_eq!(s.hotkey, None);
        assert!(!s.tray);
        assert!(s.start_hidden);
        assert!(s.close_to_tray);
    }

    #[test]
    fn default_hotkey_is_valid() {
        let hk = Settings::default().hotkey.unwrap();
        assert!(hk.parse::<global_hotkey::hotkey::HotKey>().is_ok());
    }
}
