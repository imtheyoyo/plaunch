//! Thème « Moderne » : style personnalisé d'egui inspiré de Windows 11
//! (Fluent) — coins arrondis, espacements aérés, boutons sans bordure,
//! accent bleu, ombres douces et barres de défilement fines. Deux palettes
//! (claire et sombre) : le choix suit le mode de Windows.

use super::*;

use egui::{Color32, CornerRadius, Margin, Shadow, Stroke, Style, Visuals};

/// Couleurs sémantiques exposées aux vues personnalisées (cartes, badges, barres).
#[derive(Clone, Copy)]
pub(super) struct ThemeTokens {
    pub card: Color32,
    pub field: Color32,
    pub border: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub star: Color32,
    pub text_strong: Color32,
    pub text_muted: Color32,
}

/// Renvoie les couleurs adaptées au thème actif (clair ou sombre).
pub(super) fn tokens(visuals: &Visuals) -> ThemeTokens {
    let p = if visuals.dark_mode { &DARK } else { &LIGHT };
    ThemeTokens {
        card: p.card,
        field: p.field,
        border: p.border,
        accent: p.accent,
        accent_soft: p.accent_soft,
        success: p.success,
        warning: p.warning,
        star: p.star,
        text_strong: p.text_strong,
        text_muted: visuals.weak_text_color(),
    }
}

/// Couleurs d'une palette.
struct Palette {
    dark: bool,
    /// Fond de la fenêtre principale (panneaux).
    background: Color32,
    /// Fenêtres, menus et bulles.
    surface: Color32,
    /// Cartes et bandeaux surélevés.
    card: Color32,
    /// Champs de saisie et rails.
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
    /// Teinte douce pour les éléments sélectionnés.
    accent_soft: Color32,
    /// Fond d'un élément sélectionné.
    selection: Color32,
    /// Texte d'un élément sélectionné.
    selection_text: Color32,
    link: Color32,
    success: Color32,
    warning: Color32,
    star: Color32,
    shadow: u8,
}

const DARK: Palette = Palette {
    dark: true,
    background: Color32::from_rgb(0x18, 0x19, 0x1E),
    surface: Color32::from_rgb(0x21, 0x23, 0x2A),
    card: Color32::from_rgb(0x24, 0x26, 0x2E),
    field: Color32::from_rgb(0x13, 0x14, 0x18),
    widget: Color32::from_rgb(0x2C, 0x2F, 0x38),
    widget_hover: Color32::from_rgb(0x38, 0x3C, 0x47),
    widget_press: Color32::from_rgb(0x43, 0x48, 0x55),
    line: Color32::from_rgb(0x2E, 0x31, 0x3A),
    border: Color32::from_rgb(0x38, 0x3B, 0x46),
    text: Color32::from_rgb(0xDC, 0xDF, 0xE6),
    text_strong: Color32::from_rgb(0xF6, 0xF8, 0xFB),
    accent: Color32::from_rgb(0x4F, 0x8F, 0xF7),
    accent_soft: Color32::from_rgba_premultiplied(0x20, 0x38, 0x62, 0xB8),
    selection: Color32::from_rgb(0x25, 0x5D, 0xC4),
    selection_text: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    link: Color32::from_rgb(0x6A, 0xA8, 0xFF),
    success: Color32::from_rgb(0x34, 0xD3, 0x99),
    warning: Color32::from_rgb(0xFB, 0xBF, 0x24),
    star: Color32::from_rgb(0xF5, 0xB8, 0x2E),
    shadow: 115,
};

const LIGHT: Palette = Palette {
    dark: false,
    background: Color32::from_rgb(0xF3, 0xF5, 0xF8),
    surface: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    card: Color32::from_rgb(0xFC, 0xFD, 0xFF),
    field: Color32::from_rgb(0xE9, 0xEC, 0xF2),
    widget: Color32::from_rgb(0xEA, 0xED, 0xF3),
    widget_hover: Color32::from_rgb(0xDF, 0xE3, 0xEC),
    widget_press: Color32::from_rgb(0xD2, 0xD8, 0xE3),
    line: Color32::from_rgb(0xE0, 0xE4, 0xEC),
    border: Color32::from_rgb(0xD5, 0xDA, 0xE4),
    text: Color32::from_rgb(0x24, 0x28, 0x30),
    text_strong: Color32::from_rgb(0x0B, 0x0E, 0x14),
    accent: Color32::from_rgb(0x25, 0x63, 0xEB),
    accent_soft: Color32::from_rgb(0xE0, 0xEC, 0xFE),
    selection: Color32::from_rgb(0xD3, 0xE3, 0xFD),
    selection_text: Color32::from_rgb(0x0B, 0x3B, 0x9E),
    link: Color32::from_rgb(0x1D, 0x5B, 0xD8),
    success: Color32::from_rgb(0x16, 0xA3, 0x4A),
    warning: Color32::from_rgb(0xD9, 0x77, 0x06),
    star: Color32::from_rgb(0xE5, 0x98, 0x08),
    shadow: 30,
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
