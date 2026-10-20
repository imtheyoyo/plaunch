//! Raccourcis du menu Démarrer de Windows, pour la recherche.

use std::fs;
use std::path::Path;

use crate::i18n::tr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    CurrentUser,
    AllUsers,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::CurrentUser => tr("Menu Démarrer (utilisateur)"),
            Source::AllUsers => tr("Menu Démarrer (tous)"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StartItem {
    pub name: String,
    /// Chemin complet du raccourci (`.lnk`, `.url`…).
    pub path: String,
    pub source: Source,
}

const PROGRAMS: &str = "Microsoft\\Windows\\Start Menu\\Programs";

/// Parcourt les menus Démarrer de l'utilisateur et de tous les utilisateurs.
pub fn scan() -> Vec<StartItem> {
    let mut out = Vec::new();
    let roots = [
        ("APPDATA", Source::CurrentUser),
        ("PROGRAMDATA", Source::AllUsers),
    ];
    for (var, source) in roots {
        if let Ok(base) = std::env::var(var) {
            let dir = Path::new(&base).join(PROGRAMS);
            walk(&dir, source, 0, &mut out);
        }
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    out
}

fn walk(dir: &Path, source: Source, depth: usize, out: &mut Vec<StartItem>) {
    if depth > 8 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            walk(&path, source, depth + 1, out);
            continue;
        }
        if let Some(item) = to_item(&path, source) {
            out.push(item);
        }
    }
}

fn to_item(path: &Path, source: Source) -> Option<StartItem> {
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    if !matches!(ext.as_str(), "lnk" | "url" | "appref-ms") {
        return None;
    }
    let name = path.file_stem()?.to_string_lossy().into_owned();
    if is_noise(&name) {
        return None;
    }
    Some(StartItem {
        name,
        path: path.to_string_lossy().into_owned(),
        source,
    })
}

/// Écarte les désinstalleurs et pages d'aide, comme le fait un lanceur.
fn is_noise(name: &str) -> bool {
    let n = name.to_lowercase();
    ["uninstall", "désinstall", "desinstall", "uninst"]
        .iter()
        .any(|w| n.contains(w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_shortcuts() {
        let lnk = to_item(Path::new("x/Firefox.lnk"), Source::AllUsers).unwrap();
        assert_eq!(lnk.name, "Firefox");
        assert!(to_item(Path::new("x/desktop.ini"), Source::AllUsers).is_none());
        assert!(to_item(Path::new("x/Désinstaller VLC.lnk"), Source::AllUsers).is_none());
        assert!(to_item(Path::new("x/Uninstall Foo.lnk"), Source::AllUsers).is_none());
    }

    #[test]
    fn scans_a_tree() {
        let dir = std::env::temp_dir().join(format!("plaunch-sm-{}", std::process::id()));
        let sub = dir.join(PROGRAMS.replace('\\', "/")).join("Outils");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("Notepad++.lnk"), b"").unwrap();
        fs::write(sub.join("readme.txt"), b"").unwrap();
        let mut out = Vec::new();
        walk(
            &dir.join(PROGRAMS.replace('\\', "/")),
            Source::CurrentUser,
            0,
            &mut out,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "Notepad++");
        let _ = fs::remove_dir_all(&dir);
    }
}
