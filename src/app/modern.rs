//! Thème « Moderne » : style personnalisé d'egui inspiré de Windows 11
//! (Fluent) — coins arrondis, espacements aérés, boutons sans bordure,
//! accent bleu, ombres douces et barres de défilement fines. Deux palettes
//! (claire et sombre) : le choix suit le mode de Windows.

use super::*;

use egui::{Color32, CornerRadius, Margin, Shadow, Stroke, Style, Visuals};

/// Couleurs d'une palette.
struct Palette {
    dark: bool,
    /// Fond de la fenêtre principale (panneaux).
    background: Color32,
    /// Fenêtres, menus et bulles.
    surface: Color32,
    /// Champs de saisie.
    field: Color32,
    /// Fond des widgets au repos.
    widget: Color32,
    /// Fond des widgets survolés.
    widget_hover: Color32,
    /// Fond des widgets enfoncés.
    widget_press: Color32,
    /// Séparateurs et contours discrets.
    line: Color32,
    /// Contour des fenêtres et menus.
    border: Color32,
    text: Color32,
    text_strong: Color32,
    accent: Color32,
    /// Fond d'un élément sélectionné.
    selection: Color32,
    /// Texte d'un élément sélectionné.
    selection_text: Color32,
    link: Color32,
    shadow: u8,
}

const DARK: Palette = Palette {
    dark: true,
    background: Color32::from_rgb(0x1E, 0x1F, 0x24),
    surface: Color32::from_rgb(0x25, 0x26, 0x2C),
    field: Color32::from_rgb(0x17, 0x18, 0x1C),
    widget: Color32::from_rgb(0x2E, 0x30, 0x38),
    widget_hover: Color32::from_rgb(0x3A, 0x3D, 0x47),
    widget_press: Color32::from_rgb(0x44, 0x47, 0x52),
    line: Color32::from_rgb(0x33, 0x35, 0x3D),
    border: Color32::from_rgb(0x3A, 0x3C, 0x45),
    text: Color32::from_rgb(0xD9, 0xDB, 0xE1),
    text_strong: Color32::from_rgb(0xF5, 0xF6, 0xF8),
    accent: Color32::from_rgb(0x3B, 0x82, 0xF6),
    selection: Color32::from_rgb(0x24, 0x5A, 0xC0),
    selection_text: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    link: Color32::from_rgb(0x6A, 0xA8, 0xFF),
    shadow: 110,
};

const LIGHT: Palette = Palette {
    dark: false,
    background: Color32::from_rgb(0xF6, 0xF7, 0xF9),
    surface: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    field: Color32::from_rgb(0xEB, 0xED, 0xF1),
    widget: Color32::from_rgb(0xEC, 0xEE, 0xF2),
    widget_hover: Color32::from_rgb(0xE1, 0xE4, 0xEA),
    widget_press: Color32::from_rgb(0xD5, 0xD9, 0xE1),
    line: Color32::from_rgb(0xE3, 0xE6, 0xEB),
    border: Color32::from_rgb(0xD9, 0xDD, 0xE4),
    text: Color32::from_rgb(0x24, 0x27, 0x2E),
    text_strong: Color32::from_rgb(0x0B, 0x0D, 0x12),
    accent: Color32::from_rgb(0x25, 0x63, 0xEB),
    selection: Color32::from_rgb(0xD3, 0xE3, 0xFD),
    selection_text: Color32::from_rgb(0x0B, 0x3B, 0x9E),
    link: Color32::from_rgb(0x1D, 0x5B, 0xD8),
    shadow: 34,
};

/// Applique le style « Moderne » aux deux modes (clair et sombre).
pub(super) fn apply(ctx: &egui::Context) {
    ctx.set_style_of(egui::Theme::Dark, style(&DARK));
    ctx.set_style_of(egui::Theme::Light, style(&LIGHT));
}

