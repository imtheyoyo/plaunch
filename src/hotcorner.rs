//! Coin ou bord actif : amener la souris dans un coin (ou contre un bord) de
//! l'écran principal et l'y laisser un court instant affiche la fenêtre.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::i18n::tr;
use crate::state::Shared;
use crate::window;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Corner {
    #[default]
    None,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Top,
    Bottom,
    Left,
    Right,
}

impl Corner {
    pub const ALL: [Corner; 9] = [
        Corner::None,
        Corner::TopLeft,
        Corner::TopRight,
        Corner::BottomLeft,
        Corner::BottomRight,
        Corner::Top,
        Corner::Bottom,
        Corner::Left,
        Corner::Right,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Corner::None => "none",
            Corner::TopLeft => "topleft",
            Corner::TopRight => "topright",
            Corner::BottomLeft => "bottomleft",
            Corner::BottomRight => "bottomright",
            Corner::Top => "top",
            Corner::Bottom => "bottom",
            Corner::Left => "left",
            Corner::Right => "right",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Corner::None => tr("désactivé"),
            Corner::TopLeft => tr("coin haut gauche"),
            Corner::TopRight => tr("coin haut droit"),
            Corner::BottomLeft => tr("coin bas gauche"),
            Corner::BottomRight => tr("coin bas droit"),
            Corner::Top => tr("bord haut"),
            Corner::Bottom => tr("bord bas"),
            Corner::Left => tr("bord gauche"),
            Corner::Right => tr("bord droit"),
        }
    }

    pub fn from_key(s: &str) -> Corner {
        Corner::ALL
            .into_iter()
            .find(|c| c.key().eq_ignore_ascii_case(s.trim()))
            .unwrap_or(Corner::None)
    }

    /// Le point `(x, y)` est-il dans la zone, sur un écran `w`×`h` ?
    /// `size` est la taille des coins ; un bord se déclenche contre l'écran.
    pub fn contains(self, x: i32, y: i32, w: i32, h: i32, size: i32) -> bool {
        let left = x < size;
        let right = x >= w - size;
        let top = y < size;
        let bottom = y >= h - size;
        match self {
            Corner::None => false,
            Corner::TopLeft => left && top,
            Corner::TopRight => right && top,
            Corner::BottomLeft => left && bottom,
            Corner::BottomRight => right && bottom,
            Corner::Top => y <= 0 && !left && !right,
            Corner::Bottom => y >= h - 1 && !left && !right,
            Corner::Left => x <= 0 && !top && !bottom,
            Corner::Right => x >= w - 1 && !top && !bottom,
        }
    }
}

/// Surveille la position du curseur (Windows). La configuration est relue
/// à chaque tour pour pouvoir être modifiée à chaud.
pub fn spawn(shared: Arc<Shared>) {
    if !cfg!(windows) {
        return;
    }
    std::thread::spawn(move || {
        let mut entered: Option<Instant> = None;
        let mut fired = false;
        loop {
            std::thread::sleep(Duration::from_millis(50));
            if shared.quitting.load(Ordering::SeqCst) {
                return;
            }
            let (corner, delay, size) = *shared.hot_corner.lock().unwrap();
            if corner == Corner::None {
                entered = None;
                fired = false;
                std::thread::sleep(Duration::from_millis(450));
                continue;
            }
            let Some((x, y, w, h)) = imp::cursor_and_screen() else {
                continue;
            };
            if corner.contains(x, y, w, h, size) {
                let since = *entered.get_or_insert_with(Instant::now);
                if !fired && since.elapsed() >= Duration::from_millis(delay) {
                    fired = true;
                    shared.focus_search.store(true, Ordering::SeqCst);
                    window::show();
                    shared.repaint();
                }
            } else {
                entered = None;
                fired = false;
            }
        }
    });
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN,
    };

    pub fn cursor_and_screen() -> Option<(i32, i32, i32, i32)> {
        let mut p = POINT { x: 0, y: 0 };
        // SAFETY : lecture de la position du curseur et des dimensions de
        // l'écran principal.
        unsafe {
            if GetCursorPos(&mut p) == 0 {
                return None;
            }
            Some((
                p.x,
                p.y,
                GetSystemMetrics(SM_CXSCREEN),
                GetSystemMetrics(SM_CYSCREEN),
            ))
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn cursor_and_screen() -> Option<(i32, i32, i32, i32)> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zones() {
        let (w, h, s) = (1920, 1080, 20);
        assert!(Corner::TopLeft.contains(0, 0, w, h, s));
        assert!(Corner::TopLeft.contains(19, 19, w, h, s));
        assert!(!Corner::TopLeft.contains(20, 5, w, h, s));
        assert!(Corner::BottomRight.contains(1919, 1079, w, h, s));
        assert!(Corner::Top.contains(900, 0, w, h, s));
        assert!(!Corner::Top.contains(900, 3, w, h, s));
        assert!(!Corner::Top.contains(5, 0, w, h, s)); // c'est un coin
        assert!(Corner::Right.contains(1919, 500, w, h, s));
        assert!(!Corner::None.contains(0, 0, w, h, s));
    }

    #[test]
    fn keys() {
        for c in Corner::ALL {
            assert_eq!(Corner::from_key(c.key()), c);
        }
        assert_eq!(Corner::from_key("TopLeft"), Corner::TopLeft);
        assert_eq!(Corner::from_key("?"), Corner::None);
    }
}
