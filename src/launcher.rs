//! Lancement des programmes, dossiers et documents.
//!
//! Sous Windows, tout passe par `ShellExecuteW` (verbe `open`) : les `.exe`
//! démarrent, les dossiers s'ouvrent dans l'Explorateur et les documents
//! (`.txt`, `.chm`, `.jar`, `.cmd`…) dans leur application associée.

/// Lance `target` avec `workdir` comme dossier de travail.
pub fn launch(target: &str, workdir: Option<&str>) -> Result<(), String> {
    imp::open(target, None, workdir)
}

/// Ouvre l'Explorateur sur le dossier contenant `target`, fichier sélectionné.
pub fn reveal(target: &str) -> Result<(), String> {
    imp::reveal(target)
}

#[cfg(windows)]
mod imp {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    pub fn open(file: &str, params: Option<&str>, workdir: Option<&str>) -> Result<(), String> {
        let verb = wide("open");
        let file_w = wide(file);
        let params_w = params.map(wide);
        let dir_w = workdir.map(wide);
        // SAFETY : toutes les chaînes sont terminées par un zéro et vivent
        // jusqu'à la fin de l'appel ; les pointeurs nuls sont acceptés.
        let result = unsafe {
            ShellExecuteW(
                ptr::null_mut(),
                verb.as_ptr(),
                file_w.as_ptr(),
                params_w.as_ref().map_or(ptr::null(), |p| p.as_ptr()),
                dir_w.as_ref().map_or(ptr::null(), |d| d.as_ptr()),
                SW_SHOWNORMAL,
            )
        };
        // ShellExecute renvoie une valeur > 32 en cas de succès.
        let code = result as isize;
        if code > 32 {
            Ok(())
        } else {
            Err(format!("{file} : {}", describe(code)))
        }
    }

    pub fn reveal(target: &str) -> Result<(), String> {
        let params = format!("/select,\"{target}\"");
        open("explorer.exe", Some(&params), None)
    }

    fn describe(code: isize) -> String {
        match code {
            0 => "mémoire insuffisante".into(),
            2 => "fichier introuvable".into(),
            3 => "chemin introuvable".into(),
            5 => "accès refusé".into(),
            8 => "mémoire insuffisante".into(),
            11 => "exécutable invalide".into(),
            26 => "violation de partage".into(),
            27 => "association de fichier incomplète".into(),
            31 => "aucune application associée à ce type de fichier".into(),
            32 => "DLL introuvable".into(),
            c => format!("échec du lancement (code {c})"),
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::process::Command;

    use crate::paths::to_native;

    pub fn open(file: &str, params: Option<&str>, workdir: Option<&str>) -> Result<(), String> {
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        let mut cmd = Command::new(opener);
        cmd.arg(to_native(file));
        if let Some(p) = params {
            cmd.arg(p);
        }
        if let Some(d) = workdir {
            let d = to_native(d);
            if d.is_dir() {
                cmd.current_dir(d);
            }
        }
        cmd.spawn().map(|_| ()).map_err(|e| format!("{file} : {e}"))
    }

    pub fn reveal(target: &str) -> Result<(), String> {
        let parent = crate::paths::parent(target).unwrap_or_else(|| target.to_string());
        open(&parent, None, None)
    }
}
