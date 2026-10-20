//! Icônes : extraction des icônes de programmes (Windows) et icône de
//! l'application dessinée par le code.

use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::i18n::tr;
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

/// Forme dessinée sur l'icône de l'application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconShape {
    #[default]
    Play,
    Star,
    Grid,
    Bolt,
}

impl IconShape {
    pub const ALL: [IconShape; 4] = [
        IconShape::Play,
        IconShape::Star,
        IconShape::Grid,
        IconShape::Bolt,
    ];

    pub fn key(self) -> &'static str {
        match self {
            IconShape::Play => "play",
            IconShape::Star => "star",
            IconShape::Grid => "grid",
            IconShape::Bolt => "bolt",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            IconShape::Play => tr("Lecture"),
            IconShape::Star => tr("Étoile"),
            IconShape::Grid => tr("Grille"),
            IconShape::Bolt => tr("Éclair"),
        }
    }

    pub fn from_key(s: &str) -> IconShape {
        IconShape::ALL
            .into_iter()
            .find(|c| c.key().eq_ignore_ascii_case(s.trim()))
            .unwrap_or_default()
    }

    /// Le point (coordonnées sur 32 px) appartient-il au motif ?
    fn contains(self, x: f32, y: f32) -> bool {
        match self {
            IconShape::Play => in_triangle(x, y, (12.0, 8.5), (12.0, 23.5), (24.0, 16.0)),
            IconShape::Star => in_polygon(x, y, &star_points()),
            IconShape::Grid => {
                let cell =
                    |x0: f32, y0: f32| in_rounded_rect(x, y, x0, y0, x0 + 7.0, y0 + 7.0, 1.5);
                cell(8.0, 8.0) || cell(17.0, 8.0) || cell(8.0, 17.0) || cell(17.0, 17.0)
            }
            IconShape::Bolt => in_polygon(
                x,
                y,
                &[
                    (18.5, 5.0),
                    (9.0, 18.0),
                    (15.0, 18.0),
                    (13.0, 27.0),
                    (23.0, 13.5),
                    (17.0, 13.5),
                ],
            ),
        }
    }
}

/// Couleur de fond de l'icône (dégradé vertical).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconColor {
    #[default]
    Blue,
    Green,
    Orange,
    Purple,
    Red,
    Gray,
}

impl IconColor {
    pub const ALL: [IconColor; 6] = [
        IconColor::Blue,
        IconColor::Green,
        IconColor::Orange,
        IconColor::Purple,
        IconColor::Red,
        IconColor::Gray,
    ];

    pub fn key(self) -> &'static str {
        match self {
            IconColor::Blue => "blue",
            IconColor::Green => "green",
            IconColor::Orange => "orange",
            IconColor::Purple => "purple",
            IconColor::Red => "red",
            IconColor::Gray => "gray",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            IconColor::Blue => tr("Bleu"),
            IconColor::Green => tr("Vert"),
            IconColor::Orange => tr("Orange"),
            IconColor::Purple => tr("Violet"),
            IconColor::Red => tr("Rouge"),
            IconColor::Gray => tr("Gris"),
        }
    }

    pub fn from_key(s: &str) -> IconColor {
        IconColor::ALL
            .into_iter()
            .find(|c| c.key().eq_ignore_ascii_case(s.trim()))
            .unwrap_or_default()
    }

    /// Couleurs haut et bas du dégradé.
    pub fn gradient(self) -> ([u8; 3], [u8; 3]) {
        match self {
            IconColor::Blue => ([59, 130, 246], [29, 78, 216]),
            IconColor::Green => ([34, 197, 94], [21, 128, 61]),
            IconColor::Orange => ([249, 115, 22], [194, 65, 12]),
            IconColor::Purple => ([168, 85, 247], [109, 40, 217]),
            IconColor::Red => ([239, 68, 68], [185, 28, 28]),
            IconColor::Gray => ([107, 114, 128], [55, 65, 81]),
        }
    }
}

/// Icône de l'application : carré arrondi en dégradé et motif blanc.
pub fn app_icon(shape: IconShape, color: IconColor) -> IconImage {
    render_icon(shape, color, 32)
}

