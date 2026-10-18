//! Mise à jour intégrée : interroge la dernière version publiée sur GitHub,
//! télécharge `pstart-rs.exe` et remplace l'exécutable (même sur la clé USB).
//!
//! Windows autorise à renommer un exécutable en cours d'exécution : l'ancien
//! devient `pstart-rs.exe.old` (supprimé au démarrage suivant), le nouveau
//! prend sa place, puis l'application se relance.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::i18n::{tr, trf};

const API: &str = "https://api.github.com/repos/imtheyoyo/pstart-rs/releases/latest";
pub const RELEASES_PAGE: &str = "https://github.com/imtheyoyo/pstart-rs/releases";
const ASSET: &str = "pstart-rs.exe";
/// Variable posée lors d'une relance : la nouvelle instance attend que
/// l'ancienne ait libéré le verrou d'instance unique.
pub const RESTART_ENV: &str = "PSTART_RS_RESTARTED";
const MAX_SIZE: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub url: String,
    /// Fichier `pstart-rs.exe.sha256` publié avec la version.
    pub checksum_url: Option<String>,
    pub page: String,
}

/// État affiché dans les réglages et la barre d'état.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Downloading,
    /// Nouvel exécutable en place : relance en cours.
    Installed,
    Failed(String),
}

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// `v0.6.1` → (0, 6, 1). Les suffixes (`-beta`) sont ignorés.
pub fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    let core = s.split(['-', '+']).next()?;
    let mut it = core.split('.').map(|p| p.parse::<u32>());
    let major = it.next()?.ok()?;
    let minor = it.next().unwrap_or(Ok(0)).ok()?;
    let patch = it.next().unwrap_or(Ok(0)).ok()?;
    Some((major, minor, patch))
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

fn agent() -> ureq::Agent {
    let mut builder = ureq::AgentBuilder::new();
    match native_tls::TlsConnector::new() {
        Ok(tls) => builder = builder.tls_connector(std::sync::Arc::new(tls)),
        Err(e) => crate::log::error(&format!("TLS : {e}")),
    }
    builder
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .user_agent(concat!("pstart-rs/", env!("CARGO_PKG_VERSION")))
        .try_proxy_from_env(true)
        .build()
}

/// Lit la réponse de l'API GitHub (`releases/latest`).
pub fn parse_release(json: &str) -> Result<Release, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let version = v["tag_name"]
        .as_str()
        .ok_or_else(|| tr("réponse inattendue de GitHub").to_string())?
        .to_string();
    let page = v["html_url"].as_str().unwrap_or(RELEASES_PAGE).to_string();
    let asset = |name: &str| {
        v["assets"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|a| a["name"].as_str() == Some(name))
            .and_then(|a| a["browser_download_url"].as_str())
            .map(str::to_string)
    };
    let url = asset(ASSET).ok_or_else(|| trf("{} absent de la version publiée", &[&ASSET]))?;
    let checksum_url = asset(&format!("{ASSET}.sha256"));
    Ok(Release {
        version,
        url,
        checksum_url,
        page,
    })
}

/// Renvoie la dernière version si elle est plus récente que celle-ci.
pub fn check() -> Result<Option<Release>, String> {
    let body = agent()
        .get(API)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| trf("GitHub inaccessible : {}", &[&e]))?
        .into_string()
        .map_err(|e| e.to_string())?;
    let release = parse_release(&body)?;
    Ok(is_newer(&release.version, current_version()).then_some(release))
}

fn download(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let resp = agent()
        .get(url)
        .call()
        .map_err(|e| trf("téléchargement impossible : {}", &[&e]))?;
    let mut data = Vec::new();
    resp.into_reader()
        .take(limit)
        .read_to_end(&mut data)
        .map_err(|e| trf("téléchargement impossible : {}", &[&e]))?;
    Ok(data)
}

/// Empreinte SHA-256 en hexadécimal minuscule.
pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Lit un fichier `.sha256` (« empreinte  nom » ou empreinte seule).
pub fn parse_checksum(text: &str) -> Option<String> {
    let h = text.split_whitespace().next()?.to_ascii_lowercase();
    (h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit())).then_some(h)
}

/// Vérifie l'exécutable téléchargé : format et empreinte publiée.
pub fn verify(data: &[u8], expected: &str) -> Result<(), String> {
    // Un exécutable Windows commence par « MZ ».
    if data.len() < 64 * 1024 || !data.starts_with(b"MZ") {
        return Err(tr("le fichier téléchargé n'est pas un exécutable valide").into());
    }
    let actual = sha256_hex(data);
    if actual != expected {
        crate::log::error(&format!(
            "empreinte invalide : attendu {expected}, obtenu {actual}"
        ));
        return Err(tr("empreinte SHA-256 incorrecte : mise à jour refusée").into());
    }
    Ok(())
}

