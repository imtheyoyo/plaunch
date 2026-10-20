//! Résolution des chemins à la manière de PLaunch.
//!
//! Les chemins d'un `PLaunch.xml` peuvent être :
//! - relatifs au dossier de PLaunch : `_HD\mkvtoolnix\mkvtoolnix-gui.exe`,
//!   `..\PortableApps\qBittorrentPortable\qBittorrentPortable.exe` ;
//! - relatifs à la racine du lecteur de PLaunch : `\Outils\x.exe` ;
//! - absolus : `C:\Program Files (x86)\PowerISO\PowerISO.exe` ;
//! - UNC : `\\SERVEURNAS\Disque_DATA_1\_BDZ\ComicRack\ComicRack.exe`.
//!
//! Le calcul se fait sur des chaînes au format Windows pour que le
//! comportement soit identique (et testable) sur toutes les plateformes.

/// Résout `raw` par rapport au dossier `base` (dossier du fichier PLaunch.xml).
pub fn resolve(raw: &str, base: &str) -> String {
    if is_url(raw) {
        return raw.trim().to_string();
    }
    let raw = expand_env(&expand_vars(raw.trim(), base));
    let raw = raw.replace('/', "\\");
    let base = base.replace('/', "\\");

    let joined = if raw.starts_with("\\\\") || has_drive(&raw) {
        raw
    } else if raw.starts_with('\\') {
        // Relatif à la racine du lecteur de base.
        match prefix_of(&base) {
            Some(p) => format!("{p}{raw}"),
            None => raw,
        }
    } else if raw.is_empty() {
        base.clone()
    } else {
        format!("{}\\{}", base.trim_end_matches('\\'), raw)
    };
    normalize(&joined)
}

/// Adresse web ou autre protocole (`https://…`, `mailto:` exclu).
pub fn is_url(s: &str) -> bool {
    let s = s.trim();
    s.find("://").is_some_and(|i| {
        i > 1
            && s[..i]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+')
    })
}

/// Variables propres à PLaunch : `%PLAUNCH%` (dossier de PLaunch.xml) et
/// `%DRIVE%` (son lecteur, ex. `E:`). Insensibles à la casse.
pub fn expand_vars(raw: &str, base: &str) -> String {
    if !raw.contains('%') {
        return raw.to_string();
    }
    let base = base.replace('/', "\\");
    let drive = prefix_of(&base).unwrap_or_default();
    let mut out = raw.to_string();
    for (var, value) in [
        ("%PLAUNCH%", base.trim_end_matches('\\').to_string()),
        ("%DRIVE%", drive),
    ] {
        while let Some(i) = out.to_ascii_uppercase().find(var) {
            out.replace_range(i..i + var.len(), &value);
        }
    }
    out
}

/// `true` si le chemin commence par une lettre de lecteur (`C:`).
pub fn has_drive(p: &str) -> bool {
    let b = p.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// Préfixe non modifiable d'un chemin absolu : `C:` ou `\\serveur\partage`.
fn prefix_of(p: &str) -> Option<String> {
    if has_drive(p) {
        return Some(p[..2].to_string());
    }
    if let Some(rest) = p.strip_prefix("\\\\") {
        let mut parts = rest.splitn(3, '\\');
        let server = parts.next().unwrap_or("");
        let share = parts.next().unwrap_or("");
        if share.is_empty() {
            return Some(format!("\\\\{server}"));
        }
        return Some(format!("\\\\{server}\\{share}"));
    }
    None
}

/// Supprime les composants `.` et `..` sans remonter au-delà du préfixe.
pub fn normalize(p: &str) -> String {
    let (prefix, rest) = match prefix_of(p) {
        Some(pre) => {
            let rest = p[pre.len()..].to_string();
            (pre, rest)
        }
        None => (String::new(), p.to_string()),
    };
    let absolute = rest.starts_with('\\') || !prefix.is_empty();
    let mut parts: Vec<&str> = Vec::new();
    for comp in rest.split('\\') {
        match comp {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|l| *l != "..") {
                    parts.pop();
                } else if !absolute {
                    parts.push("..");
                }
            }
            c => parts.push(c),
        }
    }
    let body = parts.join("\\");
    let trailing = if rest.ends_with('\\') && !body.is_empty() {
        "\\"
    } else {
        ""
    };
    if prefix.is_empty() && !absolute {
        format!("{body}{trailing}")
    } else {
        format!("{prefix}\\{body}{trailing}")
    }
}

