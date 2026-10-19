//! Petits composants : saisie d'un raccourci clavier par appui sur les
//! touches, et choix d'une date et d'une heure d'alarme avec un calendrier.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use eframe::egui;

use crate::i18n::{is_english, tr};

// ---- Raccourci clavier ---------------------------------------------------

/// Combinaison lisible par `global-hotkey` (« Ctrl+Alt+V »), ou `None` si
/// la touche seule ne fait pas un raccourci global raisonnable.
pub fn combo_text(key: egui::Key, m: egui::Modifiers) -> Option<String> {
    let name = match key {
        egui::Key::ArrowUp => "Up",
        egui::Key::ArrowDown => "Down",
        egui::Key::ArrowLeft => "Left",
        egui::Key::ArrowRight => "Right",
        k => k.name(),
    };
    let function_key =
        name.len() >= 2 && name.starts_with('F') && name[1..].chars().all(|c| c.is_ascii_digit());
    // Une lettre seule serait captée dans toutes les applications.
    if !(m.ctrl || m.alt || m.mac_cmd || function_key) {
        return None;
    }
    let mut parts = Vec::new();
    if m.ctrl || m.mac_cmd {
        parts.push("Ctrl");
    }
    if m.alt {
        parts.push("Alt");
    }
    if m.shift {
        parts.push("Shift");
    }
    parts.push(name);
    Some(parts.join("+"))
}

fn is_modifier(key: egui::Key) -> bool {
    matches!(
        key,
        egui::Key::ShiftLeft
            | egui::Key::ShiftRight
            | egui::Key::ControlLeft
            | egui::Key::ControlRight
            | egui::Key::AltLeft
            | egui::Key::AltRight
            | egui::Key::SuperLeft
            | egui::Key::SuperRight
    )
}

/// Champ de raccourci : un clic, puis la combinaison au clavier. Échap
/// annule, Retour arrière ou Suppr efface. Renvoie `true` si la valeur a changé.
pub fn hotkey_capture(ui: &mut egui::Ui, value: &mut String, salt: &str) -> bool {
    let id = ui.make_persistent_id(("hotkey-capture", salt));
    let mut capturing = ui.data(|d| d.get_temp::<bool>(id).unwrap_or(false));
    let mut changed = false;
    ui.horizontal(|ui| {
        let label = if capturing {
            egui::RichText::new(tr("Appuyez sur la combinaison…")).strong()
        } else if value.trim().is_empty() {
            egui::RichText::new(tr("(aucun) : cliquer pour définir")).weak()
        } else {
            egui::RichText::new(value.as_str()).monospace()
        };
        let resp = ui
            .add(
                egui::Button::new(label)
                    .selected(capturing)
                    .min_size(egui::vec2(170.0, 0.0)),
            )
            .on_hover_text(tr(
                "Cliquez puis appuyez sur les touches (Échap pour annuler)",
            ));
        if resp.clicked() {
            capturing = !capturing;
        }
        if !value.trim().is_empty() && ui.small_button("✖").on_hover_text(tr("Effacer")).clicked()
        {
            value.clear();
            capturing = false;
            changed = true;
        }
        if capturing {
            let pressed = ui.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } if !is_modifier(*key) => Some((*key, *modifiers)),
                    // Ctrl+C, Ctrl+V et Ctrl+X arrivent comme copier/coller.
                    egui::Event::Copy => Some((egui::Key::C, i.modifiers)),
                    egui::Event::Cut => Some((egui::Key::X, i.modifiers)),
                    egui::Event::Paste(_) => Some((egui::Key::V, i.modifiers)),
                    _ => None,
                })
            });
            if let Some((key, m)) = pressed {
                match key {
                    egui::Key::Escape => capturing = false,
                    egui::Key::Backspace | egui::Key::Delete if m.is_none() => {
                        value.clear();
                        capturing = false;
                        changed = true;
                    }
                    _ => {
                        if let Some(text) = combo_text(key, m) {
                            *value = text;
                            capturing = false;
                            changed = true;
                        }
                    }
                }
                // La touche ne doit pas agir ailleurs (Entrée, Suppr…).
                ui.input_mut(|i| i.events.clear());
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(id, capturing));
    changed
}

// ---- Date et heure d'alarme ----------------------------------------------

const MONTHS_FR: [&str; 12] = [
    "janvier",
    "février",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "août",
    "septembre",
    "octobre",
    "novembre",
    "décembre",
];
const MONTHS_EN: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const DAYS_FR: [&str; 7] = ["lun.", "mar.", "mer.", "jeu.", "ven.", "sam.", "dim."];
const DAYS_EN: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

