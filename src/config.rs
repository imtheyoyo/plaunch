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
    /// Date de modification du fichier au chargement ou au dernier
    /// enregistrement : sert à détecter une modification extérieure.
    pub mtime: Option<std::time::SystemTime>,
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
    pub favorite: bool,
    /// Position dans la barre des favoris (0 : non fixée).
    pub favorite_rank: u32,
    pub autorun: bool,
}

/// État de la fenêtre au lancement (`<windowstate>`, valeurs Delphi).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowState {
    #[default]
    Normal,
    Minimized,
    Maximized,
}

impl WindowState {
    pub const ALL: [WindowState; 3] = [
        WindowState::Normal,
        WindowState::Minimized,
        WindowState::Maximized,
    ];

    pub fn key(self) -> &'static str {
        match self {
            WindowState::Normal => "wsNormal",
            WindowState::Minimized => "wsMinimized",
            WindowState::Maximized => "wsMaximized",
        }
    }

    pub fn from_key(s: &str) -> WindowState {
        match s.trim().to_ascii_lowercase().as_str() {
            "wsminimized" | "1" | "minimized" => WindowState::Minimized,
            "wsmaximized" | "2" | "maximized" => WindowState::Maximized,
            _ => WindowState::Normal,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            WindowState::Normal => crate::i18n::tr("normale"),
            WindowState::Minimized => crate::i18n::tr("réduite"),
            WindowState::Maximized => crate::i18n::tr("agrandie"),
        }
    }
}

/// Options d'un raccourci. `parameters`, `description`, `windowstate` et
/// `autorun` sont des balises de PStart ; `runasadmin` et `favorite` sont
/// propres à PStart-rs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EntryOptions {
    pub parameters: String,
    pub description: String,
    pub window: WindowState,
    pub autorun: bool,
    pub admin: bool,
    pub favorite: bool,
    /// Position dans la barre des favoris (0 : non fixée, ordre du fichier).
    pub favorite_rank: u32,
    /// Raccourci clavier global qui lance ce programme (`Ctrl+Alt+V`).
    pub hotkey: String,
}

impl EntryOptions {
    pub fn of(el: &Element) -> EntryOptions {
        let text = |n: &str| el.child_text(n).unwrap_or("").trim().to_string();
        EntryOptions {
            parameters: text("parameters"),
            description: text("description"),
            window: WindowState::from_key(&text("windowstate")),
            autorun: is_true(&text("autorun")),
            admin: is_true(&text("runasadmin")),
            favorite: favorite_value(&text("favorite")).0,
            favorite_rank: favorite_value(&text("favorite")).1,
            hotkey: text("globalhotkey"),
        }
    }
}

/// `<favorite>` : `True` (épinglé) ou un numéro d'ordre dans la barre.
pub fn favorite_value(s: &str) -> (bool, u32) {
    let s = s.trim();
    if s.eq_ignore_ascii_case("true") {
        return (true, 0);
    }
    match s.parse::<u32>() {
        Ok(n) if n > 0 => (true, n),
        _ => (false, 0),
    }
}

fn is_true(s: &str) -> bool {
    s.eq_ignore_ascii_case("true") || s == "1"
}

/// Ordre des balises d'un raccourci, pour insérer au bon endroit.
pub const ENTRY_TAGS: [&str; 11] = [
    "path",
    "parameters",
    "directory",
    "description",
    "windowstate",
    "autorun",
    "runasadmin",
    "favorite",
    "globalhotkey",
    "runcount",
    "rundate",
];

