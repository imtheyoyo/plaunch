//! Lancement des programmes, dossiers et documents.
//!
//! Sous Windows, tout passe par `ShellExecuteW` (verbe `open`) : les `.exe`
//! démarrent, les dossiers s'ouvrent dans l'Explorateur et les documents
//! (`.txt`, `.chm`, `.jar`, `.cmd`…) dans leur application associée.

use crate::config::WindowState;

/// Comment lancer un raccourci.
#[derive(Debug, Clone, Copy, Default)]
pub struct Launch<'a> {
    pub params: Option<&'a str>,
    pub workdir: Option<&'a str>,
    pub window: WindowState,
    /// Verbe `runas` : demande d'élévation (UAC).
    pub admin: bool,
}

/// Lance `target` avec `workdir` comme dossier de travail.
pub fn launch(target: &str, workdir: Option<&str>) -> Result<(), String> {
    launch_with(
        target,
        &Launch {
            workdir,
            ..Default::default()
        },
    )
}

/// Lance `target` avec paramètres, état de fenêtre et élévation éventuelle.
pub fn launch_with(target: &str, how: &Launch) -> Result<(), String> {
    let params = how.params.map(str::trim).filter(|p| !p.is_empty());
    imp::run(target, params, how.workdir, how.window, how.admin)
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
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SW_SHOWMAXIMIZED, SW_SHOWMINNOACTIVE, SW_SHOWNORMAL,
    };

    use crate::config::WindowState;

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    pub fn open(file: &str, params: Option<&str>, workdir: Option<&str>) -> Result<(), String> {
        run(file, params, workdir, WindowState::Normal, false)
    }

    pub fn run(
        file: &str,
        params: Option<&str>,
        workdir: Option<&str>,
        window: WindowState,
        admin: bool,
    ) -> Result<(), String> {
        let verb = wide(if admin { "runas" } else { "open" });
        let show = match window {
            WindowState::Normal => SW_SHOWNORMAL,
            WindowState::Minimized => SW_SHOWMINNOACTIVE,
            WindowState::Maximized => SW_SHOWMAXIMIZED,
        };
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
                show,
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
            0 => crate::i18n::tr("mémoire insuffisante").into(),
            2 => crate::i18n::tr("fichier introuvable").into(),
            3 => crate::i18n::tr("chemin introuvable").into(),
            5 => crate::i18n::tr("accès refusé (ou élévation annulée)").into(),
            8 => crate::i18n::tr("mémoire insuffisante").into(),
            11 => crate::i18n::tr("exécutable invalide").into(),
            26 => crate::i18n::tr("violation de partage").into(),
            27 => crate::i18n::tr("association de fichier incomplète").into(),
            31 => crate::i18n::tr("aucune application associée à ce type de fichier").into(),
            32 => crate::i18n::tr("DLL introuvable").into(),
            c => crate::i18n::trf("échec du lancement (code {})", &[&c]),
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::process::Command;

    use crate::config::WindowState;
    use crate::paths::to_native;

    pub fn run(
        file: &str,
        params: Option<&str>,
        workdir: Option<&str>,
        _window: WindowState,
        _admin: bool,
    ) -> Result<(), String> {
        open(file, params, workdir)
    }

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