fn month_name(m: u32) -> &'static str {
    let i = (m.clamp(1, 12) - 1) as usize;
    if is_english() {
        MONTHS_EN[i]
    } else {
        MONTHS_FR[i]
    }
}

fn day_name(d: chrono::Weekday) -> &'static str {
    let i = d.num_days_from_monday() as usize;
    if is_english() {
        DAYS_EN[i]
    } else {
        DAYS_FR[i]
    }
}

/// « lun. 20 octobre 2026 à 09:00 ».
pub fn format_alarm(t: NaiveDateTime) -> String {
    if is_english() {
        format!(
            "{} {} {} {} at {:02}:{:02}",
            day_name(t.weekday()),
            t.day(),
            month_name(t.month()),
            t.year(),
            t.hour(),
            t.minute()
        )
    } else {
        format!(
            "{} {} {} {} à {:02}:{:02}",
            day_name(t.weekday()),
            t.day(),
            month_name(t.month()),
            t.year(),
            t.hour(),
            t.minute()
        )
    }
}

/// Prochaine heure pleine (proposée par défaut).
pub fn next_full_hour(now: NaiveDateTime) -> NaiveDateTime {
    let base = now.date().and_hms_opt(now.hour(), 0, 0).unwrap_or(now);
    base + Duration::hours(1)
}

/// Premier jour du mois suivant (`delta` = ±1).
fn shift_month(first: NaiveDate, delta: i32) -> NaiveDate {
    let mut y = first.year();
    let mut m = first.month() as i32 + delta;
    if m < 1 {
        m = 12;
        y -= 1;
    } else if m > 12 {
        m = 1;
        y += 1;
    }
    NaiveDate::from_ymd_opt(y, m as u32, 1).unwrap_or(first)
}

/// Éditeur d'alarme : bouton date (calendrier dépliable), heure, minutes.
/// Renvoie `true` si l'alarme a changé.
pub fn alarm_editor(ui: &mut egui::Ui, alarm: &mut Option<NaiveDateTime>, salt: usize) -> bool {
    let id = ui.make_persistent_id(("alarm-calendar", salt));
    let now = chrono::Local::now().naive_local();
    let Some(current) = *alarm else {
        if ui
            .button(tr("⏰ Ajouter une alarme"))
            .on_hover_text(tr("Rappel à l'heure choisie, même fenêtre masquée"))
            .clicked()
        {
            *alarm = Some(next_full_hour(now));
            ui.data_mut(|d| d.insert_temp(id, Some(now.date().with_day(1).unwrap_or(now.date()))));
            return true;
        }
        return false;
    };
    let mut changed = false;
    let mut shown: Option<NaiveDate> = ui.data(|d| d.get_temp(id)).flatten();
    let mut value = current;
    ui.horizontal_wrapped(|ui| {
        ui.label("⏰");
        let date_text = format_alarm(value);
        let date_label = date_text
            .rsplit_once([' '])
            .map(|(d, _)| d)
            .unwrap_or(&date_text);
        let date_label = date_label
            .trim_end_matches(" à")
            .trim_end_matches(" at")
            .to_string();
        if ui
            .button(format!("📅 {date_label}"))
            .on_hover_text(tr("Choisir la date"))
            .clicked()
        {
            shown = match shown {
                Some(_) => None,
                None => value.date().with_day(1),
            };
        }
        let mut h = value.hour();
        let mut m = value.minute();
        let rh = ui.add(
            egui::DragValue::new(&mut h)
                .range(0..=23)
                .custom_formatter(|v, _| format!("{v:02}")),
        );
        ui.label(":");
        let rm = ui.add(
            egui::DragValue::new(&mut m)
                .range(0..=59)
                .speed(0.5)
                .custom_formatter(|v, _| format!("{v:02}")),
        );
        if rh.changed() || rm.changed() {
            if let Some(t) = NaiveTime::from_hms_opt(h, m, 0) {
                value = value.date().and_time(t);
                changed = true;
            }
        }
        if ui.small_button(tr("+1 h")).clicked() {
            value += Duration::hours(1);
            changed = true;
        }
        if ui.small_button(tr("Demain 9:00")).clicked() {
            if let Some(t) = (now.date() + Duration::days(1)).and_hms_opt(9, 0, 0) {
                value = t;
                changed = true;
            }
        }
        if ui
            .small_button("✖")
            .on_hover_text(tr("Retirer l'alarme"))
            .clicked()
        {
            *alarm = None;
            shown = None;
            changed = true;
        }
    });
    if alarm.is_some() {
        if let Some(first) = shown {
            if let Some(day) = calendar(ui, first, value.date(), now.date(), &mut shown) {
                value = day.and_time(value.time());
                shown = None;
                changed = true;
            }
        }
        if value < now {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr("Date passée : le rappel partira tout de suite."),
            );
        }
        if changed {
            *alarm = Some(value);
        }
    }
    ui.data_mut(|d| d.insert_temp(id, shown));
    changed
}

