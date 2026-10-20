//! Temps d'utilisation des programmes, mesuré en observant les processus
//! ouverts. Stocké dans `plaunch-usage.txt` à côté de PLaunch.xml
//! (« secondes<TAB>chemin du raccourci », chemin tel qu'écrit dans le
//! fichier : il reste valable si la clé change de lettre).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::i18n::trf;

pub const FILE_NAME: &str = "plaunch-usage.txt";

pub fn path_for(config_file: &Path) -> PathBuf {
    config_file
        .parent()
        .map(|p| p.join(FILE_NAME))
        .unwrap_or_else(|| PathBuf::from(FILE_NAME))
}

/// Clé d'un raccourci : son chemin tel qu'écrit dans PLaunch.xml.
pub fn key(raw_path: &str) -> String {
    raw_path.trim().replace('/', "\\").to_lowercase()
}

pub fn parse(text: &str) -> HashMap<String, u64> {
    text.lines()
        .filter_map(|l| {
            let (secs, path) = l.split_once('\t')?;
            Some((key(path), secs.trim().parse().ok()?))
        })
        .collect()
}

pub fn to_text(map: &HashMap<String, u64>) -> String {
    let mut rows: Vec<_> = map.iter().filter(|(_, s)| **s > 0).collect();
    rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    rows.into_iter()
        .map(|(k, s)| format!("{s}\t{k}\r\n"))
        .collect()
}

pub fn load(path: &Path) -> HashMap<String, u64> {
    std::fs::read_to_string(path)
        .map(|t| parse(&t))
        .unwrap_or_default()
}

pub fn save(path: &Path, map: &HashMap<String, u64>) -> Result<(), String> {
    std::fs::write(path, to_text(map)).map_err(|e| format!("{} : {e}", path.display()))
}

/// « 2 h 05 », « 12 min », « < 1 min ».
pub fn format_duration(secs: u64) -> String {
    let minutes = secs / 60;
    if minutes == 0 {
        return "< 1 min".into();
    }
    let (h, m) = (minutes / 60, minutes % 60);
    if h == 0 {
        return trf("{} min", &[&m]);
    }
    if h >= 100 {
        return trf("{} h", &[&h]);
    }
    trf("{} h {}", &[&h, &format!("{m:02}")])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_format() {
        let mut m = HashMap::new();
        m.insert(key("Video\\VLC\\vlc.exe"), 3725);
        m.insert(key("Outils/A.exe"), 30);
        m.insert(key("vide.exe"), 0);
        let back = parse(&to_text(&m));
        assert_eq!(back.len(), 2);
        assert_eq!(back[&key("video\\vlc\\VLC.EXE")], 3725);
        assert_eq!(format_duration(30), "< 1 min");
        assert_eq!(format_duration(12 * 60), "12 min");
        assert_eq!(format_duration(3725), "1 h 02");
        assert_eq!(format_duration(150 * 3600), "150 h");
    }
}
