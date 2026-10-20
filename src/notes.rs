//! Notes avec alarmes, stockées dans `plaunch-notes.xml` à côté de
//! `PLaunch.xml` (même format d'écriture que PLaunch : tabulations, CRLF,
//! entités numériques).

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{Duration, NaiveDateTime};

use crate::i18n::tr;
use crate::xml::{self, Element};

pub const FILE_NAME: &str = "plaunch-notes.xml";
const STORE_FORMAT: &str = "%Y/%m/%d %H:%M:%S";
/// Format de saisie dans l'interface.
pub const INPUT_FORMAT: &str = "%Y/%m/%d %H:%M";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    Once,
    Daily,
    Weekly,
}

impl Repeat {
    pub const ALL: [Repeat; 3] = [Repeat::Once, Repeat::Daily, Repeat::Weekly];

    pub fn label(self) -> &'static str {
        match self {
            Repeat::Once => tr("une fois"),
            Repeat::Daily => tr("chaque jour"),
            Repeat::Weekly => tr("chaque semaine"),
        }
    }

    fn key(self) -> &'static str {
        match self {
            Repeat::Once => "once",
            Repeat::Daily => "daily",
            Repeat::Weekly => "weekly",
        }
    }

    fn from_key(s: &str) -> Repeat {
        match s.trim() {
            "daily" => Repeat::Daily,
            "weekly" => Repeat::Weekly,
            _ => Repeat::Once,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub title: String,
    pub text: String,
    /// Couleur de fond (RVB).
    pub color: [u8; 3],
    pub alarm: Option<NaiveDateTime>,
    pub repeat: Repeat,
    /// Texte de saisie de l'alarme (interface uniquement).
    pub alarm_input: String,
}

impl Note {
    pub fn new(color: [u8; 3]) -> Note {
        Note {
            title: String::new(),
            text: String::new(),
            color,
            alarm: None,
            repeat: Repeat::Once,
            alarm_input: String::new(),
        }
    }

    pub fn sync_input(&mut self) {
        self.alarm_input = self
            .alarm
            .map(|a| a.format(INPUT_FORMAT).to_string())
            .unwrap_or_default();
    }

    /// Valide la saisie de l'alarme ; renvoie `false` si elle est invalide.
    #[cfg(test)]
    pub fn apply_input(&mut self) -> bool {
        let s = self.alarm_input.trim();
        if s.is_empty() {
            self.alarm = None;
            return true;
        }
        match parse_input(s) {
            Some(t) => {
                self.alarm = Some(t);
                true
            }
            None => false,
        }
    }
}

/// Accepte `AAAA/MM/JJ HH:MM`, avec `-` ou `/` et secondes facultatives.
#[cfg(test)]
pub fn parse_input(s: &str) -> Option<NaiveDateTime> {
    let s = s.trim().replace('-', "/");
    NaiveDateTime::parse_from_str(&s, INPUT_FORMAT)
        .or_else(|_| NaiveDateTime::parse_from_str(&s, STORE_FORMAT))
        .ok()
}

#[derive(Debug)]
pub struct Notes {
    pub file: PathBuf,
    pub items: Vec<Note>,
}

pub fn path_for(config_file: &Path) -> PathBuf {
    config_file
        .parent()
        .map(|p| p.join(FILE_NAME))
        .unwrap_or_else(|| PathBuf::from(FILE_NAME))
}

impl Notes {
    /// Charge les notes (liste vide si le fichier n'existe pas encore).
    pub fn load(file: &Path) -> Result<Notes, String> {
        let items = match fs::read_to_string(file) {
            Ok(text) => parse(&text)?,
            Err(_) => Vec::new(),
        };
        Ok(Notes {
            file: file.to_path_buf(),
            items,
        })
    }

    pub fn save(&self) -> Result<(), String> {
        let data = to_xml(&self.items);
        let mut tmp = self.file.as_os_str().to_os_string();
        tmp.push(".tmp");
        fs::write(&tmp, data.as_bytes()).map_err(|e| format!("notes : {e}"))?;
        fs::rename(&tmp, &self.file).map_err(|e| format!("notes : {e}"))
    }

    /// Renvoie les notes dont l'alarme est échue et reprogramme les alarmes
    /// répétitives (les alarmes uniques sont effacées).
    pub fn take_due(&mut self, now: NaiveDateTime) -> Vec<Note> {
        let mut fired = Vec::new();
        for note in &mut self.items {
            let Some(at) = note.alarm else { continue };
            if at > now {
                continue;
            }
            fired.push(note.clone());
            note.alarm = match note.repeat {
                Repeat::Once => None,
                Repeat::Daily => Some(next_after(at, Duration::days(1), now)),
                Repeat::Weekly => Some(next_after(at, Duration::weeks(1), now)),
            };
            note.sync_input();
        }
        fired
    }
}

/// Prochaine occurrence strictement après `now`.
fn next_after(mut at: NaiveDateTime, step: Duration, now: NaiveDateTime) -> NaiveDateTime {
    while at <= now {
        at += step;
    }
    at
}

fn parse(text: &str) -> Result<Vec<Note>, String> {
    let root = xml::parse(text).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for el in root.children.iter().filter(|c| c.name == "note") {
        let mut note = Note::new([0xFF, 0xFF, 0xA0]);
        note.title = el.child_text("title").unwrap_or("").to_string();
        note.text = el.child_text("text").unwrap_or("").to_string();
        if let Some(c) = el.child_text("color").and_then(crate::config::parse_color) {
            note.color = c;
        }
        note.alarm = el
            .child_text("alarm")
            .and_then(|a| NaiveDateTime::parse_from_str(a.trim(), STORE_FORMAT).ok());
        note.repeat = Repeat::from_key(el.child_text("repeat").unwrap_or(""));
        note.sync_input();
        out.push(note);
    }
    Ok(out)
}

fn to_xml(items: &[Note]) -> String {
    let mut root = Element::new("notes");
    root.container = true;
    for n in items {
        let mut el = Element::new("note");
        el.container = true;
        el.children.push(Element::leaf("title", &n.title));
        el.children.push(Element::leaf("text", &n.text));
        el.children.push(Element::leaf(
            "color",
            &crate::config::format_color(n.color),
        ));
        if let Some(a) = n.alarm {
            el.children
                .push(Element::leaf("alarm", &a.format(STORE_FORMAT).to_string()));
            el.children.push(Element::leaf("repeat", n.repeat.key()));
        }
        root.children.push(el);
    }
    xml::write(&root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> NaiveDateTime {
        parse_input(s).unwrap()
    }

    #[test]
    fn round_trip() {
        let mut a = Note::new([0xFF, 0xFF, 0xA0]);
        a.title = "Courses".into();
        a.text = "Lait & œufs\nPain".into();
        a.alarm = Some(t("2026/10/19 08:30"));
        a.repeat = Repeat::Weekly;
        a.sync_input();
        let b = Note::new([0xA0, 0xC0, 0xFF]);
        let xml = to_xml(&[a.clone(), b.clone()]);
        assert!(xml.contains("Lait &#x26; &#x153;ufs"));
        let back = parse(&xml).unwrap();
        assert_eq!(back, vec![a, b]);
    }

    #[test]
    fn input_parsing() {
        assert!(parse_input("2026/10/19 08:30").is_some());
        assert!(parse_input("2026-10-19 08:30").is_some());
        assert!(parse_input("19/10/2026").is_none());
        let mut n = Note::new([0; 3]);
        n.alarm_input = "demain".into();
        assert!(!n.apply_input());
        n.alarm_input = String::new();
        assert!(n.apply_input());
        assert_eq!(n.alarm, None);
    }

    #[test]
    fn due_alarms() {
        let mut notes = Notes {
            file: PathBuf::new(),
            items: vec![Note::new([0; 3]), Note::new([0; 3]), Note::new([0; 3])],
        };
        notes.items[0].alarm = Some(t("2026/10/18 09:00"));
        notes.items[1].alarm = Some(t("2026/10/10 07:00"));
        notes.items[1].repeat = Repeat::Daily;
        notes.items[2].alarm = Some(t("2026/10/18 23:00"));
        let now = t("2026/10/18 12:00");
        let fired = notes.take_due(now);
        assert_eq!(fired.len(), 2);
        assert_eq!(notes.items[0].alarm, None);
        // Alarme quotidienne manquée : reprogrammée au lendemain matin.
        assert_eq!(notes.items[1].alarm, Some(t("2026/10/19 07:00")));
        assert_eq!(notes.items[1].alarm_input, "2026/10/19 07:00");
        assert_eq!(notes.items[2].alarm, Some(t("2026/10/18 23:00")));
        assert!(notes.take_due(now).is_empty());
    }
}
