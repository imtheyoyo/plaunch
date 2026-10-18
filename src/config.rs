//! Lecture et écriture d'un fichier `PStart.xml`.
//!
//! Le document complet est conservé sous forme d'arbre XML générique : seules
//! les valeurs que l'utilisateur modifie (compteurs de lancement…) changent,
//! tout le reste est réécrit à l'identique.

use std::fs;
use std::path::{Path, PathBuf};

use crate::paths;
use crate::xml::{self, Element};

/// Position d'un élément dans l'arbre, en indices d'enfants depuis `<files>`.
pub type ItemPath = Vec<usize>;

/// Format de date utilisé par PStart dans `<rundate>`.
pub const DATE_FORMAT: &str = "%Y/%m/%d %H:%M:%S";

pub struct Config {
    pub root: Element,
    pub file: PathBuf,
    /// Dossier de référence des chemins relatifs (format Windows).
    pub base_dir: String,
    backed_up: bool,
}

/// Une entrée aplatie (pour la recherche et les classements).
#[derive(Debug, Clone)]
pub struct FlatEntry {
    pub path: ItemPath,
    /// Noms des groupes parents.
    pub groups: Vec<String>,
    pub name: String,
    pub target: String,
    pub runcount: u32,
    pub rundate: Option<String>,
}

impl Config {
    pub fn load(file: &Path) -> Result<Config, String> {
        let bytes = fs::read(file).map_err(|e| format!("{} : {e}", file.display()))?;
        let text = decode(&bytes);
        let root = xml::parse(&text).map_err(|e| format!("{} : {e}", file.display()))?;
        if root.name != "start" {
            return Err(format!(
                "{} n'est pas un fichier PStart (racine <{}>)",
                file.display(),
                root.name
            ));
        }
        let base_dir = file
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(Config {
            root,
            file: file.to_path_buf(),
            base_dir,
            backed_up: false,
        })
    }

    /// Construit une configuration en mémoire (tests).
    #[cfg(test)]
    pub fn from_str(text: &str, base_dir: &str) -> Result<Config, String> {
        let root = xml::parse(text).map_err(|e| e.to_string())?;
        Ok(Config {
            root,
            file: PathBuf::new(),
            base_dir: base_dir.to_string(),
            backed_up: true,
        })
    }

    pub fn to_xml(&self) -> String {
        xml::write(&self.root)
    }

    /// Enregistre le fichier. Au premier enregistrement de la session, une
    /// copie de l'original est conservée dans `PStart.xml.bak`.
    pub fn save(&mut self) -> Result<(), String> {
        let data = self.to_xml();
        if !self.backed_up && self.file.exists() {
            let bak = with_suffix(&self.file, ".bak");
            fs::copy(&self.file, &bak)
                .map_err(|e| format!("sauvegarde {} : {e}", bak.display()))?;
            self.backed_up = true;
        }
        let tmp = with_suffix(&self.file, ".tmp");
        fs::write(&tmp, data.as_bytes()).map_err(|e| format!("{} : {e}", tmp.display()))?;
        fs::rename(&tmp, &self.file).map_err(|e| format!("{} : {e}", self.file.display()))?;
        Ok(())
    }

    /// Élément racine des raccourcis (`<start><files>`).
    pub fn files(&self) -> Option<&Element> {
        self.root.child("files")
    }

    pub fn item(&self, path: &[usize]) -> Option<&Element> {
        let mut cur = self.files()?;
        for &i in path {
            cur = cur.children.get(i)?;
        }
        Some(cur)
    }

    pub fn item_mut(&mut self, path: &[usize]) -> Option<&mut Element> {
        let mut cur = self.root.child_mut("files")?;
        for &i in path {
            cur = cur.children.get_mut(i)?;
        }
        Some(cur)
    }

    /// Chemin complet résolu d'une entrée.
    pub fn resolve(&self, entry: &Element) -> String {
        paths::resolve(entry_path(entry), &self.base_dir)
    }

