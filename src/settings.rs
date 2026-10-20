//! Réglages de PLaunch, stockés dans `plaunch.ini` à côté du fichier
//! `PLaunch.xml` (le fichier des raccourcis ne contient que les raccourcis).

use std::fs;
use std::path::{Path, PathBuf};

use crate::alarms::ReminderStyle;
use crate::hotcorner::Corner;
use crate::i18n::{tr, Language};
use crate::icons::{self, IconColor, IconImage, IconShape};

pub const FILE_NAME: &str = "plaunch.ini";

/// Thème de l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::System, Theme::Light, Theme::Dark];

    pub fn key(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Theme::System => tr("Système"),
            Theme::Light => tr("Clair"),
            Theme::Dark => tr("Sombre"),
        }
    }

    pub fn from_key(s: &str) -> Theme {
        match s.trim().to_ascii_lowercase().as_str() {
            "light" | "clair" => Theme::Light,
            "dark" | "sombre" => Theme::Dark,
            _ => Theme::System,
        }
    }
}

/// Bornes de la taille du texte (pourcentage).
pub const TEXT_SIZE_RANGE: std::ops::RangeInclusive<u32> = 70..=200;

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
    pub language: Language,
    pub theme: Theme,
    /// Taille du texte et de l'interface, en pourcentage.
    pub text_size: u32,
    pub icon_shape: IconShape,
    pub icon_color: IconColor,
    /// Icône personnalisée (.ico, .exe…), relative au dossier du fichier.
    pub icon_file: Option<String>,
    /// Le coin actif fonctionne sur chaque écran (sinon l'écran principal).
    pub hot_corner_all_screens: bool,
    pub reminders: ReminderStyle,
    /// Fenêtre réduite à la recherche et à la liste.
    pub compact: bool,
    /// Vérifier les mises à jour au démarrage.
    pub check_updates: bool,
    /// Lettre du lecteur au dernier lancement (détection des changements).
    pub last_drive: Option<char>,
    /// Dossier où chercher de nouveaux programmes (relatif accepté).
    pub scan_folder: Option<String>,
    /// Signaler les nouveaux programmes de ce dossier au démarrage.
    pub scan_on_start: bool,
    /// Programmes trouvés que l'utilisateur ne veut pas ajouter.
    pub scan_ignored: Vec<String>,
    /// Ramener la fenêtre d'un programme déjà ouvert au lieu de le relancer.
    pub reuse_running: bool,
    /// Mesurer le temps d'utilisation des programmes.
    pub track_usage: bool,
    /// Dernier profil ouvert (nom de fichier, dans ce dossier).
    pub last_profile: Option<String>,
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
            language: Language::Auto,
            theme: Theme::System,
            text_size: 100,
            icon_shape: IconShape::Play,
            icon_color: IconColor::Blue,
            icon_file: None,
            hot_corner_all_screens: true,
            reminders: ReminderStyle::Toast,
            compact: false,
            check_updates: true,
            last_drive: None,
            scan_folder: None,
            scan_on_start: false,
            scan_ignored: Vec::new(),
            reuse_running: true,
            track_usage: true,
            last_profile: None,
        }
    }
}

impl Settings {
    /// Contenu complet du fichier, commentaires compris.
    pub fn to_ini(&self) -> String {
        let corners: Vec<&str> = Corner::ALL.iter().map(|c| c.key()).collect();
        let join = |keys: Vec<&str>| keys.join(", ");
        format!(
            "# Réglages de PLaunch\r\n\
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
             hot_corner_size={}\r\n\
             # Coin actif sur tous les écrans (sinon l'écran principal uniquement).\r\n\
             hot_corner_all_screens={}\r\n\
             \r\n\
             # Rappels des notes : {}.\r\n\
             reminders={}\r\n\
             \r\n\
             # Mode compact (recherche et liste seulement) ; vérification des mises à jour.\r\n\
             compact={}\r\n\
             check_updates={}\r\n\
             \r\n\
             # Lettre du lecteur au dernier lancement (géré automatiquement).\r\n\
             last_drive={}\r\n\
             \r\n\
             # Dossier surveillé pour les nouveaux programmes portables (ex. PortableApps),\r\n\
             # signalement au démarrage, et programmes ignorés (séparés par |).\r\n\
             scan_folder={}\r\n\
             scan_on_start={}\r\n\
             scan_ignored={}\r\n\
             \r\n\
             # Programme déjà ouvert : ramener sa fenêtre au lieu d'en ouvrir une autre.\r\n\
             reuse_running={}\r\n\
             # Mesurer le temps d'utilisation des programmes (plaunch-usage.txt).\r\n\
             track_usage={}\r\n\
             \r\n\
             # Dernier profil ouvert (PLaunch.xml, PLaunch-Travail.xml…).\r\n\
             last_profile={}\r\n\
             \r\n\
             # Langue de l'interface : {}.\r\n\
             language={}\r\n\
             \r\n\
             # Thème : {}. Taille du texte en pourcentage ({}-{}).\r\n\
             theme={}\r\n\
             text_size={}\r\n\
             \r\n\
             # Icône de la fenêtre et de la zone de notification.\r\n\
             # Forme : {}. Couleur : {}.\r\n\
             icon_shape={}\r\n\
             icon_color={}\r\n\
             # Ou bien une icône prise dans un fichier .ico ou .exe (chemin relatif accepté).\r\n\
             icon_file={}\r\n",
            self.hotkey.as_deref().unwrap_or(""),
            self.tray,
            self.close_to_tray,
            self.start_hidden,
            corners.join(", "),
            self.hot_corner.key(),
            self.hot_corner_delay,
            self.hot_corner_size,
            self.hot_corner_all_screens,
            join(ReminderStyle::ALL.iter().map(|l| l.key()).collect()),
            self.reminders.key(),
            self.compact,
            self.check_updates,
            self.last_drive.map(String::from).unwrap_or_default(),
            self.scan_folder.as_deref().unwrap_or(""),
            self.scan_on_start,
            self.scan_ignored.join("|"),
            self.reuse_running,
            self.track_usage,
            self.last_profile.as_deref().unwrap_or(""),
            join(Language::ALL.iter().map(|l| l.key()).collect()),
            self.language.key(),
            join(Theme::ALL.iter().map(|l| l.key()).collect()),
            TEXT_SIZE_RANGE.start(),
            TEXT_SIZE_RANGE.end(),
            self.theme.key(),
            self.text_size,
            join(IconShape::ALL.iter().map(|l| l.key()).collect()),
            join(IconColor::ALL.iter().map(|l| l.key()).collect()),
            self.icon_shape.key(),
            self.icon_color.key(),
            self.icon_file.as_deref().unwrap_or(""),
        )
    }

