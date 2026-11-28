//! Vue « Statistiques » : totaux, raccourcis les plus lancés, activité
//! récente et nettoyage des raccourcis morts.

use eframe::egui;

use super::{modern, Action, PLaunchApp};
use crate::config::{self, Config, FlatEntry};
use crate::i18n::{tr, trf};

const TOP: usize = 15;

/// Répartition par date du dernier lancement.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Activity {
    pub week: usize,
    pub month: usize,
    pub year: usize,
    pub older: usize,
    pub never: usize,
}

pub fn activity(entries: &[FlatEntry], now: chrono::NaiveDateTime) -> Activity {
    let mut a = Activity::default();
    for e in entries {
        let date = e.rundate.as_deref().and_then(|d| {
            chrono::NaiveDateTime::parse_from_str(d.trim(), config::DATE_FORMAT).ok()
        });
        match date.map(|d| (now - d).num_days()) {
            None => a.never += 1,
            Some(d) if d <= 7 => a.week += 1,
            Some(d) if d <= 31 => a.month += 1,
            Some(d) if d <= 365 => a.year += 1,
            Some(_) => a.older += 1,
        }
    }
    a
}

impl PLaunchApp {
    pub(super) fn stats_view(&self, ui: &mut egui::Ui, cfg: &Config, actions: &mut Vec<Action>) {
        let tok = modern::tokens(ui.visuals());
        let entries: Vec<FlatEntry> = self.index.entries.iter().map(|c| c.entry.clone()).collect();
        let total: u64 = entries.iter().map(|e| e.runcount as u64).sum();
        let groups = self.index.groups_count;
        let dead: Vec<&FlatEntry> = entries
            .iter()
            .filter(|e| self.exists.get(&e.target) == Some(&false))
            .collect();
        let checked = entries
            .iter()
            .filter(|e| self.exists.contains_key(&e.target))
            .count();
        let act = activity(&entries, chrono::Local::now().naive_local());

        ui.heading(tr("Statistiques"));
        ui.add_space(8.0);
        let missing_text = if checked < entries.len() {
            trf("{} (vérification…)", &[&dead.len()])
        } else {
            dead.len().to_string()
        };
        let kpis = [
            (tr("Raccourcis"), entries.len().to_string(), None),
            (tr("Groupes"), groups.to_string(), None),
            (tr("Lancements enregistrés"), total.to_string(), None),
            (
                tr("Introuvables"),
                missing_text,
                (!dead.is_empty()).then_some(tok.warning),
            ),
        ];
        let cols = if ui.available_width() >= 420.0 { 4 } else { 2 };
        ui.columns(cols, |columns| {
            for (idx, (label, value, color)) in kpis.iter().enumerate() {
                let col = &mut columns[idx % cols];
                egui::Frame::new()
                    .fill(tok.card)
                    .stroke(egui::Stroke::new(1.0, tok.border))
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(col, |ui| {
                        ui.set_width(ui.available_width());
                        let mut v = egui::RichText::new(value).size(18.0).strong();
                        if let Some(c) = color {
                            v = v.color(*c);
                        }
                        ui.label(v);
                        ui.add(
                            egui::Label::new(egui::RichText::new(*label).small().weak()).truncate(),
                        );
                    });
                if idx + cols < kpis.len() {
                    col.add_space(6.0);
                }
            }
        });
        if let Some(n) = cfg.number(&["execution", "count"]) {
            ui.add_space(4.0);
            ui.weak(format!("{}: {n}", tr("Démarrages de PLaunch")));
        }

        ui.add_space(12.0);
        ui.label(egui::RichText::new(tr("Les plus lancés")).strong());
        let top: Vec<&FlatEntry> = self.index.most_used.iter().take(TOP).collect();
        if top.is_empty() {
            ui.weak(tr("Aucun lancement enregistré."));
        } else {
            self.bars(ui, &top, actions);
        }

        ui.add_space(12.0);
        ui.label(egui::RichText::new(tr("Temps d'utilisation")).strong());
        let usage = self.shared.usage.lock().unwrap().clone();
        let mut timed: Vec<(&FlatEntry, u64)> = entries
            .iter()
            .filter_map(|e| {
                let el = cfg.item(&e.path)?;
                let secs = *usage.get(&crate::usage::key(config::entry_path(el)))?;
                (secs > 0).then_some((e, secs))
            })
            .collect();
        timed.sort_by_key(|t| std::cmp::Reverse(t.1));
        timed.truncate(TOP);
        if timed.is_empty() {
            ui.weak(tr(
                "Pas encore mesuré : le temps des programmes ouverts est compté en continu.",
            ));
        } else {
            let total: u64 = usage.values().sum();
            ui.weak(trf("Total : {}", &[&crate::usage::format_duration(total)]));
            let max = timed[0].1 as usize;
            for (e, secs) in &timed {
                let resp = bar_row_text(
                    ui,
                    &e.name,
                    *secs as usize,
                    &crate::usage::format_duration(*secs),
                    max,
                    self.textures.get(&e.target),
                );
                if resp.clicked() {
                    actions.push(Action::Select(e.path.clone()));
                }
            }
        }

        ui.add_space(12.0);
        ui.label(egui::RichText::new(tr("Dernier lancement")).strong());
        let rows = [
            (tr("cette semaine"), act.week),
            (tr("ce mois-ci"), act.month),
            (tr("cette année"), act.year),
            (tr("plus d'un an"), act.older),
            (tr("jamais"), act.never),
        ];
        let max = rows.iter().map(|r| r.1).max().unwrap_or(0);
        for (label, n) in rows {
            bar_row(ui, label, n, max, None);
        }

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(tr("Raccourcis introuvables")).strong());
            if !dead.is_empty()
                && ui
                    .button(tr("Tout supprimer…"))
                    .on_hover_text(tr("Retire ces raccourcis de PLaunch.xml"))
                    .clicked()
            {
                actions.push(Action::AskCleanup(
                    dead.iter().map(|e| e.path.clone()).collect(),
                ));
            }
        });
        if dead.is_empty() {
            ui.weak(tr("Aucun : tous les raccourcis vérifiés sont valides."));
        }
        for e in &dead {
            ui.horizontal(|ui| {
                if ui
                    .small_button(super::icon::X)
                    .on_hover_text(tr("Supprimer…"))
                    .clicked()
                {
                    actions.push(Action::AskDelete(e.path.clone()));
                }
                if ui.link(&e.name).clicked() {
                    actions.push(Action::Edit(e.path.clone()));
                }
                ui.add(egui::Label::new(egui::RichText::new(&e.target).weak().small()).truncate());
            });
        }

        let never: Vec<&FlatEntry> = entries.iter().filter(|e| e.runcount == 0).collect();
        if !never.is_empty() {
            ui.add_space(12.0);
            egui::CollapsingHeader::new(trf("Jamais lancés ({})", &[&never.len()]))
                .id_salt("never")
                .show(ui, |ui| {
                    for e in never {
                        let resp = ui.selectable_label(false, &e.name);
                        if resp.double_clicked() {
                            actions.push(Action::Launch(e.path.clone()));
                        } else if resp.clicked() {
                            actions.push(Action::Select(e.path.clone()));
                        }
                    }
                });
        }
    }

    fn bars(&self, ui: &mut egui::Ui, top: &[&FlatEntry], actions: &mut Vec<Action>) {
        let max = top.first().map_or(1, |e| e.runcount as usize);
        for e in top {
            let resp = bar_row(
                ui,
                &e.name,
                e.runcount as usize,
                max,
                self.textures.get(&e.target),
            );
            if resp.double_clicked() {
                actions.push(Action::Launch(e.path.clone()));
            } else if resp.clicked() {
                actions.push(Action::Select(e.path.clone()));
            }
        }
    }
}

