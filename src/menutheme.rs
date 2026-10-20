//! Thème des menus natifs de Windows (menu de l'icône de notification).
//!
//! Ces menus sont dessinés par Windows, pas par l'interface : ils suivent le
//! mode « application » du processus, réglé par deux fonctions non
//! documentées mais stables de `uxtheme.dll` (ordinaux 135
//! `SetPreferredAppMode` et 136 `FlushMenuThemes`, Windows 10 1903 et +).

use crate::settings::Theme;

use std::sync::atomic::{AtomicI32, Ordering};

/// Mode appliqué en dernier (0 : aucun).
static CURRENT: AtomicI32 = AtomicI32::new(0);
/// Thème choisi, pour suivre les changements de Windows en mode « Système ».
static CHOSEN: AtomicI32 = AtomicI32::new(0);

const FORCE_DARK: i32 = 2;
const FORCE_LIGHT: i32 = 3;

/// Valeur de `PreferredAppMode` pour un thème. « Système » est résolu ici
/// (lecture du réglage de Windows) : le mode « AllowDark » ne suffit pas
/// pour le menu de notification, dont la fenêtre n'est pas la nôtre.
pub fn app_mode(theme: Theme, system_dark: bool) -> i32 {
    match theme {
        Theme::Dark => FORCE_DARK,
        Theme::Light => FORCE_LIGHT,
        Theme::System if system_dark => FORCE_DARK,
        Theme::System => FORCE_LIGHT,
    }
}

fn code(theme: Theme) -> i32 {
    match theme {
        Theme::System => 1,
        Theme::Light => 2,
        Theme::Dark => 3,
    }
}

/// Applique le thème aux menus natifs.
pub fn apply(theme: Theme) {
    CHOSEN.store(code(theme), Ordering::SeqCst);
    let mode = app_mode(theme, theme == Theme::System && imp::system_is_dark());
    if CURRENT.swap(mode, Ordering::SeqCst) != mode {
        imp::set_mode(mode);
    }
}

/// En mode « Système », suit un changement de thème de Windows (appel peu
/// coûteux, fait régulièrement).
pub fn follow_system() {
    if CHOSEN.load(Ordering::SeqCst) == code(Theme::System) {
        apply(Theme::System);
    }
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// Réglage « Mode des applications » de Windows.
    pub fn system_is_dark() -> bool {
        let key = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
        let value = wide("AppsUseLightTheme");
        let mut data = 1u32;
        let mut size = 4u32;
        // SAFETY : lecture d'une valeur DWORD dans un tampon de 4 octets.
        let r = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                &mut data as *mut u32 as *mut core::ffi::c_void,
                &mut size,
            )
        };
        r == ERROR_SUCCESS && data == 0
    }

    pub fn set_mode(mode: i32) {
        // SAFETY : fonctions chargées par ordinal depuis uxtheme.dll ; en
        // leur absence (ancien Windows), rien n'est appelé.
        unsafe {
            let lib = LoadLibraryW(wide("uxtheme.dll").as_ptr());
            if lib.is_null() {
                return;
            }
            if let Some(f) = GetProcAddress(lib, 135usize as *const u8) {
                let set_mode: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(f);
                set_mode(mode);
            }
            if let Some(f) = GetProcAddress(lib, 136usize as *const u8) {
                let flush: unsafe extern "system" fn() = std::mem::transmute(f);
                flush();
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn system_is_dark() -> bool {
        false
    }
    pub fn set_mode(_mode: i32) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes() {
        assert_eq!(app_mode(Theme::Dark, false), FORCE_DARK);
        assert_eq!(app_mode(Theme::Light, true), FORCE_LIGHT);
        assert_eq!(app_mode(Theme::System, true), FORCE_DARK);
        assert_eq!(app_mode(Theme::System, false), FORCE_LIGHT);
    }
}
