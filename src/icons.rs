//! Icônes : extraction des icônes de programmes (Windows) et icône de
//! l'application dessinée par le code.

use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::state::Shared;

/// Image RGBA (alpha non prémultiplié).
#[derive(Clone, Debug)]
pub struct IconImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl IconImage {
    /// Réduction par moyenne de blocs (ex. 32 px → 16 px pour les menus).
    pub fn downscale(&self, size: u32) -> IconImage {
        if size == 0 || self.width <= size || self.height <= size {
            return self.clone();
        }
        let fx = self.width / size;
        let fy = self.height / size;
        let mut out = vec![0u8; (size * size * 4) as usize];
        for y in 0..size {
            for x in 0..size {
                let mut acc = [0u32; 4];
                let mut alpha_sum = 0u32;
                for dy in 0..fy {
                    for dx in 0..fx {
                        let sx = x * fx + dx;
                        let sy = y * fy + dy;
                        let i = ((sy * self.width + sx) * 4) as usize;
                        let a = self.rgba[i + 3] as u32;
                        // Moyenne pondérée par l'alpha pour éviter les franges.
                        acc[0] += self.rgba[i] as u32 * a;
                        acc[1] += self.rgba[i + 1] as u32 * a;
                        acc[2] += self.rgba[i + 2] as u32 * a;
                        alpha_sum += a;
                        acc[3] += a;
                    }
                }
                let n = fx * fy;
                let o = ((y * size + x) * 4) as usize;
                for c in 0..3 {
                    if let Some(v) = acc[c].checked_div(alpha_sum) {
                        out[o + c] = v as u8;
                    }
                }
                out[o + 3] = (acc[3] / n) as u8;
            }
        }
        IconImage {
            width: size,
            height: size,
            rgba: out,
        }
    }
}