/// Remplace, insère (à sa place dans [`ENTRY_TAGS`]) ou retire une balise.
pub fn set_tag(el: &mut Element, name: &str, value: Option<&str>) {
    let existing = el.children.iter().position(|c| c.name == name);
    match (value, existing) {
        (Some(v), Some(i)) => el.children[i].text = v.to_string(),
        (Some(v), None) => {
            let rank = ENTRY_TAGS.iter().position(|t| *t == name);
            let pos = el
                .children
                .iter()
                .position(|c| {
                    let r = ENTRY_TAGS.iter().position(|t| *t == c.name);
                    matches!((rank, r), (Some(a), Some(b)) if b > a)
                })
                .unwrap_or(el.children.len());
            el.children.insert(pos, Element::leaf(name, v));
        }
        (None, Some(i)) => {
            el.children.remove(i);
        }
        (None, None) => {}
    }
}

/// Écrit les options d'un raccourci (les valeurs par défaut retirent la balise).
pub fn set_entry_options(el: &mut Element, o: &EntryOptions) {
    let opt = |s: &str| {
        let s = s.trim();
        (!s.is_empty()).then(|| s.to_string())
    };
    let flag = |b: bool| b.then(|| "True".to_string());
    set_tag(el, "parameters", opt(&o.parameters).as_deref());
    set_tag(el, "description", opt(&o.description).as_deref());
    let ws = (o.window != WindowState::Normal).then(|| o.window.key().to_string());
    set_tag(el, "windowstate", ws.as_deref());
    set_tag(el, "autorun", flag(o.autorun).as_deref());
    set_tag(el, "runasadmin", flag(o.admin).as_deref());
    let fav = o.favorite.then(|| {
        if o.favorite_rank > 0 {
            o.favorite_rank.to_string()
        } else {
            "True".to_string()
        }
    });
    set_tag(el, "favorite", fav.as_deref());
    set_tag(el, "globalhotkey", opt(&o.hotkey).as_deref());
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
            mtime: modified(file),
        })
    }

    /// Construit une configuration en mémoire (tests).
    #[cfg(test)]
    pub fn from_str(text: &str, base_dir: &str) -> Result<Config, String> {
        let root = xml::parse(text).map_err(|e| e.to_string())?;
        Ok(Config {
            root,
            file: PathBuf::new(),
            mtime: None,
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
        self.mtime = modified(&self.file);
        Ok(())
    }

    /// Le fichier a-t-il été modifié par un autre programme ?
    pub fn changed_on_disk(&self) -> bool {
        match (modified(&self.file), self.mtime) {
            (Some(now), Some(known)) => now != known,
            _ => false,
        }
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
        if paths::is_url(entry_path(entry)) {
            return None;
        }
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
                    favorite: favorite_value(child.child_text("favorite").unwrap_or("")).0,
                    favorite_rank: favorite_value(child.child_text("favorite").unwrap_or("")).1,
                    autorun: is_true(child.child_text("autorun").unwrap_or("").trim()),
                }),
                _ => {}
            }
            path.pop();
        }
    }

    /// Favoris dans l'ordre de la barre (numéro, puis ordre du fichier).
    pub fn favorites(&self) -> Vec<FlatEntry> {
        let mut favs: Vec<FlatEntry> = self.flatten().into_iter().filter(|e| e.favorite).collect();
        // Tri stable : à numéro égal, l'ordre du fichier est conservé.
        favs.sort_by_key(|e| {
            if e.favorite_rank == 0 {
                u32::MAX
            } else {
                e.favorite_rank
            }
        });
        favs
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

    /// Options de recherche de PStart (`<search>`).
    pub fn search_options(&self) -> SearchOptions {
        let color = |name: &str| {
            self.root
                .get(&["search", "colors", name])
                .and_then(|e| parse_color(&e.text))
        };
        SearchOptions {
            pstart: self.flag(&["search", "pstart"]).unwrap_or(true),
            current_user: self
                .flag(&["search", "currentuserstartmenu"])
                .unwrap_or(true),
            all_users: self.flag(&["search", "allusersstartmenu"]).unwrap_or(true),
            colorize: self.flag(&["search", "colorize"]).unwrap_or(true),
            color_pstart: color("pstart"),
            color_current_user: color("currentuserstartmenu"),
            color_all_users: color("allusersstartmenu"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchOptions {
    pub pstart: bool,
    pub current_user: bool,
    pub all_users: bool,
    pub colorize: bool,
    pub color_pstart: Option<[u8; 3]>,
    pub color_current_user: Option<[u8; 3]>,
    pub color_all_users: Option<[u8; 3]>,
}

/// RVB → couleur Delphi `BBGGRRh`.
pub fn format_color([r, g, b]: [u8; 3]) -> String {
    format!("{b:02X}{g:02X}{r:02X}h")
}

/// Palette de couleurs de PStart (`<colors>`), dans l'ordre du fichier.
impl Config {
    pub fn palette(&self) -> Vec<[u8; 3]> {
        let mut out: Vec<[u8; 3]> = self
            .root
            .child("colors")
            .map(|c| {
                c.children
                    .iter()
                    .filter_map(|e| e.child_text("value").and_then(parse_color))
                    .collect()
            })
            .unwrap_or_default();
        // Complète avec les couleurs par défaut de PStart.
        for c in ["A0FFFFh", "A0FFA0h", "FFA0A0h", "A0C0FFh", "A0A0FFh"] {
            if out.len() >= 5 {
                break;
            }
            if let Some(c) = parse_color(c) {
                if !out.contains(&c) {
                    out.push(c);
                }
            }
        }
        out
    }

    /// Couleur par défaut des notes (`<note><defaultcolor>`).
    pub fn default_note_color(&self) -> [u8; 3] {
        self.root
            .get(&["note", "defaultcolor"])
            .and_then(|e| parse_color(&e.text))
            .unwrap_or([0xFF, 0xFF, 0xA0])
    }
}

/// Couleur Delphi `BBGGRRh` (ex. `0080FFh`) → RVB.
pub fn parse_color(text: &str) -> Option<[u8; 3]> {
    let hex = text.trim().trim_end_matches(['h', 'H']);
    if hex.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(hex, 16).ok()?;
    Some([
        (v & 0xFF) as u8,
        ((v >> 8) & 0xFF) as u8,
        ((v >> 16) & 0xFF) as u8,
    ])
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

fn modified(file: &Path) -> Option<std::time::SystemTime> {
    fs::metadata(file).and_then(|m| m.modified()).ok()
}

/// Décode le fichier : UTF-8 (avec ou sans BOM), UTF-16 LE/BE, sinon
/// Windows-1252 (cas des anciens fichiers non ASCII).
fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let u: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        return String::from_utf16_lossy(&u);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let u: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_be_bytes(*c))
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

    fn cfg_palette_len() -> usize {
        Config::from_str(SAMPLE, "E:\\Portable")
            .unwrap()
            .palette()
            .len()
    }

    #[test]
    fn colors_are_bgr() {
        // 0080FFh en Delphi = rouge FF, vert 80, bleu 00 (orange).
        assert_eq!(parse_color("0080FFh"), Some([0xFF, 0x80, 0x00]));
        assert_eq!(parse_color("40A000h"), Some([0x00, 0xA0, 0x40]));
        assert_eq!(parse_color("xyz"), None);
        assert_eq!(format_color([0xFF, 0x80, 0x00]), "0080FFh");
        assert_eq!(cfg_palette_len(), 5);
        let cfg = Config::from_str(SAMPLE, "E:\\Portable").unwrap();
        let s = cfg.search_options();
        assert!(s.pstart && s.current_user && s.all_users);
    }

    #[test]
    fn decodes_cp1252() {
        assert_eq!(decode(b"Vid\xe9o"), "Vidéo");
        assert_eq!(decode("Vidéo".as_bytes()), "Vidéo");
        assert_eq!(decode(&[0xFF, 0xFE, b'V', 0, 0xE9, 0]), "Vé");
        assert_eq!(decode(&[0xFE, 0xFF, 0, b'V', 0, 0xE9]), "Vé");
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
