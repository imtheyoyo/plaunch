//! Capture d'écran : recadrage, réduction et encodage PNG.
//!
//! Par défaut, l'image est réduite à l'échelle des « points » egui : un pixel
//! de l'image correspond à un point, l'unité des clics. L'IA peut donc cliquer
//! directement aux coordonnées lues sur la capture.

use egui::{ColorImage, Rect};
use serde_json::Value;

use crate::{base64, Reply};

pub struct ShotOptions {
    /// Pixels physiques par point au moment de la demande.
    ppp: f32,
    /// Zone à capturer, en points.
    region: Option<Rect>,
    /// Résolution d'origine (pas de mise à l'échelle des points).
    native: bool,
    /// Largeur maximale de l'image, en pixels.
    max_width: u32,
}

impl ShotOptions {
    pub fn from_args(args: &Value, ppp: f32) -> Self {
        let num = |k: &str| args.get(k).and_then(Value::as_f64).map(|v| v as f32);
        let region = match (num("x"), num("y"), num("width"), num("height")) {
            (Some(x), Some(y), Some(w), Some(h)) if w > 0.0 && h > 0.0 => {
                Some(Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h)))
            }
            _ => None,
        };
        ShotOptions {
            ppp: if ppp > 0.0 { ppp } else { 1.0 },
            region,
            native: args.get("native").and_then(Value::as_bool).unwrap_or(false),
            max_width: args
                .get("max_width")
                .and_then(Value::as_u64)
                .map(|w| w.clamp(64, 4096) as u32)
                .unwrap_or(1600),
        }
    }
}

pub fn encode(img: &ColorImage, opts: &ShotOptions) -> Reply {
    let [w, h] = img.size;
    if w == 0 || h == 0 {
        return Reply::Error("capture vide (fenêtre masquée ou réduite ?)".into());
    }
    // Recadrage (en pixels physiques).
    let (x0, y0, cw, ch) = match opts.region {
        Some(r) => {
            let x0 = ((r.min.x * opts.ppp).max(0.0) as usize).min(w - 1);
            let y0 = ((r.min.y * opts.ppp).max(0.0) as usize).min(h - 1);
            let x1 = ((r.max.x * opts.ppp).ceil() as usize).clamp(x0 + 1, w);
            let y1 = ((r.max.y * opts.ppp).ceil() as usize).clamp(y0 + 1, h);
            (x0, y0, x1 - x0, y1 - y0)
        }
        None => (0, 0, w, h),
    };
    // Taille finale : points (ou pixels d'origine), plafonnée.
    let scale = if opts.native { 1.0 } else { 1.0 / opts.ppp };
    let mut tw = ((cw as f32 * scale).round() as usize).max(1);
    let mut th = ((ch as f32 * scale).round() as usize).max(1);
    if tw > opts.max_width as usize {
        th = (th * opts.max_width as usize / tw).max(1);
        tw = opts.max_width as usize;
    }
    let rgba = resample(img, x0, y0, cw, ch, tw, th);
    let mut png_data = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png_data, tw as u32, th as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let res = enc
            .write_header()
            .and_then(|mut wr| wr.write_image_data(&rgba));
        if let Err(e) = res {
            return Reply::Error(format!("encodage PNG : {e}"));
        }
    }
    // Facteur image → points, pour convertir les coordonnées lues.
    let points_per_px = cw as f32 / opts.ppp / tw as f32;
    let (ox, oy) = (x0 as f32 / opts.ppp, y0 as f32 / opts.ppp);
    let text = if (points_per_px - 1.0).abs() < 0.01 && ox == 0.0 && oy == 0.0 {
        format!("Capture {tw}×{th} px ; 1 px = 1 point (coordonnées utilisables telles quelles pour cliquer).")
    } else {
        format!(
            "Capture {tw}×{th} px ; point = ({ox:.0} + px_x × {points_per_px:.3}, {oy:.0} + px_y × {points_per_px:.3})."
        )
    };
    Reply::Image {
        png_base64: base64::encode(&png_data),
        text,
    }
}

/// Réduction par moyenne de zone (ou agrandissement au plus proche).
fn resample(
    img: &ColorImage,
    x0: usize,
    y0: usize,
    cw: usize,
    ch: usize,
    tw: usize,
    th: usize,
) -> Vec<u8> {
    let w = img.size[0];
    let mut out = Vec::with_capacity(tw * th * 4);
    for ty in 0..th {
        let sy0 = y0 + ty * ch / th;
        let sy1 = (y0 + (ty + 1) * ch / th).max(sy0 + 1);
        for tx in 0..tw {
            let sx0 = x0 + tx * cw / tw;
            let sx1 = (x0 + (tx + 1) * cw / tw).max(sx0 + 1);
            let mut acc = [0u32; 4];
            let mut n = 0u32;
            for sy in sy0..sy1 {
                for sx in sx0..sx1 {
                    let c = img.pixels[sy * w + sx].to_srgba_unmultiplied();
                    for i in 0..4 {
                        acc[i] += u32::from(c[i]);
                    }
                    n += 1;
                }
            }
            out.extend(acc.iter().map(|v| (v / n.max(1)) as u8));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_to_points() {
        let img = ColorImage::new([200, 100], vec![egui::Color32::RED; 200 * 100]);
        let opts = ShotOptions::from_args(&serde_json::json!({}), 2.0);
        match encode(&img, &opts) {
            Reply::Image { text, png_base64 } => {
                assert!(text.contains("100×50"), "{text}");
                assert!(!png_base64.is_empty());
            }
            r => panic!("{r:?}"),
        }
        let opts = ShotOptions::from_args(
            &serde_json::json!({"x":10,"y":10,"width":20,"height":20}),
            2.0,
        );
        match encode(&img, &opts) {
            Reply::Image { text, .. } => assert!(text.contains("20×20"), "{text}"),
            r => panic!("{r:?}"),
        }
    }
}
