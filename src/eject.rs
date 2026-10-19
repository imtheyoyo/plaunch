//! Éjection de la clé USB : repère les programmes lancés depuis le lecteur,
//! les ferme (proprement puis, si demandé, de force) et éjecte le lecteur.
//!
//! PStart-rs tournant lui-même depuis la clé, l'éjection est confiée à une
//! copie de l'exécutable placée dans le dossier temporaire (`--eject-helper`) :
//! elle attend la fin de PStart-rs puis demande à Windows de retirer le
//! périphérique (`CM_Request_Device_EjectW`, comme « Retirer le
//! périphérique en toute sécurité »), et affiche le résultat.

/// Argument du mode « assistant d'éjection ».
pub const HELPER_ARG: &str = "--eject-helper";
/// Nom de la copie temporaire.
pub const HELPER_EXE: &str = "pstart-rs-eject.exe";

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

/// Arguments du mode assistant : `--eject-helper E 1234`.
pub fn parse_helper_args(args: &[String]) -> Option<(char, u32)> {
    if args.first().map(String::as_str) != Some(HELPER_ARG) {
        return None;
    }
    let drive = args
        .get(1)?
        .chars()
        .next()
        .filter(char::is_ascii_alphabetic)?;
    let pid = args.get(2)?.parse().ok()?;
    Some((drive.to_ascii_uppercase(), pid))
}

/// Supprime une ancienne copie de l'assistant.
pub fn cleanup_helper() {
    let _ = std::fs::remove_file(std::env::temp_dir().join(HELPER_EXE));
}

pub use imp::{
    activate, all_processes, can_eject, close_gracefully, eject, kill, processes_on, run_helper,
};

/// Chemin d'exécutable normalisé pour comparer (minuscules, `\`).
pub fn path_key(p: &str) -> String {
    p.trim().replace('/', "\\").to_lowercase()
}

