//! Éjection de la clé USB : repère les programmes lancés depuis le lecteur,
//! les ferme (proprement puis, si demandé, de force) et éjecte le lecteur.
//!
//! PStart-rs tournant lui-même depuis la clé, l'éjection est confiée à un
//! processus PowerShell détaché qui attend la fermeture de PStart-rs.

/// Programme ouvert depuis le lecteur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proc {
    pub pid: u32,
    pub path: String,
}

impl Proc {
    pub fn name(&self) -> &str {
        self.path.rsplit(['\\', '/']).next().unwrap_or(&self.path)
    }
}

/// Le chemin désigne-t-il le lecteur `drive` ?
#[cfg_attr(not(windows), allow(dead_code))]
pub fn on_drive(path: &str, drive: char) -> bool {
    crate::paths::drive_letter(path) == Some(drive.to_ascii_uppercase())
}

/// Commande PowerShell d'éjection (verbe « Éjecter » de l'Explorateur).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn eject_script(drive: char, delay_s: u32) -> String {
    format!(
        "Start-Sleep -Seconds {delay_s}; \
         (New-Object -ComObject Shell.Application).Namespace(17).ParseName('{}:').InvokeVerb('Eject')",
        drive.to_ascii_uppercase()
    )
}

pub use imp::{can_eject, close_gracefully, eject, kill, processes_on};

#[cfg(windows)]
mod imp {
    use std::os::windows::process::CommandExt;

    use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
    use windows_sys::Win32::System::ProcessStatus::EnumProcesses;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, TerminateProcess, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_CLOSE,
    };

    use super::{on_drive, Proc};

    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    /// Lecteur amovible, ou disque USB « fixe » qui n'est pas celui du système.
    pub fn can_eject(drive: char) -> bool {
        let root: Vec<u16> = format!("{drive}:\\")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // SAFETY : chemin terminé par un zéro.
        let kind = unsafe { GetDriveTypeW(root.as_ptr()) };
        let system = std::env::var("SystemDrive")
            .ok()
            .and_then(|s| s.chars().next())
            .map(|c| c.to_ascii_uppercase());
        kind == DRIVE_REMOVABLE || (kind == DRIVE_FIXED && system != Some(drive))
    }

    fn image_path(pid: u32) -> Option<String> {
        // SAFETY : handle ouvert en lecture limitée et refermé ; tampon de
        // taille connue.
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return None;
            }
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
            CloseHandle(h);
            (ok != 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
        }
    }

    /// Processus dont l'exécutable est sur le lecteur (sauf PStart-rs).
    pub fn processes_on(drive: char) -> Vec<Proc> {
        let mut pids = vec![0u32; 4096];
        let mut needed = 0u32;
        // SAFETY : tampon de taille indiquée en octets.
        let ok = unsafe { EnumProcesses(pids.as_mut_ptr(), (pids.len() * 4) as u32, &mut needed) };
        if ok == 0 {
            return Vec::new();
        }
        pids.truncate(needed as usize / 4);
        let me = std::process::id();
        let mut out: Vec<Proc> = pids
            .into_iter()
            .filter(|&p| p != 0 && p != me)
            .filter_map(|pid| image_path(pid).map(|path| Proc { pid, path }))
            .filter(|p| on_drive(&p.path, drive))
            .collect();
        out.sort_by_key(|p| p.name().to_lowercase());
        out
    }

    unsafe extern "system" fn close_window(hwnd: HWND, lparam: LPARAM) -> i32 {
        let pids = &*(lparam as *const Vec<u32>);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pids.contains(&pid) && IsWindowVisible(hwnd) != 0 {
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
        }
        1
    }

    /// Demande la fermeture des fenêtres de ces processus (comme la croix :
    /// le programme peut proposer d'enregistrer).
    pub fn close_gracefully(pids: &[u32]) {
        let list: Vec<u32> = pids.to_vec();
        // SAFETY : `list` vit pendant tout l'appel synchrone d'EnumWindows.
        unsafe {
            EnumWindows(Some(close_window), &list as *const Vec<u32> as LPARAM);
        }
    }

    /// Termine les processus restants (données non enregistrées perdues).
    pub fn kill(pids: &[u32]) {
        for &pid in pids {
            // SAFETY : handle ouvert pour terminer, puis refermé.
            unsafe {
                let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
                if !h.is_null() {
                    TerminateProcess(h, 1);
                    CloseHandle(h);
                }
            }
        }
    }

    /// Lance l'éjection différée ; PStart-rs doit ensuite se fermer.
    pub fn eject(drive: char) -> Result<(), String> {
        let windows = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-Command",
            ])
            .arg(super::eject_script(drive, 3))
            // Dossier courant hors de la clé, sinon elle resterait occupée.
            .current_dir(format!("{windows}\\System32"))
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

#[cfg(not(windows))]
mod imp {
    use super::Proc;

    pub fn can_eject(_drive: char) -> bool {
        false
    }
    pub fn processes_on(_drive: char) -> Vec<Proc> {
        Vec::new()
    }
    pub fn close_gracefully(_pids: &[u32]) {}
    pub fn kill(_pids: &[u32]) {}
    pub fn eject(_drive: char) -> Result<(), String> {
        Err(crate::i18n::tr("disponible sous Windows uniquement").into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert!(on_drive("e:\\Apps\\vlc.exe", 'E'));
        assert!(!on_drive("C:\\Windows\\x.exe", 'E'));
        let p = Proc {
            pid: 1,
            path: "E:\\Apps\\VLC\\vlc.exe".into(),
        };
        assert_eq!(p.name(), "vlc.exe");
        let s = eject_script('f', 3);
        assert!(s.contains("ParseName('F:')") && s.starts_with("Start-Sleep -Seconds 3"));
    }
}