/// Télécharge la version, vérifie son empreinte puis remplace l'exécutable.
pub fn install(release: &Release) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let Some(sum_url) = &release.checksum_url else {
        return Err(tr("pas d'empreinte publiée pour cette version : mise à jour manuelle").into());
    };
    let sum = download(sum_url, 4096)?;
    let expected = parse_checksum(&String::from_utf8_lossy(&sum))
        .ok_or_else(|| tr("empreinte publiée illisible").to_string())?;
    let data = download(&release.url, MAX_SIZE)?;
    verify(&data, &expected)?;
    crate::log::info(&format!(
        "mise à jour vers {} vérifiée ({expected})",
        release.version
    ));
    replace_exe(&exe, &data)
}

fn with_suffix(exe: &Path, suffix: &str) -> PathBuf {
    let mut s = exe.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

/// Écrit `data` à côté de `exe` puis échange les fichiers.
pub fn replace_exe(exe: &Path, data: &[u8]) -> Result<(), String> {
    let new = with_suffix(exe, ".new");
    let old = with_suffix(exe, ".old");
    let io = |e: std::io::Error| trf("remplacement impossible : {}", &[&e]);
    std::fs::write(&new, data).map_err(io)?;
    let _ = std::fs::remove_file(&old);
    std::fs::rename(exe, &old).map_err(|e| {
        let _ = std::fs::remove_file(&new);
        io(e)
    })?;
    if let Err(e) = std::fs::rename(&new, exe) {
        // On remet l'ancien exécutable en place.
        let _ = std::fs::rename(&old, exe);
        return Err(io(e));
    }
    Ok(())
}

/// Supprime l'ancien exécutable laissé par une mise à jour.
pub fn cleanup() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(with_suffix(&exe, ".old"));
        let _ = std::fs::remove_file(with_suffix(&exe, ".new"));
    }
}

/// Relance l'exécutable (avec les mêmes arguments).
pub fn relaunch() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .env(RESTART_ENV, "1")
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("v0.6.1"), Some((0, 6, 1)));
        assert_eq!(parse_version("1.2"), Some((1, 2, 0)));
        assert_eq!(parse_version("0.7.0-beta"), Some((0, 7, 0)));
        assert_eq!(parse_version("abc"), None);
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(!is_newer("v0.6.0", "0.6.0"));
        assert!(!is_newer("n'importe quoi", "0.6.0"));
        assert!(parse_version(current_version()).is_some());
    }

    #[test]
    fn release_json() {
        let json = r#"{"tag_name":"v0.7.0","html_url":"https://example/r",
            "assets":[{"name":"autre.zip","browser_download_url":"https://x/a"},
                      {"name":"pstart-rs.exe","browser_download_url":"https://x/p.exe"}]}"#;
        let r = parse_release(json).unwrap();
        assert_eq!(r.version, "v0.7.0");
        assert_eq!(r.url, "https://x/p.exe");
        assert_eq!(r.checksum_url, None);
        let json = r#"{"tag_name":"v0.7.0","assets":[
            {"name":"pstart-rs.exe","browser_download_url":"https://x/p.exe"},
            {"name":"pstart-rs.exe.sha256","browser_download_url":"https://x/p.sha"}]}"#;
        assert_eq!(
            parse_release(json).unwrap().checksum_url.as_deref(),
            Some("https://x/p.sha")
        );
        assert!(parse_release(r#"{"tag_name":"v1","assets":[]}"#).is_err());
        assert!(parse_release("pas du json").is_err());
    }

    #[test]
    fn checksums() {
        // Vecteur de test officiel : SHA-256("abc").
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let h = "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD  pstart-rs.exe";
        assert_eq!(
            parse_checksum(h).as_deref(),
            Some(sha256_hex(b"abc").as_str())
        );
        assert_eq!(parse_checksum("pas une empreinte"), None);
        let mut exe = b"MZ".to_vec();
        exe.resize(70 * 1024, 7);
        let good = sha256_hex(&exe);
        assert!(verify(&exe, &good).is_ok());
        assert!(verify(&exe, &sha256_hex(b"autre")).is_err());
        assert!(verify(b"MZ court", &sha256_hex(b"MZ court")).is_err());
    }

    #[test]
    fn swap_files() {
        let dir = std::env::temp_dir().join(format!("pstart-upd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("pstart-rs.exe");
        std::fs::write(&exe, b"ancien").unwrap();
        replace_exe(&exe, b"nouveau").unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"nouveau");
        assert_eq!(std::fs::read(with_suffix(&exe, ".old")).unwrap(), b"ancien");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
