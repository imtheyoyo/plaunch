//! Arbre XML minimal et *sans perte* pour les fichiers PLaunch.
//!
//! PLaunch écrit un XML très régulier : pas de déclaration, des tabulations,
//! des fins de ligne CRLF, aucun contenu mixte, et tous les caractères
//! spéciaux ou non ASCII encodés en références numériques (`&#xE9;`, `&#x26;`).
//! Ce module relit ce format dans un arbre générique et le réécrit à
//! l'identique, de sorte que les sections que plaunch ne connaît pas
//! (notes, capteurs de souris, couleurs…) sont conservées telles quelles.

use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};

/// Un élément XML. PLaunch n'utilise jamais de contenu mixte : un élément a
/// soit des enfants, soit du texte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Element>,
    /// Texte (déjà décodé) d'un élément feuille.
    pub text: String,
    /// `true` pour un conteneur (élément qui a ou peut avoir des enfants,
    /// même vide, comme `<notes>` ou un groupe vide).
    pub container: bool,
}

impl Element {
    pub fn new(name: &str) -> Self {
        Element {
            name: name.to_string(),
            attrs: Vec::new(),
            children: Vec::new(),
            text: String::new(),
            container: false,
        }
    }

    pub fn leaf(name: &str, text: &str) -> Self {
        let mut e = Element::new(name);
        e.text = text.to_string();
        e
    }

    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }

    pub fn child_mut(&mut self, name: &str) -> Option<&mut Element> {
        self.children.iter_mut().find(|c| c.name == name)
    }

    /// Texte d'un enfant feuille, s'il existe.
    pub fn child_text(&self, name: &str) -> Option<&str> {
        self.child(name).map(|c| c.text.as_str())
    }

    /// Chemin d'accès par noms : `get(&["panel", "hotkey"])`.
    pub fn get(&self, path: &[&str]) -> Option<&Element> {
        let mut cur = self;
        for name in path {
            cur = cur.child(name)?;
        }
        Some(cur)
    }
}

#[derive(Debug)]
pub struct XmlError(pub String);

impl std::fmt::Display for XmlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "XML invalide : {}", self.0)
    }
}

impl std::error::Error for XmlError {}

/// Analyse un document et renvoie son élément racine.
pub fn parse(input: &str) -> Result<Element, XmlError> {
    // Ignore un éventuel BOM UTF-8.
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    let mut reader = Reader::from_str(input);
    reader.config_mut().trim_text(false);

    // Pile des éléments ouverts.
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;

    loop {
        let ev = reader
            .read_event()
            .map_err(|e| XmlError(format!("position {}: {e}", reader.buffer_position())))?;
        match ev {
            Event::Start(s) => {
                stack.push(start_element(&s)?);
            }
            Event::Empty(s) => {
                let el = start_element(&s)?;
                attach(&mut stack, &mut root, el)?;
            }
            Event::End(_) => {
                let mut el = stack
                    .pop()
                    .ok_or_else(|| XmlError("balise fermante orpheline".into()))?;
                finish(&mut el);
                attach(&mut stack, &mut root, el)?;
            }
            Event::Text(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&t.xml10_content());
                }
            }
            Event::GeneralRef(r) => {
                if let Some(top) = stack.last_mut() {
                    let c = resolve_ref(&r)?;
                    top.text.push(c);
                }
            }
            Event::CData(c) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&c.xml10_content());
                }
            }
            Event::Eof => break,
            // Déclaration, commentaires, instructions de traitement : ignorés.
            _ => {}
        }
    }

    if !stack.is_empty() {
        return Err(XmlError("document tronqué (balises non fermées)".into()));
    }
    root.ok_or_else(|| XmlError("document vide".into()))
}

fn start_element(s: &quick_xml::events::BytesStart<'_>) -> Result<Element, XmlError> {
    let name = s.name().0.to_string();
    let mut el = Element::new(&name);
    for a in s.attributes() {
        let a = a.map_err(|e| XmlError(format!("attribut : {e}")))?;
        let key = a.key.0.to_string();
        let value = a
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|e| XmlError(format!("attribut {key} : {e}")))?
            .into_owned();
        el.attrs.push((key, value));
    }
    Ok(el)
}

