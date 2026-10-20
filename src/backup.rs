//! Sauvegarde et restauration : `PLaunch.xml`, les notes et les réglages dans
//! une seule archive zip.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;

use crate::i18n::trf;

/// Noms des fichiers dans l'archive.
pub const CONFIG: &str = "PLaunch.xml";
pub const SETTINGS: &str = crate::settings::FILE_NAME;
pub const NOTES: &str = crate::notes::FILE_NAME;
const MAX_ENTRY: u64 = 32 * 1024 * 1024;

/// Emplacements réels des fichiers sauvegardés.
#[derive(Debug, Clone)]
pub struct Targets {
    pub config: PathBuf,
    pub settings: PathBuf,
    pub notes: PathBuf,
}

impl Targets {
    pub fn for_config(config: &Path) -> Targets {
        Targets {
            config: config.to_path_buf(),
            settings: crate::settings::path_for(config),
            notes: crate::notes::path_for(config),
        }
    }

    fn pairs(&self) -> [(&'static str, &Path); 3] {
        [
            (CONFIG, &self.config),
            (SETTINGS, &self.settings),
            (NOTES, &self.notes),
        ]
    }
}

/// Nom proposé pour une nouvelle sauvegarde.
pub fn default_name() -> String {
    format!(
        "PLaunch-{}.zip",
        chrono::Local::now().format("%Y-%m-%d_%H-%M")
    )
}

/// Crée l'archive ; renvoie le nombre de fichiers sauvegardés.
pub fn export(dest: &Path, targets: &Targets) -> Result<usize, String> {
    let err = |e: &dyn std::fmt::Display| trf("sauvegarde : {}", &[e]);
    let file = File::create(dest).map_err(|e| err(&e))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut n = 0;
    for (name, path) in targets.pairs() {
        let Ok(data) = std::fs::read(path) else {
            continue; // fichier absent (pas encore de notes, par exemple)
        };
        zip.start_file(name, options).map_err(|e| err(&e))?;
        zip.write_all(&data).map_err(|e| err(&e))?;
        n += 1;
    }
    zip.finish().map_err(|e| err(&e))?;
    Ok(n)
}

/// Fichiers reconnus dans une archive.
pub fn contents(src: &Path) -> Result<Vec<String>, String> {
    let file = File::open(src).map_err(|e| e.to_string())?;
    let zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    Ok(zip
        .file_names()
        .filter(|n| [CONFIG, SETTINGS, NOTES].contains(n))
        .map(str::to_string)
        .collect())
}

/// Restaure les fichiers de l'archive. Les fichiers remplacés sont d'abord
/// copiés en `.avant-restauration`. Renvoie les noms restaurés.
pub fn restore(src: &Path, targets: &Targets) -> Result<Vec<String>, String> {
    let err = |e: &dyn std::fmt::Display| trf("restauration : {}", &[e]);
    let file = File::open(src).map_err(|e| err(&e))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| err(&e))?;
    // Lecture complète avant toute écriture : une archive abîmée ne laisse
    // pas une configuration à moitié restaurée.
    let mut found = Vec::new();
    for (name, path) in targets.pairs() {
        let Ok(mut entry) = zip.by_name(name) else {
            continue;
        };
        let mut data = Vec::new();
        entry
            .by_ref()
            .take(MAX_ENTRY)
            .read_to_end(&mut data)
            .map_err(|e| err(&e))?;
        found.push((name, path, data));
    }
    if !found.iter().any(|(n, ..)| *n == CONFIG) {
        return Err(err(&trf("{} absent de l'archive", &[&CONFIG])));
    }
    let mut restored = Vec::new();
    for (name, path, data) in found {
        if path.exists() {
            let mut keep = path.as_os_str().to_os_string();
            keep.push(".avant-restauration");
            std::fs::copy(path, PathBuf::from(keep)).map_err(|e| err(&e))?;
        }
        std::fs::write(path, data).map_err(|e| err(&e))?;
        restored.push(name.to_string());
    }
    Ok(restored)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_then_restore() {
        let dir = std::env::temp_dir().join(format!("plaunch-bak-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let t = Targets::for_config(&dir.join("PLaunch.xml"));
        std::fs::write(&t.config, "<start>\r\n</start>\r\n").unwrap();
        std::fs::write(&t.settings, "tray=false\r\n").unwrap();
        let zip = dir.join(default_name());
        assert_eq!(export(&zip, &t).unwrap(), 2); // pas encore de notes
        assert_eq!(contents(&zip).unwrap(), vec![CONFIG, SETTINGS]);

        std::fs::write(&t.config, "modifié").unwrap();
        let restored = restore(&zip, &t).unwrap();
        assert_eq!(restored, vec![CONFIG, SETTINGS]);
        assert_eq!(
            std::fs::read_to_string(&t.config).unwrap(),
            "<start>\r\n</start>\r\n"
        );
        let kept = dir.join("PLaunch.xml.avant-restauration");
        assert_eq!(std::fs::read_to_string(kept).unwrap(), "modifié");
        assert!(restore(&dir.join("absent.zip"), &t).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