#[cfg(windows)]
mod imp {
    use std::mem::size_of;
    use std::os::windows::process::CommandExt;

    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_Parent, CM_Request_Device_EjectW, SetupDiDestroyDeviceInfoList,
        SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
        CR_SUCCESS, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, PNP_VETO_TYPE, SP_DEVICE_INTERFACE_DATA,
        SP_DEVICE_INTERFACE_DETAIL_DATA_W, SP_DEVINFO_DATA,
    };
    use windows_sys::Win32::Foundation::{CloseHandle, HWND, INVALID_HANDLE_VALUE, LPARAM};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, GetDriveTypeW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Ioctl::{
        GUID_DEVINTERFACE_DISK, IOCTL_STORAGE_GET_DEVICE_NUMBER, STORAGE_DEVICE_NUMBER,
    };
    use windows_sys::Win32::System::ProcessStatus::EnumProcesses;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
        PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE as SYNCHRONIZE,
        PROCESS_TERMINATE,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowTextLengthW, GetWindowThreadProcessId, IsIconic,
        IsWindowVisible, MessageBoxW, PostMessageW, SetForegroundWindow, ShowWindow, GW_OWNER,
        MB_ICONINFORMATION, MB_ICONWARNING, MB_OK, MB_SETFOREGROUND, MB_TOPMOST, SW_RESTORE,
        WM_CLOSE,
    };

    use super::{on_drive, Proc};

    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
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
        let mut out: Vec<Proc> = all_processes()
            .into_iter()
            .filter(|p| on_drive(&p.path, drive))
            .collect();
        out.sort_by_key(|p| p.name().to_lowercase());
        out
    }

    /// Tous les processus lisibles (sauf PStart-rs), avec leur exécutable.
    pub fn all_processes() -> Vec<Proc> {
        let mut pids = vec![0u32; 4096];
        let mut needed = 0u32;
        // SAFETY : tampon de taille indiquée en octets.
        let ok = unsafe { EnumProcesses(pids.as_mut_ptr(), (pids.len() * 4) as u32, &mut needed) };
        if ok == 0 {
            return Vec::new();
        }
        pids.truncate(needed as usize / 4);
        let me = std::process::id();
        pids.into_iter()
            .filter(|&p| p != 0 && p != me)
            .filter_map(|pid| image_path(pid).map(|path| Proc { pid, path }))
            .collect()
    }

    unsafe extern "system" fn find_window(hwnd: HWND, lparam: LPARAM) -> i32 {
        let state = &mut *(lparam as *mut (Vec<u32>, HWND));
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if state.0.contains(&pid)
            && IsWindowVisible(hwnd) != 0
            && GetWindow(hwnd, GW_OWNER).is_null()
            && GetWindowTextLengthW(hwnd) > 0
        {
            state.1 = hwnd;
            return 0; // trouvé : arrêt de l'énumération
        }
        1
    }

    /// Met au premier plan la fenêtre principale d'un de ces processus.
    pub fn activate(pids: &[u32]) -> bool {
        let mut state: (Vec<u32>, HWND) = (pids.to_vec(), std::ptr::null_mut());
        // SAFETY : `state` vit pendant l'appel synchrone d'EnumWindows ; le
        // handle trouvé est utilisé aussitôt.
        unsafe {
            EnumWindows(Some(find_window), &mut state as *mut _ as LPARAM);
            let w = state.1;
            if w.is_null() {
                return false;
            }
            if IsIconic(w) != 0 {
                ShowWindow(w, SW_RESTORE);
            }
            SetForegroundWindow(w) != 0
        }
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

    /// Copie l'exécutable dans le dossier temporaire et lance l'assistant
    /// d'éjection ; PStart-rs doit ensuite se fermer.
    pub fn eject(drive: char) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let helper = std::env::temp_dir().join(super::HELPER_EXE);
        std::fs::copy(&exe, &helper).map_err(|e| e.to_string())?;
        let windows = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        std::process::Command::new(&helper)
            .arg(super::HELPER_ARG)
            .arg(drive.to_string())
            .arg(std::process::id().to_string())
            // Dossier courant hors de la clé, sinon elle resterait occupée.
            .current_dir(format!("{windows}\\System32"))
            .creation_flags(DETACHED_PROCESS)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// Mode assistant : attend la fin de PStart-rs puis éjecte.
    pub fn run_helper(drive: char, pid: u32) {
        crate::log::info(&format!(
            "assistant d'éjection : {drive}: (attente du processus {pid})"
        ));
        // SAFETY : handle de synchronisation ouvert puis refermé.
        unsafe {
            let h = OpenProcess(SYNCHRONIZE, 0, pid);
            if !h.is_null() {
                WaitForSingleObject(h, 20_000);
                CloseHandle(h);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(800));
        let mut result = Err(String::new());
        for _ in 0..6 {
            result = eject_now(drive);
            if result.is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1000));
        }
        let (text, flags) = match &result {
            Ok(()) => (
                crate::i18n::trf("La clé {}: peut être retirée en toute sécurité.", &[&drive]),
                MB_ICONINFORMATION,
            ),
            Err(e) => (
                crate::i18n::trf("Windows refuse d'éjecter la clé {}: : {}", &[&drive, e]),
                MB_ICONWARNING,
            ),
        };
        match &result {
            Ok(()) => crate::log::info(&format!("éjection de {drive}: réussie")),
            Err(e) => crate::log::error(&format!("éjection de {drive}: refusée : {e}")),
        }
        let caption: Vec<u16> = "PStart-rs".encode_utf16().chain(Some(0)).collect();
        let text: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        // SAFETY : chaînes terminées par un zéro.
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                caption.as_ptr(),
                MB_OK | flags | MB_TOPMOST | MB_SETFOREGROUND,
            );
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// Numéro de disque physique d'un volume (`\\.\E:`) ou d'une interface.
    fn device_number(path: &str) -> Option<(u32, u32)> {
        let p = wide(path);
        // SAFETY : ouverture sans droit d'accès (requête d'information seule),
        // tampon de sortie de la taille de la structure.
        unsafe {
            let h = CreateFileW(
                p.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            );
            if h == INVALID_HANDLE_VALUE {
                return None;
            }
            let mut n = STORAGE_DEVICE_NUMBER {
                DeviceType: 0,
                DeviceNumber: 0,
                PartitionNumber: 0,
            };
            let mut ret = 0u32;
            let ok = DeviceIoControl(
                h,
                IOCTL_STORAGE_GET_DEVICE_NUMBER,
                std::ptr::null(),
                0,
                &mut n as *mut _ as *mut core::ffi::c_void,
                size_of::<STORAGE_DEVICE_NUMBER>() as u32,
                &mut ret,
                std::ptr::null_mut(),
            );
            CloseHandle(h);
            (ok != 0).then_some((n.DeviceType, n.DeviceNumber))
        }
    }

    /// Instance de périphérique du disque qui porte le numéro donné.
    fn disk_devinst(number: (u32, u32)) -> Option<u32> {
        // SAFETY : énumération SetupAPI classique ; tampon de détail aligné
        // sur 8 octets et `cbSize` renseigné comme l'exige l'API.
        unsafe {
            let set = SetupDiGetClassDevsW(
                &GUID_DEVINTERFACE_DISK,
                std::ptr::null(),
                std::ptr::null_mut(),
                DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
            );
            if set == -1 {
                return None;
            }
            let mut found = None;
            let mut index = 0;
            loop {
                let mut iface = SP_DEVICE_INTERFACE_DATA {
                    cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
                    ..std::mem::zeroed()
                };
                if SetupDiEnumDeviceInterfaces(
                    set,
                    std::ptr::null(),
                    &GUID_DEVINTERFACE_DISK,
                    index,
                    &mut iface,
                ) == 0
                {
                    break;
                }
                index += 1;
                let mut buf = vec![0u64; 512];
                let detail = buf.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
                (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
                let mut info = SP_DEVINFO_DATA {
                    cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
                    ..std::mem::zeroed()
                };
                if SetupDiGetDeviceInterfaceDetailW(
                    set,
                    &iface,
                    detail,
                    (buf.len() * 8) as u32,
                    std::ptr::null_mut(),
                    &mut info,
                ) == 0
                {
                    continue;
                }
                let path_ptr = std::ptr::addr_of!((*detail).DevicePath) as *const u16;
                let mut len = 0;
                while *path_ptr.add(len) != 0 {
                    len += 1;
                }
                let path = String::from_utf16_lossy(std::slice::from_raw_parts(path_ptr, len));
                if device_number(&path) == Some(number) {
                    found = Some(info.DevInst);
                    break;
                }
            }
            SetupDiDestroyDeviceInfoList(set);
            found
        }
    }

    /// Demande à Windows de retirer le périphérique qui porte le lecteur.
    pub fn eject_now(drive: char) -> Result<(), String> {
        let number = device_number(&format!("\\\\.\\{drive}:"))
            .ok_or_else(|| crate::i18n::tr("lecteur introuvable").to_string())?;
        let devinst = disk_devinst(number)
            .ok_or_else(|| crate::i18n::tr("périphérique introuvable").to_string())?;
        // SAFETY : appels CfgMgr32 sur une instance obtenue ci-dessus ; tampon
        // de nom de veto de taille indiquée.
        unsafe {
            let mut parent = 0u32;
            let target = if CM_Get_Parent(&mut parent, devinst, 0) == CR_SUCCESS {
                parent
            } else {
                devinst
            };
            let mut veto: PNP_VETO_TYPE = 0;
            let mut name = [0u16; 512];
            let r = CM_Request_Device_EjectW(
                target,
                &mut veto,
                name.as_mut_ptr(),
                name.len() as u32,
                0,
            );
            if r == CR_SUCCESS && veto == 0 {
                return Ok(());
            }
            let len = name.iter().position(|&c| c == 0).unwrap_or(0);
            let who = String::from_utf16_lossy(&name[..len]);
            Err(super::veto_message(veto, &who))
        }
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
    /// Linux : lecture de /proc (utile pour les essais).
    pub fn all_processes() -> Vec<Proc> {
        let me = std::process::id();
        let Ok(dir) = std::fs::read_dir("/proc") else {
            return Vec::new();
        };
        dir.flatten()
            .filter_map(|e| {
                let pid: u32 = e.file_name().to_str()?.parse().ok()?;
                if pid == me {
                    return None;
                }
                let exe = std::fs::read_link(e.path().join("exe")).ok()?;
                Some(Proc {
                    pid,
                    path: exe.to_string_lossy().into_owned(),
                })
            })
            .collect()
    }
    pub fn activate(_pids: &[u32]) -> bool {
        false
    }
    pub fn close_gracefully(_pids: &[u32]) {}
    pub fn kill(_pids: &[u32]) {}
    pub fn eject(_drive: char) -> Result<(), String> {
        Err(crate::i18n::tr("disponible sous Windows uniquement").into())
    }
    pub fn run_helper(_drive: char, _pid: u32) {}
}

/// Explication lisible d'un refus d'éjection (types `PNP_VETO_TYPE`).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn veto_message(veto: i32, who: &str) -> String {
    use crate::i18n::{tr, trf};
    let who = who.trim();
    match veto {
        // Un programme garde un fichier ouvert sur la clé.
        5 | 6 if !who.is_empty() => trf("fichier ouvert par {}", &[&who]),
        5 | 6 => tr("un fichier de la clé est encore ouvert").to_string(),
        // Périphérique ou service qui bloque.
        _ if !who.is_empty() => trf("utilisé par {} (code {})", &[&who, &veto]),
        0 => tr("refus sans motif (fermez les fenêtres ouvertes sur la clé)").to_string(),
        _ => trf("refus de Windows (code {})", &[&veto]),
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
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            parse_helper_args(&args(&[HELPER_ARG, "e", "42"])),
            Some(('E', 42))
        );
        assert_eq!(parse_helper_args(&args(&["PStart.xml"])), None);
        assert_eq!(parse_helper_args(&args(&[HELPER_ARG, "1", "x"])), None);
        assert!(veto_message(6, "\\Device\\Notepad2.exe").contains("Notepad2"));
        assert!(veto_message(0, "").contains("refus"));
    }
}