/// Même dessin à une taille quelconque (fichier .ico de l'exécutable).
pub fn render_icon(shape: IconShape, color: IconColor, size: u32) -> IconImage {
    #[allow(non_snake_case)]
    let S = size;
    // Le dessin est défini sur une grille de 32 px.
    let k = 32.0 / S as f32;
    let mut rgba = vec![0u8; (S * S * 4) as usize];
    let radius = 7.0f32;
    let (top, bottom) = color.gradient();
    for y in 0..S {
        for x in 0..S {
            // Suréchantillonnage 4×4 pour l'anticrénelage.
            let mut cover_box = 0.0f32;
            let mut cover_fg = 0.0f32;
            for sy in 0..4 {
                for sx in 0..4 {
                    let px = (x as f32 + (sx as f32 + 0.5) / 4.0) * k;
                    let py = (y as f32 + (sy as f32 + 0.5) / 4.0) * k;
                    if in_rounded_rect(px, py, 1.0, 1.0, 31.0, 31.0, radius) {
                        cover_box += 1.0 / 16.0;
                        if shape.contains(px, py) {
                            cover_fg += 1.0 / 16.0;
                        }
                    }
                }
            }
            let t = y as f32 / S as f32;
            let mix = if cover_box > 0.0 {
                cover_fg / cover_box
            } else {
                0.0
            };
            let i = ((y * S + x) * 4) as usize;
            for c in 0..3 {
                let bg = lerp(top[c] as f32, bottom[c] as f32, t);
                rgba[i + c] = (bg * (1.0 - mix) + 255.0 * mix).round() as u8;
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

/// Icône choisie dans les réglages : fichier personnalisé (.ico, .exe…)
/// s'il est lisible, sinon l'icône dessinée.
pub fn custom_icon(shape: IconShape, color: IconColor, file: Option<&str>) -> IconImage {
    if let Some(f) = file.filter(|f| !f.trim().is_empty()) {
        imp::init_thread();
        if let Some(img) = imp::extract_file(f) {
            return img;
        }
    }
    app_icon(shape, color)
}

fn star_points() -> [(f32, f32); 10] {
    let mut pts = [(0.0, 0.0); 10];
    for (i, p) in pts.iter_mut().enumerate() {
        let r = if i % 2 == 0 { 10.5 } else { 4.6 };
        let a = std::f32::consts::PI * (i as f32 / 5.0) - std::f32::consts::FRAC_PI_2;
        *p = (16.0 + r * a.cos(), 16.8 + r * a.sin());
    }
    pts
}

/// Test pair-impair de l'appartenance à un polygone.
fn in_polygon(x: f32, y: f32, pts: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let mut j = pts.len() - 1;
    for i in 0..pts.len() {
        let (xi, yi) = pts[i];
        let (xj, yj) = pts[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
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
        // Lien web : icône du navigateur par défaut (type .html).
        if crate::paths::is_url(target) {
            return query("page.html", true);
        }
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

    /// Icône d'un fichier local, sans icône générique de repli.
    pub fn extract_file(target: &str) -> Option<IconImage> {
        query(target, false)
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
        BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
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
            },
            ..Default::default()
        }
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

    pub fn extract_file(_target: &str) -> Option<IconImage> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_icon_shape() {
        let icon = app_icon(IconShape::Play, IconColor::Blue);
        assert_eq!(icon.rgba.len(), 32 * 32 * 4);
        // Coin transparent, centre opaque et blanc (triangle).
        assert_eq!(icon.rgba[3], 0);
        let c = ((16 * 32 + 15) * 4) as usize;
        assert_eq!(icon.rgba[c + 3], 255);
        assert!(icon.rgba[c] > 200);
    }

    #[test]
    fn every_shape_draws_something() {
        for shape in IconShape::ALL {
            assert_eq!(IconShape::from_key(shape.key()), shape);
            let icon = app_icon(shape, IconColor::Green);
            let white = icon
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[0] > 240 && p[1] > 240 && p[2] > 240 && p[3] == 255)
                .count();
            assert!(white > 60, "{shape:?} : {white} pixels blancs");
            // Le fond garde la couleur choisie.
            assert!(icon.rgba[(16 * 32 + 3) * 4 + 1] > icon.rgba[(16 * 32 + 3) * 4]);
        }
        for color in IconColor::ALL {
            assert_eq!(IconColor::from_key(color.key()), color);
        }
        assert_eq!(IconShape::from_key("?"), IconShape::Play);
    }

    /// Régénère `assets/plaunch.256.rgba` (puis le .ico avec
    /// `python3 assets/make_ico.py`) : `cargo test -- --ignored ico_source`.
    #[test]
    #[ignore]
    fn ico_source() {
        let img = render_icon(IconShape::Play, IconColor::Blue, 256);
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/plaunch.256.rgba");
        std::fs::write(path, &img.rgba).unwrap();
    }

    #[test]
    fn render_scales() {
        let big = render_icon(IconShape::Play, IconColor::Blue, 64);
        assert_eq!(big.rgba.len(), 64 * 64 * 4);
        assert_eq!(big.rgba[3], 0);
        let c = ((32 * 64 + 30) * 4) as usize;
        assert_eq!(big.rgba[c + 3], 255);
        assert!(big.rgba[c] > 200);
    }

    #[test]
    fn downscale_halves() {
        let icon = app_icon(IconShape::Play, IconColor::Blue).downscale(16);
        assert_eq!((icon.width, icon.height), (16, 16));
        assert_eq!(icon.rgba.len(), 16 * 16 * 4);
    }
}