/// Icône de l'application : carré arrondi bleu et triangle « lecture ».
pub fn app_icon() -> IconImage {
    const S: u32 = 32;
    let mut rgba = vec![0u8; (S * S * 4) as usize];
    let radius = 7.0f32;
    for y in 0..S {
        for x in 0..S {
            // Suréchantillonnage 4×4 pour l'anticrénelage.
            let mut cover_box = 0.0f32;
            let mut cover_tri = 0.0f32;
            for sy in 0..4 {
                for sx in 0..4 {
                    let px = x as f32 + (sx as f32 + 0.5) / 4.0;
                    let py = y as f32 + (sy as f32 + 0.5) / 4.0;
                    if in_rounded_rect(px, py, 1.0, 1.0, 31.0, 31.0, radius) {
                        cover_box += 1.0 / 16.0;
                        if in_triangle(px, py, (12.0, 8.5), (12.0, 23.5), (24.0, 16.0)) {
                            cover_tri += 1.0 / 16.0;
                        }
                    }
                }
            }
            let t = y as f32 / S as f32;
            // Dégradé vertical #3B82F6 → #1D4ED8.
            let bg = [
                lerp(59.0, 29.0, t),
                lerp(130.0, 78.0, t),
                lerp(246.0, 216.0, t),
            ];
            let mix = if cover_box > 0.0 {
                cover_tri / cover_box
            } else {
                0.0
            };
            let i = ((y * S + x) * 4) as usize;
            for c in 0..3 {
                rgba[i + c] = (bg[c] * (1.0 - mix) + 255.0 * mix).round() as u8;
            }
            rgba[i + 3] = (cover_box * 255.0).round() as u8;
        }
    }
    IconImage {
        width: S,
        height: S,
        rgba,
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn in_rounded_rect(x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> bool {
    if x < x0 || x > x1 || y < y0 || y > y1 {
        return false;
    }
    let cx = x.clamp(x0 + r, x1 - r);
    let cy = y.clamp(y0 + r, y1 - r);
    (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}

fn in_triangle(x: f32, y: f32, a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
    let sign = |p: (f32, f32), q: (f32, f32), r: (f32, f32)| {
        (p.0 - r.0) * (q.1 - r.1) - (q.0 - r.0) * (p.1 - r.1)
    };
    let p = (x, y);
    let d1 = sign(p, a, b);
    let d2 = sign(p, b, c);
    let d3 = sign(p, c, a);
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

/// Lance l'extraction des icônes en tâche de fond. `generation` permet
/// d'abandonner le travail si le fichier est rechargé entre-temps.
pub fn spawn_loader(targets: Vec<String>, shared: Arc<Shared>, generation: u64) {
    std::thread::spawn(move || {
        imp::init_thread();
        let mut seen = HashSet::new();
        for (n, target) in targets.into_iter().enumerate() {
            if shared.generation.load(Ordering::SeqCst) != generation {
                return;
            }
            if !seen.insert(target.clone()) {
                continue;
            }
            if let Some(img) = imp::extract(&target) {
                shared.icons.lock().unwrap().insert(target, img);
            }
            if n % 16 == 15 {
                shared.repaint();
            }
        }
        shared.icons_version.fetch_add(1, Ordering::SeqCst);
        shared.repaint();
    });
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::ptr;

    use windows_sys::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL;
    use windows_sys::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    use windows_sys::Win32::UI::Shell::{
        SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_USEFILEATTRIBUTES,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

    use super::IconImage;

    pub fn init_thread() {
        // SHGetFileInfo exige COM sur le thread appelant.
        // SAFETY : initialisation COM classique, une fois par thread.
        unsafe {
            CoInitializeEx(ptr::null(), COINIT_APARTMENTTHREADED as u32);
        }
    }

    pub fn extract(target: &str) -> Option<IconImage> {
        // Les chemins réseau peuvent bloquer longtemps : on se contente de
        // l'icône générique du type de fichier, sans accès au partage.
        let network = target.starts_with("\\\\");
        if !network {
            if let Some(img) = query(target, false) {
                return Some(img);
            }
        }
        query(target, true)
    }

    fn query(target: &str, generic: bool) -> Option<IconImage> {
        let wide: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
        let mut sfi = SHFILEINFOW::default();
        let mut flags = SHGFI_ICON | SHGFI_LARGEICON;
        let mut attrs = 0;
        if generic {
            flags |= SHGFI_USEFILEATTRIBUTES;
            attrs = FILE_ATTRIBUTE_NORMAL;
        }
        // SAFETY : chemin terminé par un zéro, structure de taille correcte.
        let ok = unsafe {
            SHGetFileInfoW(
                wide.as_ptr(),
                attrs,
                &mut sfi,
                size_of::<SHFILEINFOW>() as u32,
                flags,
            )
        };
        if ok == 0 || sfi.hIcon.is_null() {
            return None;
        }
        // SAFETY : icône valide renvoyée par le shell, détruite après usage.
        unsafe {
            let img = to_rgba(sfi.hIcon);
            DestroyIcon(sfi.hIcon);
            img
        }
    }

    fn header(w: i32, h: i32) -> BITMAPINFO {
        let mut bmi = BITMAPINFO::default();
        bmi.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // lignes de haut en bas
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        };
        bmi
    }

    unsafe fn to_rgba(hicon: HICON) -> Option<IconImage> {
        let mut info = ICONINFO::default();
        if GetIconInfo(hicon, &mut info) == 0 {
            return None;
        }
        let result = convert(&info);
        if !info.hbmColor.is_null() {
            DeleteObject(info.hbmColor);
        }
        if !info.hbmMask.is_null() {
            DeleteObject(info.hbmMask);
        }
        result
    }

    unsafe fn convert(info: &ICONINFO) -> Option<IconImage> {
        if info.hbmColor.is_null() {
            return None; // icône monochrome : rare, ignorée
        }
        let mut bm = BITMAP::default();
        let got = GetObjectW(
            info.hbmColor,
            size_of::<BITMAP>() as i32,
            &mut bm as *mut BITMAP as *mut c_void,
        );
        let (w, h) = (bm.bmWidth, bm.bmHeight);
        if got == 0 || w <= 0 || h <= 0 || w > 256 || h > 256 {
            return None;
        }
        let len = (w * h * 4) as usize;
        let mut px = vec![0u8; len];
        let mut mask = vec![0u8; len];
        let hdc = GetDC(ptr::null_mut());
        let mut bmi = header(w, h);
        let lines = GetDIBits(
            hdc,
            info.hbmColor,
            0,
            h as u32,
            px.as_mut_ptr() as *mut c_void,
            &mut bmi,
            DIB_RGB_COLORS,
        );
        let mask_ok = !info.hbmMask.is_null() && {
            let mut bmi = header(w, h);
            GetDIBits(
                hdc,
                info.hbmMask,
                0,
                h as u32,
                mask.as_mut_ptr() as *mut c_void,
                &mut bmi,
                DIB_RGB_COLORS,
            ) == h
        };
        ReleaseDC(ptr::null_mut(), hdc);
        if lines != h {
            return None;
        }
        let pixels = px.as_chunks_mut::<4>().0;
        let has_alpha = pixels.iter().any(|p| p[3] != 0);
        let masks = mask.as_chunks::<4>().0;
        for (p, m) in pixels.iter_mut().zip(masks) {
            p.swap(0, 2); // BGRA → RGBA
            if !has_alpha {
                // Icône sans canal alpha : le masque indique la transparence.
                p[3] = if mask_ok && m[0] != 0 { 0 } else { 255 };
            }
        }
        Some(IconImage {
            width: w as u32,
            height: h as u32,
            rgba: px,
        })
    }
}

#[cfg(not(windows))]
mod imp {
    use super::IconImage;

    pub fn init_thread() {}

    pub fn extract(_target: &str) -> Option<IconImage> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_icon_shape() {
        let icon = app_icon();
        assert_eq!(icon.rgba.len(), 32 * 32 * 4);
        // Coin transparent, centre opaque et blanc (triangle).
        assert_eq!(icon.rgba[3], 0);
        let c = ((16 * 32 + 15) * 4) as usize;
        assert_eq!(icon.rgba[c + 3], 255);
        assert!(icon.rgba[c] > 200);
    }

    #[test]
    fn downscale_halves() {
        let icon = app_icon().downscale(16);
        assert_eq!((icon.width, icon.height), (16, 16));
        assert_eq!(icon.rgba.len(), 16 * 16 * 4);
    }
}
