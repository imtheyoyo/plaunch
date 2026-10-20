//! Démarrage avec Windows (clé `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`).
//!
//! La valeur s'appelle `PLaunch`. Le chemin enregistré est absolu : si la clé
//! USB change de lettre, il suffit de réactiver l'option (PLaunch le fait
//! aussi de lui-même au démarrage).

use std::path::Path;

const VALUE_NAME: &str = "PLaunch";

/// Ligne de commande enregistrée : exécutable courant + fichier PLaunch.xml.
pub fn command_line(config: Option<&Path>) -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = format!("\"{}\"", exe.display());
    if let Some(c) = config {
        cmd.push_str(&format!(" \"{}\"", c.display()));
    }
    Ok(cmd)
}

#[cfg(windows)]
mod imp {
    use std::ptr;

    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
        HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SZ,
    };

    const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn open(write: bool) -> Result<HKEY, String> {
        let key = wide(RUN_KEY);
        let mut hkey: HKEY = ptr::null_mut();
        let access = if write {
            KEY_READ | KEY_SET_VALUE
        } else {
            KEY_READ
        };
        // SAFETY : clé et chemin valides, handle de sortie initialisé.
        let r = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, key.as_ptr(), 0, access, &mut hkey) };
        if r == ERROR_SUCCESS {
            Ok(hkey)
        } else {
            Err(crate::i18n::trf("registre inaccessible (code {})", &[&r]))
        }
    }

    pub fn is_enabled(name: &str) -> bool {
        let Ok(hkey) = open(false) else { return false };
        let name = wide(name);
        // SAFETY : simple test d'existence, sans tampon de sortie.
        unsafe {
            let r = RegQueryValueExW(
                hkey,
                name.as_ptr(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            );
            RegCloseKey(hkey);
            r == ERROR_SUCCESS
        }
    }

    pub fn enable(name: &str, command: &str) -> Result<(), String> {
        let hkey = open(true)?;
        let name = wide(name);
        let data = wide(command);
        // SAFETY : données REG_SZ terminées par un zéro, taille en octets.
        let r = unsafe {
            let r = RegSetValueExW(
                hkey,
                name.as_ptr(),
                0,
                REG_SZ,
                data.as_ptr() as *const u8,
                (data.len() * 2) as u32,
            );
            RegCloseKey(hkey);
            r
        };
        if r == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(crate::i18n::trf(
                "écriture du registre impossible (code {})",
                &[&r],
            ))
        }
    }

    pub fn disable(name: &str) -> Result<(), String> {
        let hkey = open(true)?;
        let name = wide(name);
        // SAFETY : suppression d'une valeur de la clé ouverte.
        unsafe {
            RegDeleteValueW(hkey, name.as_ptr());
            RegCloseKey(hkey);
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn is_enabled(_name: &str) -> bool {
        false
    }
    pub fn enable(_name: &str, _command: &str) -> Result<(), String> {
        Err(crate::i18n::tr("disponible sous Windows uniquement").into())
    }
    pub fn disable(_name: &str) -> Result<(), String> {
        Ok(())
    }
}

pub fn is_enabled() -> bool {
    imp::is_enabled(VALUE_NAME)
}

pub fn set(enabled: bool, config: Option<&Path>) -> Result<(), String> {
    if enabled {
        imp::enable(VALUE_NAME, &command_line(config)?)
    } else {
        imp::disable(VALUE_NAME)
    }
}