    /// Dossier de travail d'une entrée : `<directory>` s'il est renseigné,
    /// sinon le dossier de la cible.
    pub fn working_dir(&self, entry: &Element) -> Option<String> {
        match entry.child_text("directory") {
            Some(d) if !d.trim().is_empty() => Some(paths::resolve(d, &self.base_dir)),
            _ => paths::parent(&self.resolve(entry)),
        }
    }

    /// Toutes les entrées, dans l'ordre du fichier.
    pub fn flatten(&self) -> Vec<FlatEntry> {
        let mut out = Vec::new();
        if let Some(files) = self.files() {
            let mut path = Vec::new();
            let mut groups = Vec::new();
            self.walk(files, &mut path, &mut groups, &mut out);
        }
        out
    }

    fn walk(
        &self,
        el: &Element,
        path: &mut ItemPath,
        groups: &mut Vec<String>,
        out: &mut Vec<FlatEntry>,
    ) {
        for (i, child) in el.children.iter().enumerate() {
            path.push(i);
            match child.name.as_str() {
                "files" => {
                    groups.push(display_name(child).to_string());
                    self.walk(child, path, groups, out);
                    groups.pop();
                }
                "file" => out.push(FlatEntry {
                    path: path.clone(),
                    groups: groups.clone(),
                    name: display_name(child).to_string(),
                    target: self.resolve(child),
                    runcount: runcount(child),
                    rundate: child.child_text("rundate").map(str::to_string),
                }),
                _ => {}
            }
            path.pop();
        }
    }

    /// Option booléenne `True`/`False` de PStart.
    pub fn flag(&self, path: &[&str]) -> Option<bool> {
        self.root
            .get(path)
            .map(|e| e.text.trim().eq_ignore_ascii_case("true"))
    }

    pub fn number(&self, path: &[&str]) -> Option<i64> {
        self.root.get(path).and_then(|e| e.text.trim().parse().ok())
    }

    /// Lancement en un seul clic (`<general><singleclick>`).
    pub fn single_click(&self) -> bool {
        self.flag(&["general", "singleclick"]).unwrap_or(false)
    }

    /// Taille de fenêtre mémorisée par PStart.
    pub fn window_size(&self) -> Option<[f32; 2]> {
        let w = self.number(&["window", "main", "size", "x"])?;
        let h = self.number(&["window", "main", "size", "y"])?;
        (w > 100 && h > 100).then_some([w as f32, h as f32])
    }
}

/// Nom affiché d'un groupe ou d'une entrée (attribut `name`, espaces retirés).
pub fn display_name(el: &Element) -> &str {
    el.attr("name").map(str::trim).unwrap_or("(sans nom)")
}

pub fn entry_path(el: &Element) -> &str {
    el.child_text("path").unwrap_or("")
}

pub fn runcount(el: &Element) -> u32 {
    el.child_text("runcount")
        .and_then(|t| t.trim().parse().ok())
        .unwrap_or(0)
}

/// Met à jour `<runcount>` et `<rundate>` après un lancement, en respectant
/// l'ordre des balises de PStart (`path`, `directory`, …, `runcount`, `rundate`).
pub fn record_run(el: &mut Element, now: &str) {
    let count = runcount(el) + 1;
    set_child(el, "runcount", &count.to_string(), &["rundate"]);
    set_child(el, "rundate", now, &[]);
}

/// Remplace le texte d'un enfant, ou l'insère avant le premier des enfants
/// `before` présents (sinon à la fin).
fn set_child(el: &mut Element, name: &str, value: &str, before: &[&str]) {
    if let Some(c) = el.child_mut(name) {
        c.text = value.to_string();
        return;
    }
    let pos = el
        .children
        .iter()
        .position(|c| before.contains(&c.name.as_str()))
        .unwrap_or(el.children.len());
    el.children.insert(pos, Element::leaf(name, value));
}

