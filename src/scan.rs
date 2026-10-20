//! Détection de programmes portables à ajouter : dossiers PortableApps.com
//! (fichier `App\AppInfo\appinfo.ini`), exécutables isolés et dossiers de
//! raccourcis `.lnk` / `.url` (import d'un autre lanceur ou d'un menu).

use std::collections::HashSet;
use std::path::Path;

/// Programme trouvé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub name: String,
    /// Chemin complet.
    pub path: String,
    /// Catégorie PortableApps ou sous-dossier : groupe proposé.
    pub category: Option<String>,
}

const MAX_DEPTH: usize = 4;
const MAX_RESULTS: usize = 500;

/// Exécutables qui ne sont pas des programmes à lancer.
fn is_noise(stem: &str) -> bool {
    let s = stem.to_lowercase();
    [
        "unins", "uninst", "setup", "install", "update", "updater", "crash", "helper", "report",
        "elevate", "7z", "vcredist", "dxsetup", "unzip", "service", "plugin",
    ]
    .iter()
    .any(|w| s.contains(w))
}

/// Lit une section/clé d'un fichier .ini simple.
pub fn ini_value(text: &str, section: &str, key: &str) -> Option<String> {
    let mut current = String::new();
    for line in text.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if let Some(s) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            current = s.trim().to_lowercase();
            continue;
        }
        if current != section.to_lowercase() {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if k.trim().eq_ignore_ascii_case(key) {
                let v = v.trim();
                return (!v.is_empty()).then(|| v.to_string());
            }
        }
    }
    None
}

/// Application au format PortableApps.com dans `dir`, si c'en est une.
fn portable_app(dir: &Path) -> Option<Found> {
    let ini = dir.join("App").join("AppInfo").join("appinfo.ini");
    let bytes = std::fs::read(ini).ok()?;
    let text = decode(&bytes);
    let start = ini_value(&text, "Control", "Start")?;
    let exe = dir.join(&start);
    if !exe.is_file() {
        return None;
    }
    let name = ini_value(&text, "Details", "Name").unwrap_or_else(|| stem(&exe));
    Some(Found {
        name,
        path: exe.to_string_lossy().into_owned(),
        category: ini_value(&text, "Details", "Category"),
    })
}

/// Les appinfo.ini sont en UTF-8 ou UTF-16 LE.
fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let u: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        return String::from_utf16_lossy(&u);
    }
    String::from_utf8_lossy(bytes).into_owned()
}

fn stem(p: &Path) -> String {
    p.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Parcourt `root` et renvoie les programmes absents de `known` (chemins
/// complets en minuscules).
pub fn scan(root: &Path, known: &HashSet<String>) -> Vec<Found> {
    let mut out = Vec::new();
    walk(root, root, 0, known, &mut out);
    out.sort_by(|a, b| {
        (a.category.as_deref().unwrap_or(""), a.name.to_lowercase())
            .cmp(&(b.category.as_deref().unwrap_or(""), b.name.to_lowercase()))
    });
    out
}

fn walk(root: &Path, dir: &Path, depth: usize, known: &HashSet<String>, out: &mut Vec<Found>) {
    if depth > MAX_DEPTH || out.len() >= MAX_RESULTS {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    // Sous-dossier relatif à la racine : catégorie des raccourcis importés.
    let folder = dir
        .strip_prefix(root)
        .ok()
        .and_then(|r| r.iter().next())
        .map(|s| s.to_string_lossy().into_owned());
    let mut exes = Vec::new();
    for path in entries {
        if path.is_dir() {
            let name = stem(&path).to_lowercase();
            // La plateforme PortableApps.com et les données ne sont pas des applications.
            if name == "portableapps.com" || name == "data" || name.starts_with('.') {
                continue;
            }
            if let Some(app) = portable_app(&path) {
                if !known.contains(&app.path.to_lowercase()) {
                    out.push(app);
                }
                continue; // pas de recherche à l'intérieur d'une application
            }
            walk(root, &path, depth + 1, known, out);
            continue;
        }
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let full = path.to_string_lossy().into_owned();
        if known.contains(&full.to_lowercase()) {
            continue;
        }
        match ext.as_str() {
            "lnk" | "url" if !is_noise(&stem(&path)) => out.push(Found {
                name: stem(&path),
                path: full,
                category: folder.clone(),
            }),
            "exe" if depth <= 2 && !is_noise(&stem(&path)) => exes.push(full),
            _ => {}
        }
    }
    // Dossier d'un seul programme : on garde l'exécutable principal (celui
    // qui porte le nom du dossier, sinon le premier).
    if !exes.is_empty() && depth > 0 {
        let dir_name = stem(dir).to_lowercase();
        let main = exes
            .iter()
            .find(|e| stem(Path::new(e)).to_lowercase().starts_with(&dir_name))
            .unwrap_or(&exes[0])
            .clone();
        out.push(Found {
            name: stem(Path::new(&main)),
            path: main,
            category: None,
        });
    } else {
        for e in exes {
            out.push(Found {
                name: stem(Path::new(&e)),
                path: e,
                category: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn ini() {
        let t = "[Format]\r\nType=PortableApps.comFormat\r\n[Details]\r\nName=VLC Media Player Portable\r\nCategory=Music & Video\r\n[Control]\r\nStart=VLCPortable.exe\r\n";
        assert_eq!(
            ini_value(t, "details", "name").as_deref(),
            Some("VLC Media Player Portable")
        );
        assert_eq!(
            ini_value(t, "Control", "Start").as_deref(),
            Some("VLCPortable.exe")
        );
        assert_eq!(ini_value(t, "Control", "Absent"), None);
    }

    #[test]
    fn finds_programs() {
        let root = std::env::temp_dir().join(format!("plaunch-scan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        // Application PortableApps.com.
        let vlc = root.join("VLCPortable");
        fs::create_dir_all(vlc.join("App").join("AppInfo")).unwrap();
        fs::write(
            vlc.join("App").join("AppInfo").join("appinfo.ini"),
            "[Details]\nName=VLC Portable\nCategory=Video\n[Control]\nStart=VLCPortable.exe\n",
        )
        .unwrap();
        fs::write(vlc.join("VLCPortable.exe"), b"MZ").unwrap();
        fs::write(vlc.join("App").join("vlc.exe"), b"MZ").unwrap(); // ignoré
                                                                    // Programme isolé avec son désinstalleur.
        let np = root.join("Notepad2");
        fs::create_dir_all(&np).unwrap();
        fs::write(np.join("Notepad2.exe"), b"MZ").unwrap();
        fs::write(np.join("Uninstall.exe"), b"MZ").unwrap();
        fs::write(np.join("Helper.exe"), b"MZ").unwrap();
        // Raccourcis rangés par dossier.
        let lnk = root.join("Bureautique");
        fs::create_dir_all(&lnk).unwrap();
        fs::write(lnk.join("Writer.lnk"), b"").unwrap();
        // Déjà connu.
        let known_dir = root.join("Connu");
        fs::create_dir_all(&known_dir).unwrap();
        fs::write(known_dir.join("Connu.exe"), b"MZ").unwrap();
        let known: HashSet<String> = [known_dir.join("Connu.exe").to_string_lossy().to_lowercase()]
            .into_iter()
            .collect();

        let found = scan(&root, &known);
        let names: Vec<&str> = found.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["Notepad2", "Writer", "VLC Portable"]);
        assert_eq!(found[1].category.as_deref(), Some("Bureautique"));
        assert_eq!(found[2].category.as_deref(), Some("Video"));
        let _ = fs::remove_dir_all(&root);
    }
}
