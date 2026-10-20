//! Journal `plaunch.log`, à côté de l'exécutable (ou dans le dossier
//! temporaire si la clé est en lecture seule). Garde aussi les plantages.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

pub const FILE_NAME: &str = "plaunch.log";
/// Au-delà, le journal est renommé en `.log.1` (un seul ancien gardé).
const MAX_SIZE: u64 = 512 * 1024;

static PATH: OnceLock<PathBuf> = OnceLock::new();
static LOCK: Mutex<()> = Mutex::new(());

/// Emplacement du journal (choisi au premier appel).
pub fn path() -> PathBuf {
    PATH.get_or_init(|| {
        let beside_exe = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|d| d.join(FILE_NAME)));
        match beside_exe {
            Some(p) if writable(&p) => p,
            _ => std::env::temp_dir().join(FILE_NAME),
        }
    })
    .clone()
}

fn writable(p: &PathBuf) -> bool {
    OpenOptions::new().create(true).append(true).open(p).is_ok()
}

fn write(level: &str, msg: &str) {
    let _guard = LOCK.lock();
    let path = path();
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_SIZE) {
        let mut old = path.as_os_str().to_os_string();
        old.push(".1");
        let _ = std::fs::rename(&path, PathBuf::from(old));
    }
    let line = format!(
        "{} [{level}] {}\r\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        msg.replace('\n', "\r\n    ")
    );
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line.as_bytes());
    }
    if cfg!(debug_assertions) {
        eprint!("{line}");
    }
}

pub fn info(msg: &str) {
    write("INFO", msg);
}

pub fn error(msg: &str) {
    write("ERREUR", msg);
}

/// Écrit les plantages dans le journal (avec la pile d'appels) et prévient
/// l'utilisateur, la version release n'ayant pas de console.
pub fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_default();
        let backtrace = std::backtrace::Backtrace::force_capture();
        error(&format!(
            "PLANTAGE v{} : {payload} ({location})\n{backtrace}",
            env!("CARGO_PKG_VERSION")
        ));
        crash_box(&format!(
            "{}\n\n{payload}\n\n{}",
            crate::i18n::tr("PLaunch a rencontré une erreur et va se fermer."),
            crate::i18n::trf("Détails dans {}", &[&path().display()])
        ));
        default(info);
    }));
}

#[cfg(windows)]
fn crash_box(text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    let caption: Vec<u16> = "PLaunch".encode_utf16().chain(Some(0)).collect();
    let text: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    // SAFETY : chaînes terminées par un zéro.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[cfg(not(windows))]
fn crash_box(text: &str) {
    eprintln!("{text}");
}