/// Dossier parent d'un chemin Windows (sans le dernier composant).
pub fn parent(p: &str) -> Option<String> {
    let p = p.trim_end_matches('\\');
    let idx = p.rfind('\\')?;
    let parent = &p[..idx];
    if parent.is_empty() || parent == "\\" {
        return None;
    }
    if has_drive(parent) && parent.len() == 2 {
        return Some(format!("{parent}\\"));
    }
    Some(parent.to_string())
}

/// Remplace les variables `%NOM%` par leur valeur d'environnement.
pub fn expand_env(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match std::env::var(name) {
                    Ok(v) if !name.is_empty() => out.push_str(&v),
                    _ => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Exprime `target` relativement au dossier `base` quand ils sont sur le
/// même lecteur (c'est ce qui rend une clé USB portable quelle que soit sa
/// lettre). Sinon, renvoie le chemin absolu tel quel.
pub fn make_relative(target: &str, base: &str) -> String {
    let target = normalize(&target.replace('/', "\\"));
    let base = normalize(&base.replace('/', "\\"));
    let same_drive =
        has_drive(&target) && has_drive(&base) && target[..2].eq_ignore_ascii_case(&base[..2]);
    if !same_drive {
        return target;
    }
    let t: Vec<&str> = target[2..].split('\\').filter(|s| !s.is_empty()).collect();
    let b: Vec<&str> = base[2..].split('\\').filter(|s| !s.is_empty()).collect();
    let common = t
        .iter()
        .zip(&b)
        .take_while(|(x, y)| x.to_lowercase() == y.to_lowercase())
        .count();
    let mut parts: Vec<&str> = std::iter::repeat_n("..", b.len() - common).collect();
    parts.extend(&t[common..]);
    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("\\")
    }
}

/// Convertit un chemin Windows en chemin natif de la plateforme courante.
/// Lettre de lecteur d'un chemin absolu (`E:\…` → `E`).
pub fn drive_letter(p: &str) -> Option<char> {
    let mut chars = p.chars();
    let letter = chars.next()?;
    (letter.is_ascii_alphabetic() && chars.next() == Some(':')).then(|| letter.to_ascii_uppercase())
}

/// Remplace la lettre de lecteur `old` par `new` si `p` est un chemin absolu
/// sur `old` (`F:\…`, `F:/…` ou `F:`). `None` sinon.
pub fn replace_drive(p: &str, old: char, new: char) -> Option<String> {
    if drive_letter(p)? != old.to_ascii_uppercase() {
        return None;
    }
    let rest = &p[2..];
    if !(rest.is_empty() || rest.starts_with('\\') || rest.starts_with('/')) {
        return None; // « F:relatif » : rare, laissé tel quel
    }
    Some(format!("{}:{rest}", new.to_ascii_uppercase()))
}

pub fn to_native(p: &str) -> std::path::PathBuf {
    if cfg!(windows) {
        std::path::PathBuf::from(p)
    } else {
        std::path::PathBuf::from(p.replace('\\', "/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variables() {
        let base = "E:\\Portable";
        assert_eq!(
            resolve("%PLAUNCH%\\VLC\\vlc.exe", base),
            "E:\\Portable\\VLC\\vlc.exe"
        );
        assert_eq!(resolve("%drive%\\Outils\\a.exe", base), "E:\\Outils\\a.exe");
        assert_eq!(expand_vars("100%", base), "100%");
        assert!(is_url("https://example.com/a?b=c"));
        assert!(!is_url("C:\\x"));
        assert!(!is_url("://x"));
        assert_eq!(
            resolve(" https://example.com ", base),
            "https://example.com"
        );
    }

    #[test]
    fn drives() {
        assert_eq!(drive_letter("e:\\x"), Some('E'));
        assert_eq!(drive_letter("\\\\NAS\\x"), None);
        assert_eq!(drive_letter("_HD\\x"), None);
        assert_eq!(
            replace_drive("F:\\Apps\\a.exe", 'f', 'g').as_deref(),
            Some("G:\\Apps\\a.exe")
        );
        assert_eq!(replace_drive("F:", 'F', 'E').as_deref(), Some("E:"));
        assert_eq!(replace_drive("F:rel", 'F', 'E'), None);
        assert_eq!(replace_drive("C:\\x", 'F', 'E'), None);
    }

    const BASE: &str = "F:\\_Appli Portables";

    #[test]
    fn relative() {
        assert_eq!(
            resolve("_HD\\mkvtoolnix\\mkvtoolnix-gui.exe", BASE),
            "F:\\_Appli Portables\\_HD\\mkvtoolnix\\mkvtoolnix-gui.exe"
        );
    }

    #[test]
    fn parent_dir() {
        assert_eq!(
            resolve(
                "..\\PortableApps\\qBittorrentPortable\\qBittorrentPortable.exe",
                BASE
            ),
            "F:\\PortableApps\\qBittorrentPortable\\qBittorrentPortable.exe"
        );
    }

    #[test]
    fn absolute_and_unc() {
        assert_eq!(
            resolve("C:\\Program Files (x86)\\PowerISO\\PowerISO.exe", BASE),
            "C:\\Program Files (x86)\\PowerISO\\PowerISO.exe"
        );
        assert_eq!(
            resolve(
                "\\\\SERVEURNAS\\Disque_DATA_1\\_BDZ\\ComicRack\\ComicRack.exe",
                BASE
            ),
            "\\\\SERVEURNAS\\Disque_DATA_1\\_BDZ\\ComicRack\\ComicRack.exe"
        );
    }

    #[test]
    fn drive_root() {
        assert_eq!(resolve("\\Outils\\x.exe", BASE), "F:\\Outils\\x.exe");
    }

    #[test]
    fn cannot_escape_drive() {
        assert_eq!(resolve("..\\..\\..\\x.exe", BASE), "F:\\x.exe");
    }

    #[test]
    fn folders_and_parents() {
        assert_eq!(
            resolve("_Internet\\JDownloader\\downloads", BASE),
            "F:\\_Appli Portables\\_Internet\\JDownloader\\downloads"
        );
        assert_eq!(parent("F:\\a\\b.exe").as_deref(), Some("F:\\a"));
        assert_eq!(parent("F:\\b.exe").as_deref(), Some("F:\\"));
    }

    #[test]
    fn relative_paths() {
        assert_eq!(
            make_relative("F:\\_Appli Portables\\_HD\\x.exe", BASE),
            "_HD\\x.exe"
        );
        assert_eq!(
            make_relative("f:\\PortableApps\\q\\q.exe", BASE),
            "..\\PortableApps\\q\\q.exe"
        );
        assert_eq!(
            make_relative("C:\\Program Files\\x.exe", BASE),
            "C:\\Program Files\\x.exe"
        );
        assert_eq!(
            make_relative("\\\\NAS\\p\\x.exe", BASE),
            "\\\\NAS\\p\\x.exe"
        );
        // Aller-retour avec resolve().
        let rel = make_relative("F:\\Outils\\Vid\u{e9}o\\v.exe", BASE);
        assert_eq!(resolve(&rel, BASE), "F:\\Outils\\Vid\u{e9}o\\v.exe");
    }

    #[test]
    fn env_vars() {
        std::env::set_var("PLAUNCH_TEST_VAR", "C:\\Outils");
        assert_eq!(
            resolve("%PLAUNCH_TEST_VAR%\\x.exe", BASE),
            "C:\\Outils\\x.exe"
        );
        assert_eq!(expand_env("100%"), "100%");
        assert_eq!(expand_env("%INCONNUE_XYZ%"), "%INCONNUE_XYZ%");
    }
}
