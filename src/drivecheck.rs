//! Vérification du lecteur au démarrage : lecture seule, place libre.

use std::path::Path;

use crate::i18n::{tr, trf};

/// En dessous, la clé est signalée comme presque pleine.
const LOW_SPACE: u64 = 50 * 1024 * 1024;

/// « 12,5 Mo », « 1,2 Go ».
pub fn format_size(bytes: u64) -> String {
    let (value, unit) = if bytes >= 1 << 30 {
        (bytes as f64 / (1u64 << 30) as f64, tr("Go"))
    } else if bytes >= 1 << 20 {
        (bytes as f64 / (1u64 << 20) as f64, tr("Mo"))
    } else {
        (bytes as f64 / 1024.0, tr("Ko"))
    };
    let text = format!("{value:.1}");
    let text = if crate::i18n::is_english() {
        text
    } else {
        text.replace('.', ",")
    };
    format!("{text} {unit}")
}

/// Avertissements pour le dossier de PLaunch.xml.
pub fn check(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let probe = dir.join(".plaunch-write-test");
    match std::fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
        }
        Err(_) => out.push(
            tr("Lecteur en lecture seule : les modifications ne pourront pas être enregistrées.")
                .to_string(),
        ),
    }
    if let Some(free) = free_space(dir) {
        if free < LOW_SPACE {
            out.push(trf(
                "Clé presque pleine : {} libres. Les enregistrements risquent d'échouer.",
                &[&format_size(free)],
            ));
        }
    }
    out
}

#[cfg(windows)]
fn free_space(dir: &Path) -> Option<u64> {
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = dir
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let mut free = 0u64;
    // SAFETY : chemin terminé par un zéro ; pointeurs facultatifs nuls.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(free)
}

#[cfg(not(windows))]
fn free_space(_dir: &Path) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_writable() {
        assert_eq!(format_size(12 * 1024 * 1024 + 512 * 1024), "12,5 Mo");
        assert_eq!(format_size(3 << 30), "3,0 Go");
        assert_eq!(format_size(2048), "2,0 Ko");
        assert!(check(&std::env::temp_dir()).is_empty());
        assert!(!check(Path::new("/dossier/inexistant/xyz")).is_empty());
    }
}