/// Décode le fichier : UTF-8 (avec ou sans BOM), UTF-16 LE/BE, sinon
/// Windows-1252 (cas des anciens fichiers non ASCII).
fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let u: Vec<u16> = rest
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&u);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let u: Vec<u16> = rest
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&u);
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| cp1252(b)).collect(),
    }
}

fn cp1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž',
        '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}',
        'ž', 'Ÿ',
    ];
    match b {
        0x80..=0x9F => HIGH[(b - 0x80) as usize],
        _ => b as char,
    }
}

fn with_suffix(p: &Path, suffix: &str) -> PathBuf {
    let mut s = p.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../tests/fixtures/PStart.sample.xml");

    #[test]
    fn round_trip_is_byte_identical() {
        let cfg = Config::from_str(SAMPLE, "E:\\Portable").unwrap();
        assert_eq!(cfg.to_xml(), SAMPLE);
    }

    #[test]
    fn flatten_entries() {
        let cfg = Config::from_str(SAMPLE, "E:\\Portable").unwrap();
        let all = cfg.flatten();
        let vlc = all.iter().find(|e| e.name == "VLC media player").unwrap();
        assert_eq!(vlc.groups, vec!["Vidéo".to_string()]);
        assert_eq!(vlc.target, "E:\\Portable\\Video\\VLC\\vlc.exe");
        assert_eq!(vlc.runcount, 62);
        let nested = all.iter().find(|e| e.name == "Tag&Rename").unwrap();
        assert_eq!(
            nested.groups,
            vec!["Musique".to_string(), "Tags".to_string()]
        );
    }

    #[test]
    fn record_run_updates_and_inserts() {
        let mut cfg = Config::from_str(SAMPLE, "E:\\Portable").unwrap();
        let all = cfg.flatten();
        let never = all.iter().find(|e| e.runcount == 0).unwrap().path.clone();
        let el = cfg.item_mut(&never).unwrap();
        record_run(el, "2026/10/18 18:00:00");
        let names: Vec<&str> = el.children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names.last(), Some(&"rundate"));
        assert_eq!(runcount(el), 1);
        record_run(el, "2026/10/18 18:05:00");
        assert_eq!(runcount(el), 2);
        assert_eq!(el.child_text("rundate"), Some("2026/10/18 18:05:00"));
    }

    #[test]
    fn working_dir_prefers_directory_tag() {
        let cfg = Config::from_str(SAMPLE, "E:\\Portable").unwrap();
        let all = cfg.flatten();
        let iso = all.iter().find(|e| e.name == "PowerISO").unwrap();
        let el = cfg.item(&iso.path).unwrap();
        assert_eq!(
            cfg.working_dir(el).as_deref(),
            Some("C:\\Program Files (x86)\\PowerISO")
        );
        let vlc = all.iter().find(|e| e.name == "VLC media player").unwrap();
        let el = cfg.item(&vlc.path).unwrap();
        assert_eq!(
            cfg.working_dir(el).as_deref(),
            Some("E:\\Portable\\Video\\VLC")
        );
    }

    #[test]
    fn settings() {
        let cfg = Config::from_str(SAMPLE, "E:\\Portable").unwrap();
        assert!(!cfg.single_click());
        assert_eq!(cfg.window_size(), Some([451.0, 823.0]));
    }

    #[test]
    fn decodes_cp1252() {
        assert_eq!(decode(b"Vid\xe9o"), "Vidéo");
        assert_eq!(decode("Vidéo".as_bytes()), "Vidéo");
    }

    /// Vérifie l'aller-retour sur un vrai fichier :
    /// `PSTART_XML=chemin\PStart.xml cargo test -- --ignored`
    #[test]
    #[ignore]
    fn round_trip_real_file() {
        let path = std::env::var("PSTART_XML").expect("PSTART_XML non défini");
        let original = fs::read_to_string(&path).unwrap();
        let cfg = Config::load(Path::new(&path)).unwrap();
        assert_eq!(cfg.to_xml(), original);
        assert!(!cfg.flatten().is_empty());
    }
}
