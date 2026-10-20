//! Profils : plusieurs fichiers de raccourcis côte à côte
//! (`PLaunch.xml`, `PLaunch-Travail.xml`, `PLaunch-Maison.xml`…). Les
//! réglages, notes et temps d'utilisation du dossier sont communs.

use std::path::{Path, PathBuf};

use crate::i18n::tr;

pub const MAIN: &str = "PLaunch.xml";
const PREFIX: &str = "PLaunch-";

/// Profils présents dans `dir`, le profil principal en premier.
pub fn list(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_profile(p))
        .collect();
    out.sort_by_key(|p| (!is_main(p), display_name(p).to_lowercase()));
    out
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn is_main(p: &Path) -> bool {
    file_name(p).eq_ignore_ascii_case(MAIN)
}

/// `PLaunch.xml` ou `PLaunch-<nom>.xml`.
pub fn is_profile(p: &Path) -> bool {
    let name = file_name(p);
    let lower = name.to_lowercase();
    if !lower.ends_with(".xml") {
        return false;
    }
    is_main(p) || (lower.starts_with(&PREFIX.to_lowercase()) && name.len() > PREFIX.len() + 4)
}

/// « Principal » ou le nom du profil (`PLaunch-Travail.xml` → « Travail »).
pub fn display_name(p: &Path) -> String {
    if is_main(p) {
        return tr("Principal").to_string();
    }
    let name = file_name(p);
    let stem = &name[..name.len() - 4];
    stem.get(PREFIX.len()..).unwrap_or(stem).to_string()
}

/// Fichier d'un nouveau profil (caractères interdits remplacés).
pub fn file_for(dir: &Path, name: &str) -> Option<PathBuf> {
    let clean: String = name
        .trim()
        .chars()
        .map(|c| {
            if "\\/:*?\"<>|".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim_matches(['.', ' ']).to_string();
    (!clean.is_empty()).then(|| dir.join(format!("{PREFIX}{clean}.xml")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_listing() {
        let dir = std::env::temp_dir().join(format!("plaunch-prof-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for f in [
            "PLaunch.xml",
            "PLaunch-Travail.xml",
            "plaunch-maison.xml",
            "PLaunch.xml.bak",
            "autre.xml",
        ] {
            std::fs::write(dir.join(f), "<start></start>").unwrap();
        }
        let names: Vec<String> = list(&dir).iter().map(|p| display_name(p)).collect();
        assert_eq!(names, ["Principal", "maison", "Travail"]);
        assert_eq!(
            file_for(&dir, " Jeux / vidéo ")
                .unwrap()
                .file_name()
                .unwrap(),
            "PLaunch-Jeux _ vidéo.xml"
        );
        assert!(file_for(&dir, "  ").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
