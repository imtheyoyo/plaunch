//! Affichage/masquage de la fenêtre principale par l'API Win32.
//!
//! Une fenêtre egui masquée ne reçoit plus de rafraîchissement : les
//! actions venant de l'icône de notification ou du raccourci global agissent
//! donc directement sur la fenêtre native, sans passer par la boucle egui.

use std::sync::atomic::{AtomicIsize, Ordering};

/// Handle natif de la fenêtre principale (0 tant qu'il est inconnu).
static HWND: AtomicIsize = AtomicIsize::new(0);

// Titre distinctif : sert aussi à retrouver la fenêtre (instance unique),
// il ne doit pas pouvoir correspondre à un dossier ouvert dans l'Explorateur.
// Le titre est traduit : l'instance unique cherche les deux variantes.
pub const TITLE: &str = "PStart-rs · Lanceur portable";

/// Titre dans la langue de l'interface.
pub fn title() -> &'static str {
    crate::i18n::tr(TITLE)
}

pub fn set_handle(hwnd: isize) {
    HWND.store(hwnd, Ordering::SeqCst);
}

pub fn handle() -> Option<isize> {
    match HWND.load(Ordering::SeqCst) {
        0 => None,
        h => Some(h),
    }
}

/// Récupère le handle natif depuis le contexte de création eframe.
pub fn capture(cc: &eframe::CreationContext<'_>) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    if let Ok(h) = cc.window_handle() {
        if let RawWindowHandle::Win32(w) = h.as_raw() {
            set_handle(w.hwnd.get());
        }
    }
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetForegroundWindow, IsIconic, IsWindowVisible, PostMessageW,
        SetForegroundWindow, ShowWindow, SW_HIDE, SW_RESTORE, SW_SHOW, WM_CLOSE,
    };

    fn hwnd(h: isize) -> HWND {
        h as HWND
    }

    pub fn show(h: isize) {
        // SAFETY : appels Win32 sur un handle de fenêtre ; un handle périmé
        // fait simplement échouer l'appel.
        unsafe {
            let w = hwnd(h);
            ShowWindow(w, SW_SHOW);
            if IsIconic(w) != 0 {
                ShowWindow(w, SW_RESTORE);
            }
            SetForegroundWindow(w);
        }
    }

    pub fn hide(h: isize) {
        unsafe {
            ShowWindow(hwnd(h), SW_HIDE);
        }
    }

    pub fn is_shown_and_active(h: isize) -> bool {
        unsafe {
            let w = hwnd(h);
            IsWindowVisible(w) != 0 && IsIconic(w) == 0 && GetForegroundWindow() == w
        }
    }

    pub fn post_close(h: isize) -> bool {
        unsafe { PostMessageW(hwnd(h), WM_CLOSE, 0, 0) != 0 }
    }

    /// Met au premier plan une autre instance déjà ouverte.
    pub fn activate_existing(title: &str) -> bool {
        let title_w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY : chaîne terminée par un zéro ; classe nulle acceptée.
        let w = unsafe { FindWindowW(std::ptr::null(), title_w.as_ptr()) };
        if w.is_null() {
            return false;
        }
        show(w as isize);
        true
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn show(_h: isize) {}
    pub fn hide(_h: isize) {}
    pub fn is_shown_and_active(_h: isize) -> bool {
        true
    }
    pub fn post_close(_h: isize) -> bool {
        false
    }
    pub fn activate_existing(_title: &str) -> bool {
        false
    }
}

pub fn show() {
    if let Some(h) = handle() {
        imp::show(h);
    }
}

pub fn hide() {
    if let Some(h) = handle() {
        imp::hide(h);
    }
}

/// Masque la fenêtre si elle est au premier plan, sinon l'affiche.
pub fn toggle() {
    if let Some(h) = handle() {
        if imp::is_shown_and_active(h) {
            imp::hide(h);
        } else {
            imp::show(h);
        }
    }
}

/// Demande la fermeture normale de la fenêtre (WM_CLOSE).
pub fn request_close() -> bool {
    handle().is_some_and(imp::post_close)
}

pub fn activate_existing() -> bool {
    imp::activate_existing(TITLE) || imp::activate_existing(crate::i18n::english(TITLE))
}
