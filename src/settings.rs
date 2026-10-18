//! Réglages propres à PStart-rs, stockés dans `pstart-rs.ini` à côté du
//! fichier `PStart.xml` (pour ne jamais ajouter de balises inconnues de
//! PStart dans son propre fichier).

use std::fs;
use std::path::{Path, PathBuf};

use crate::hotcorner::Corner;

pub const FILE_NAME: &str = "pstart-rs.ini";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub hotkey: Option<String>,
    pub tray: bool,
    pub close_to_tray: bool,
    pub start_hidden: bool,
    pub hot_corner: Corner,
    /// Temps de présence dans la zone avant affichage (ms).
    pub hot_corner_delay: u64,
    /// Taille des coins actifs (pixels).
    pub hot_corner_size: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            hotkey: Some("Ctrl+Shift+Space".into()),
            tray: true,
            close_to_tray: true,
            start_hidden: false,
            hot_corner: Corner::None,
            hot_corner_delay: 300,
            hot_corner_size: 8,
        }
    }
}

impl Settings {
    /// Contenu complet du fichier, commentaires compris.
    pub fn to_ini(&self) -> String {
        let corners: Vec<&str> = Corner::ALL.iter().map(|c| c.key()).collect();
        format!(
            "# Réglages de PStart-rs\r\n\
             #\r\n\
             # Raccourci clavier global pour afficher ou masquer la fenêtre.\r\n\
             # Exemples : Ctrl+Shift+Space, Ctrl+Alt+P, Shift+Super+S. Laisser vide pour désactiver.\r\n\
             hotkey={}\r\n\
             \r\n\
             # Icône dans la zone de notification (true/false).\r\n\
             tray={}\r\n\
             \r\n\
             # Le bouton de fermeture masque la fenêtre au lieu de quitter (si tray=true).\r\n\
             close_to_tray={}\r\n\
             \r\n\
             # Démarrer masqué dans la zone de notification (si tray=true).\r\n\
             start_hidden={}\r\n\
             \r\n\
             # Coin ou bord actif qui affiche la fenêtre : {}.\r\n\
             hot_corner={}\r\n\
             # Délai de présence dans la zone (millisecondes) et taille des coins (pixels).\r\n\
             hot_corner_delay={}\r\n\
             hot_corner_size={}\r\n",
            self.hotkey.as_deref().unwrap_or(""),
            self.tray,
            self.close_to_tray,
            self.start_hidden,
            corners.join(", "),
            self.hot_corner.key(),
            self.hot_corner_delay,
            self.hot_corner_size,
        )
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        fs::write(path, self.to_ini()).map_err(|e| format!("{} : {e}", path.display()))
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
/// n'existe pas encore (les nouvelles options sont ajoutées au passage).
pub fn load(path: &Path) -> Settings {
    match fs::read_to_string(path) {
        Ok(text) => {
            let s = parse(&text);
            if !text.contains("hot_corner=") {
                let _ = s.save(path);
            }
            s
        }
        Err(_) => {
            // Lecteur en lecture seule : on garde simplement les défauts.
            let s = Settings::default();
            let _ = s.save(path);
            s
        }
    }
}

pub fn parse(text: &str) -> Settings {
    let mut s = Settings {
        hotkey: None,
        ..Settings::default()
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
            "hot_corner" => s.hot_corner = Corner::from_key(value),
            "hot_corner_delay" => {
                s.hot_corner_delay = value.parse().unwrap_or(s.hot_corner_delay).min(5000)
            }
            "hot_corner_size" => {
                s.hot_corner_size = value.parse().unwrap_or(s.hot_corner_size).clamp(1, 200)
            }
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
    fn ini_round_trip() {
        let s = Settings {
            hot_corner: Corner::TopLeft,
            hot_corner_delay: 500,
            hotkey: None,
            ..Default::default()
        };
        assert_eq!(parse(&s.to_ini()), s);
        assert_eq!(parse(&Settings::default().to_ini()), Settings::default());
    }

    #[test]
    fn default_hotkey_is_valid() {
        let hk = Settings::default().hotkey.unwrap();
        assert!(hk.parse::<global_hotkey::hotkey::HotKey>().is_ok());
    }
}