/// Grille du mois commençant à `first`. Renvoie le jour cliqué.
fn calendar(
    ui: &mut egui::Ui,
    first: NaiveDate,
    selected: NaiveDate,
    today: NaiveDate,
    shown: &mut Option<NaiveDate>,
) -> Option<NaiveDate> {
    let mut picked = None;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            if ui.small_button("◀").clicked() {
                *shown = Some(shift_month(first, -1));
            }
            ui.strong(format!("{} {}", month_name(first.month()), first.year()));
            if ui.small_button("▶").clicked() {
                *shown = Some(shift_month(first, 1));
            }
            if ui.small_button(tr("Aujourd'hui")).clicked() {
                picked = Some(today);
            }
        });
        egui::Grid::new(("cal", first))
            .spacing([2.0, 2.0])
            .show(ui, |ui| {
                for d in 0..7 {
                    let wd = chrono::Weekday::try_from(d as u8).unwrap_or(chrono::Weekday::Mon);
                    ui.weak(day_name(wd));
                }
                ui.end_row();
                let offset = first.weekday().num_days_from_monday();
                for _ in 0..offset {
                    ui.label("");
                }
                let mut col = offset;
                let mut day = first;
                while day.month() == first.month() {
                    let mut text = egui::RichText::new(format!("{:>2}", day.day())).monospace();
                    if day == today {
                        text = text.underline().strong();
                    }
                    let past = day < today;
                    let resp = ui.add_enabled(
                        !past,
                        egui::Button::new(text)
                            .selected(day == selected)
                            .min_size(egui::vec2(30.0, 22.0)),
                    );
                    if resp.clicked() {
                        picked = Some(day);
                    }
                    col += 1;
                    if col == 7 {
                        col = 0;
                        ui.end_row();
                    }
                    day += Duration::days(1);
                }
            });
    });
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combos() {
        let ctrl_alt = egui::Modifiers {
            ctrl: true,
            alt: true,
            ..Default::default()
        };
        assert_eq!(
            combo_text(egui::Key::V, ctrl_alt).as_deref(),
            Some("Ctrl+Alt+V")
        );
        assert_eq!(combo_text(egui::Key::V, egui::Modifiers::NONE), None);
        assert!(is_modifier(egui::Key::AltLeft) && !is_modifier(egui::Key::V));
        assert_eq!(
            combo_text(egui::Key::F9, egui::Modifiers::NONE).as_deref(),
            Some("F9")
        );
        let shift = egui::Modifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        };
        assert_eq!(
            combo_text(egui::Key::Space, shift).as_deref(),
            Some("Ctrl+Shift+Space")
        );
        // Toutes ces combinaisons sont comprises par global-hotkey.
        for (k, m) in [
            (egui::Key::V, ctrl_alt),
            (egui::Key::Num1, ctrl_alt),
            (egui::Key::F9, egui::Modifiers::NONE),
            (egui::Key::ArrowUp, ctrl_alt),
            (egui::Key::Space, shift),
        ] {
            let text = combo_text(k, m).unwrap();
            assert!(crate::hotkey::parse(&text).is_ok(), "{text}");
        }
    }

    #[test]
    fn dates() {
        let t = NaiveDateTime::parse_from_str("2026/10/19 21:16:00", "%Y/%m/%d %H:%M:%S").unwrap();
        assert_eq!(next_full_hour(t).format("%H:%M").to_string(), "22:00");
        assert!(format_alarm(t).starts_with("lun. 19 octobre 2026"));
        let first = NaiveDate::from_ymd_opt(2026, 12, 1).unwrap();
        assert_eq!(
            shift_month(first, 1),
            NaiveDate::from_ymd_opt(2027, 1, 1).unwrap()
        );
        assert_eq!(
            shift_month(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), -1),
            NaiveDate::from_ymd_opt(2025, 12, 1).unwrap()
        );
    }
}