/// Remet les styles par défaut d'egui.
pub(super) fn reset(ctx: &egui::Context) {
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        ctx.set_style_of(theme, theme.default_style());
    }
}

fn style(p: &Palette) -> Style {
    let base = if p.dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    let mut style = base.default_style();
    style.visuals = visuals(p);

    let s = &mut style.spacing;
    s.item_spacing = egui::vec2(8.0, 6.0);
    s.button_padding = egui::vec2(10.0, 4.0);
    s.interact_size.y = 24.0;
    s.window_margin = Margin::same(12);
    s.menu_margin = Margin::same(6);
    s.menu_spacing = 4.0;
    s.icon_width = 16.0;
    s.icon_width_inner = 9.0;
    s.icon_spacing = 6.0;
    s.indent = 20.0;
    s.slider_rail_height = 4.0;
    s.scroll = egui::style::ScrollStyle::thin();
    s.scroll.bar_width = 8.0;
    style
}

fn visuals(p: &Palette) -> Visuals {
    let mut v = if p.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    let small = CornerRadius::same(6);
    let large = CornerRadius::same(10);
    let none = Stroke::NONE;

    v.panel_fill = p.background;
    v.window_fill = p.surface;
    v.faint_bg_color = p.surface;
    v.extreme_bg_color = p.field;
    v.text_edit_bg_color = Some(p.field);
    v.code_bg_color = p.widget;
    v.hyperlink_color = p.link;
    v.window_corner_radius = large;
    v.menu_corner_radius = large;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_shadow = Shadow {
        offset: [0, 8],
        blur: 28,
        spread: 0,
        color: Color32::from_black_alpha(p.shadow),
    };
    v.popup_shadow = Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: Color32::from_black_alpha(p.shadow),
    };
    v.weak_text_alpha = 0.65;
    v.selection.bg_fill = p.selection;
    v.selection.stroke = Stroke::new(1.0, p.selection_text);
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;
    v.indent_has_left_vline = false;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.surface;
    w.noninteractive.weak_bg_fill = p.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = small;

    w.inactive.bg_fill = p.widget;
    w.inactive.weak_bg_fill = p.widget;
    w.inactive.bg_stroke = none;
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = small;
    w.inactive.expansion = 0.0;

    w.hovered.bg_fill = p.widget_hover;
    w.hovered.weak_bg_fill = p.widget_hover;
    w.hovered.bg_stroke = none;
    w.hovered.fg_stroke = Stroke::new(1.5, p.text_strong);
    w.hovered.corner_radius = small;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = p.widget_press;
    w.active.weak_bg_fill = p.widget_press;
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(2.0, p.text_strong);
    w.active.corner_radius = small;
    w.active.expansion = 0.0;

    w.open.bg_fill = p.widget_hover;
    w.open.weak_bg_fill = p.widget_hover;
    w.open.bg_stroke = Stroke::new(1.0, p.accent);
    w.open.fg_stroke = Stroke::new(1.0, p.text_strong);
    w.open.corner_radius = small;
    w.open.expansion = 0.0;
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palettes_match_their_mode() {
        assert!(style(&DARK).visuals.dark_mode);
        assert!(!style(&LIGHT).visuals.dark_mode);
        assert_eq!(style(&DARK).visuals.panel_fill, DARK.background);
    }

    #[test]
    fn reset_restores_egui_defaults() {
        // `Style` compare ses formateurs par pointeur : on compare les
        // parties qui comptent.
        let same = |a: &Style, b: &Style| a.visuals == b.visuals && a.spacing == b.spacing;
        let ctx = egui::Context::default();
        apply(&ctx);
        let dark = egui::Theme::Dark.default_style();
        assert!(!same(&ctx.style_of(egui::Theme::Dark), &dark));
        reset(&ctx);
        assert!(same(&ctx.style_of(egui::Theme::Dark), &dark));
        let light = egui::Theme::Light.default_style();
        assert!(same(&ctx.style_of(egui::Theme::Light), &light));
    }
}
