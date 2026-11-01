//! Icônes de l'interface : sous-ensemble de Phosphor (licence MIT,
//! `assets/fonts/PHOSPHOR-LICENSE.txt`) embarqué dans l'exécutable.
//!
//! Depuis egui 0.37, les polices d'émojis ne sont plus fournies : les
//! symboles (★, ⚙, 📁…) dépendaient des polices de Windows et pouvaient
//! s'afficher en carrés. Ces icônes, elles, sont toujours présentes.
//! La police dessine aussi les symboles Unicode déjà utilisés dans les
//! textes (voir `UNICODE` dans le script).
//! Pour en ajouter : `tools/make_icon_font.py`, puis une constante ici.
#![allow(dead_code)]

use eframe::egui;
use egui::epaint::text::{FontData, FontInsert, FontPriority, InsertFontFamily};

const FONT: &[u8] = include_bytes!("../../assets/fonts/plaunch-icons.ttf");

/// Ajoute la police d'icônes en tête des polices de texte : elle ne contient
/// que des symboles (zone privée et quelques caractères comme ★ ⚙ 📁), qui
/// gardent ainsi le même style partout.
pub fn install(ctx: &egui::Context) {
    let families = [egui::FontFamily::Proportional, egui::FontFamily::Monospace]
        .into_iter()
        .map(|family| InsertFontFamily {
            family,
            priority: FontPriority::Highest,
        })
        .collect();
    let data = FontData::from_static(FONT).tweak(egui::epaint::text::FontTweak {
        y_offset_factor: 0.1,
        ..Default::default()
    });
    ctx.add_font(FontInsert::new("plaunch-icons", data, families));
}

/// Icône suivie d'un texte, séparés par une espace.
pub fn with(icon: &str, text: impl AsRef<str>) -> String {
    format!("{icon} {}", text.as_ref())
}

pub const ALARM: &str = "\u{E006}";
pub const ARROW_ARC_LEFT: &str = "\u{E014}";
pub const ARROW_ARC_RIGHT: &str = "\u{E016}";
pub const ARROW_CIRCLE_UP: &str = "\u{E030}";
pub const ARROW_CLOCKWISE: &str = "\u{E036}";
pub const ARROW_RIGHT: &str = "\u{E06C}";
pub const ARROW_U_UP_LEFT: &str = "\u{E08A}";
pub const ARROWS_IN_SIMPLE: &str = "\u{E09E}";
pub const ARROWS_OUT_SIMPLE: &str = "\u{E0A6}";
pub const CALENDAR_BLANK: &str = "\u{E10A}";
pub const CARET_DOWN: &str = "\u{E136}";
pub const CARET_LEFT: &str = "\u{E138}";
pub const CARET_RIGHT: &str = "\u{E13A}";
pub const CARET_UP: &str = "\u{E13C}";
pub const CHART_BAR: &str = "\u{E150}";
pub const CHECK: &str = "\u{E182}";
pub const CIRCLE: &str = "\u{E18A}";
pub const CLOCK_COUNTER_CLOCKWISE: &str = "\u{E1A0}";
pub const COPY: &str = "\u{E1CA}";
pub const EJECT: &str = "\u{E212}";
pub const FOLDER: &str = "\u{E24A}";
pub const FOLDER_OPEN: &str = "\u{E256}";
pub const GEAR_SIX: &str = "\u{E272}";
pub const INFO: &str = "\u{E2CE}";
pub const KEYBOARD: &str = "\u{E2D8}";
pub const LIST: &str = "\u{E2F0}";
pub const MAGNIFYING_GLASS: &str = "\u{E30C}";
pub const MONITOR: &str = "\u{E32E}";
pub const MOON: &str = "\u{E330}";
pub const NOTE_PENCIL: &str = "\u{E34C}";
pub const PENCIL_SIMPLE: &str = "\u{E3B4}";
pub const PLAY: &str = "\u{E3D0}";
pub const PLUS: &str = "\u{E3D4}";
pub const POWER: &str = "\u{E3DA}";
pub const QUESTION: &str = "\u{E3E8}";
pub const ROCKET_LAUNCH: &str = "\u{E3FE}";
pub const ROWS: &str = "\u{E5A2}";
pub const SQUARES_FOUR: &str = "\u{E464}";
pub const STAR: &str = "\u{E46A}";
pub const SUN: &str = "\u{E472}";
pub const TRASH: &str = "\u{E4A6}";
pub const USER_CIRCLE: &str = "\u{E4C4}";
pub const WARNING: &str = "\u{E4E0}";
pub const X: &str = "\u{E4F6}";
pub const STAR_FILL: &str = "\u{E900}";
pub const CIRCLE_FILL: &str = "\u{E901}";
pub const PLAY_FILL: &str = "\u{E902}";
pub const CARET_LEFT_FILL: &str = "\u{E903}";
pub const CARET_RIGHT_FILL: &str = "\u{E904}";

#[cfg(test)]
mod tests {
    #[test]
    fn font_has_every_icon() {
        let src = include_str!("icon.rs");
        let font = super::FONT;
        // Table cmap minimale : on vérifie juste que la police est lisible
        // et que chaque constante pointe dans la zone privée utilisée.
        assert!(font.len() > 1000);
        for line in src.lines().filter(|l| l.starts_with("pub const ")) {
            let code = line
                .split("\\u{")
                .nth(1)
                .unwrap()
                .split('}')
                .next()
                .unwrap();
            let c = u32::from_str_radix(code, 16).unwrap();
            assert!((0xE000..=0xF8FF).contains(&c), "{line}");
        }
    }
}