fn resolve_ref(r: &quick_xml::events::BytesRef<'_>) -> Result<char, XmlError> {
    if let Some(c) = r
        .resolve_char_ref()
        .map_err(|e| XmlError(format!("référence de caractère : {e}")))?
    {
        return Ok(c);
    }
    let name: &str = r.as_ref();
    match name {
        "amp" => Ok('&'),
        "lt" => Ok('<'),
        "gt" => Ok('>'),
        "quot" => Ok('"'),
        "apos" => Ok('\''),
        other => Err(XmlError(format!("entité inconnue &{other};"))),
    }
}

/// Termine un élément : si c'est un conteneur, le texte (indentation) est
/// abandonné.
fn finish(el: &mut Element) {
    let whitespace_only = el.text.chars().all(char::is_whitespace);
    if !el.children.is_empty() {
        el.container = true;
        el.text.clear();
    } else if whitespace_only && el.text.contains('\n') {
        // `<notes>\r\n\t</notes>` : conteneur vide.
        el.container = true;
        el.text.clear();
    }
}

fn attach(stack: &mut [Element], root: &mut Option<Element>, el: Element) -> Result<(), XmlError> {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(el);
        Ok(())
    } else if root.is_none() {
        *root = Some(el);
        Ok(())
    } else {
        Err(XmlError("plusieurs éléments racine".into()))
    }
}

/// Sérialise au format PLaunch (tabulations, CRLF, références numériques).
pub fn write(root: &Element) -> String {
    let mut out = String::with_capacity(64 * 1024);
    write_element(&mut out, root, 0);
    out
}

fn write_element(out: &mut String, el: &Element, depth: usize) {
    indent(out, depth);
    out.push('<');
    out.push_str(&el.name);
    for (k, v) in &el.attrs {
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        escape_into(out, v);
        out.push('"');
    }
    out.push('>');
    if el.container || !el.children.is_empty() {
        out.push_str("\r\n");
        for c in &el.children {
            write_element(out, c, depth + 1);
        }
        indent(out, depth);
    } else {
        escape_into(out, &el.text);
    }
    out.push_str("</");
    out.push_str(&el.name);
    out.push_str(">\r\n");
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push('\t');
    }
}

/// Encode comme PLaunch : `&`, `<`, `>`, `"` et tout caractère non ASCII en
/// référence hexadécimale majuscule.
fn escape_into(out: &mut String, s: &str) {
    use std::fmt::Write;
    for c in s.chars() {
        match c {
            '&' | '<' | '>' | '"' => {
                let _ = write!(out, "&#x{:X};", c as u32);
            }
            c if !c.is_ascii() => {
                let _ = write!(out, "&#x{:X};", c as u32);
            }
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_entities() {
        let doc = "<a><b name=\"Tag&#x26;Rename\">Cr&#xE9;ation &amp; co</b></a>";
        let root = parse(doc).unwrap();
        let b = root.child("b").unwrap();
        assert_eq!(b.attr("name"), Some("Tag&Rename"));
        assert_eq!(b.text, "Création & co");
    }

    #[test]
    fn empty_container_vs_empty_leaf() {
        let doc = "<a>\r\n\t<notes>\r\n\t</notes>\r\n\t<sound></sound>\r\n</a>\r\n";
        let root = parse(doc).unwrap();
        assert!(root.child("notes").unwrap().container);
        assert!(!root.child("sound").unwrap().container);
        assert_eq!(write(&root), doc);
    }

    #[test]
    fn escape_like_plaunch() {
        let mut s = String::new();
        escape_into(&mut s, "Audacity® & \"Création\"");
        assert_eq!(s, "Audacity&#xAE; &#x26; &#x22;Cr&#xE9;ation&#x22;");
    }
}