/// Ligne « libellé — barre — valeur ».
fn bar_row(
    ui: &mut egui::Ui,
    label: &str,
    value: usize,
    max: usize,
    icon: Option<&egui::TextureHandle>,
) -> egui::Response {
    bar_row_text(ui, label, value, &value.to_string(), max, icon)
}

/// Variante dont la valeur affichée est un texte (durée…).
fn bar_row_text(
    ui: &mut egui::Ui,
    label: &str,
    value: usize,
    shown: &str,
    max: usize,
    icon: Option<&egui::TextureHandle>,
) -> egui::Response {
    let tok = modern::tokens(ui.visuals());
    let width = ui.available_width();
    let label_w = (width * 0.40).clamp(90.0, 210.0);
    let h = 22.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, h), egui::Sense::click());
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    if resp.hovered() {
        painter.rect_filled(rect, 5.0, visuals.widgets.hovered.weak_bg_fill);
    }
    let mut x = rect.left() + 4.0;
    if let Some(tex) = icon {
        let r = egui::Rect::from_min_size(egui::pos2(x, rect.top() + 3.0), egui::vec2(16.0, 16.0));
        painter.image(
            tex.id(),
            r,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        x += 22.0;
    }
    let font = egui::TextStyle::Body.resolve(ui.style());
    let text_color = visuals.text_color();
    let galley = painter.layout_no_wrap(label.to_string(), font.clone(), text_color);
    let clip = egui::Rect::from_min_max(
        egui::pos2(x, rect.top()),
        egui::pos2(rect.left() + label_w - 8.0, rect.bottom()),
    );
    painter.with_clip_rect(clip).galley(
        egui::pos2(x, rect.center().y - galley.size().y / 2.0),
        galley,
        text_color,
    );
    let val_galley = painter.layout_no_wrap(shown.to_string(), font, visuals.weak_text_color());
    let val_w = val_galley.size().x + 12.0;
    let bar_left = rect.left() + label_w;
    let bar_max = (rect.right() - bar_left - val_w).max(16.0);
    let track = egui::Rect::from_min_size(
        egui::pos2(bar_left, rect.top() + 6.0),
        egui::vec2(bar_max, h - 12.0),
    );
    painter.rect_filled(track, 4.0, tok.field);
    let frac = if max == 0 {
        0.0
    } else {
        (value as f32 / max as f32).clamp(0.0, 1.0)
    };
    if value > 0 {
        let bar = egui::Rect::from_min_size(
            track.min,
            egui::vec2((bar_max * frac).max(4.0), track.height()),
        );
        painter.rect_filled(bar, 4.0, tok.accent);
    }
    painter.galley(
        egui::pos2(
            track.right() + 6.0,
            rect.center().y - val_galley.size().y / 2.0,
        ),
        val_galley,
        visuals.weak_text_color(),
    );
    resp.on_hover_text(format!("{label} : {shown}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(date: Option<&str>) -> FlatEntry {
        FlatEntry {
            path: vec![0],
            groups: vec![],
            name: "x".into(),
            target: "x".into(),
            runcount: 1,
            rundate: date.map(str::to_string),
            favorite: false,
            favorite_rank: 0,
            autorun: false,
        }
    }

    #[test]
    fn activity_buckets() {
        let now = chrono::NaiveDateTime::parse_from_str("2026/10/18 12:00:00", config::DATE_FORMAT)
            .unwrap();
        let list = [
            entry(Some("2026/10/17 09:00:00")),
            entry(Some("2026/10/01 09:00:00")),
            entry(Some("2026/03/01 09:00:00")),
            entry(Some("2020/01/01 09:00:00")),
            entry(None),
            entry(Some("illisible")),
        ];
        assert_eq!(
            activity(&list, now),
            Activity {
                week: 1,
                month: 1,
                year: 1,
                older: 1,
                never: 2
            }
        );
    }
}