    /// Chemin complet de l'icône personnalisée (relatif au dossier du
    /// fichier de réglages, pour rester portable).
    pub fn icon_path(&self, settings_file: Option<&Path>) -> Option<String> {
        let f = self.icon_file.as_deref()?.trim();
        if f.is_empty() {
            return None;
        }
        if Path::new(f).is_absolute() || crate::paths::has_drive(f) || f.starts_with("\\\\") {
            return Some(f.to_string());
        }
        let dir = settings_file.and_then(Path::parent)?;
        Some(dir.join(f).to_string_lossy().into_owned())
    }

    /// Icône de la fenêtre et de la zone de notification.
    pub fn icon(&self, settings_file: Option<&Path>) -> IconImage {
        icons::custom_icon(
            self.icon_shape,
            self.icon_color,
            self.icon_path(settings_file).as_deref(),
        )
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        fs::write(path, self.to_ini()).map_err(|e| format!("{} : {e}", path.display()))
    }
}

/// Chemin du fichier de réglages associé à un `PLaunch.xml`.
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
            if !text.contains("track_usage=") {
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
            "language" => s.language = Language::from_key(value),
            "theme" => s.theme = Theme::from_key(value),
            "text_size" => {
                s.text_size = value
                    .trim_end_matches('%')
                    .parse()
                    .unwrap_or(s.text_size)
                    .clamp(*TEXT_SIZE_RANGE.start(), *TEXT_SIZE_RANGE.end())
            }
            "icon_shape" => s.icon_shape = IconShape::from_key(value),
            "icon_color" => s.icon_color = IconColor::from_key(value),
            "icon_file" => s.icon_file = (!value.is_empty()).then(|| value.to_string()),
            "hot_corner_all_screens" => {
                s.hot_corner_all_screens = boolean(value, s.hot_corner_all_screens)
            }
            "reminders" => s.reminders = ReminderStyle::from_key(value),
            "compact" => s.compact = boolean(value, s.compact),
            "check_updates" => s.check_updates = boolean(value, s.check_updates),
            "scan_folder" => s.scan_folder = (!value.is_empty()).then(|| value.to_string()),
            "scan_on_start" => s.scan_on_start = boolean(value, s.scan_on_start),
            "reuse_running" => s.reuse_running = boolean(value, s.reuse_running),
            "track_usage" => s.track_usage = boolean(value, s.track_usage),
            "last_profile" => s.last_profile = (!value.is_empty()).then(|| value.to_string()),
            "scan_ignored" => {
                s.scan_ignored = value
                    .split('|')
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(str::to_string)
                    .collect()
            }
            "last_drive" => {
                s.last_drive = value
                    .chars()
                    .next()
                    .filter(char::is_ascii_alphabetic)
                    .map(|c| c.to_ascii_uppercase())
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
        let s = Settings {
            language: Language::English,
            theme: Theme::Dark,
            text_size: 125,
            icon_shape: IconShape::Bolt,
            icon_color: IconColor::Purple,
            icon_file: Some("_Icones\\plaunch.ico".into()),
            hot_corner_all_screens: false,
            reminders: ReminderStyle::Both,
            compact: true,
            check_updates: false,
            last_drive: Some('F'),
            scan_folder: Some("PortableApps".into()),
            scan_on_start: true,
            reuse_running: false,
            track_usage: false,
            last_profile: Some("PLaunch-Travail.xml".into()),
            scan_ignored: vec!["E:\\A\\a.exe".into(), "E:\\B\\b.exe".into()],
            ..Default::default()
        };
        assert_eq!(parse(&s.to_ini()), s);
    }

    #[test]
    fn appearance_values() {
        let s = parse("theme=Sombre\nlanguage=EN\ntext_size=500%\nicon_color=nope\n");
        assert_eq!(s.theme, Theme::Dark);
        assert_eq!(s.language, Language::English);
        assert_eq!(s.text_size, 200);
        assert_eq!(s.icon_color, IconColor::Blue);
        assert_eq!(parse("text_size=10").text_size, 70);
    }

    #[test]
    fn default_hotkey_is_valid() {
        let hk = Settings::default().hotkey.unwrap();
        assert!(hk.parse::<global_hotkey::hotkey::HotKey>().is_ok());
    }
}
