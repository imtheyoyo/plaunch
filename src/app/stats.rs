//! Vue « Statistiques » : totaux, raccourcis les plus lancés, activité
//! récente et nettoyage des raccourcis morts.

use eframe::egui;

use super::{Action, PStartApp};
use crate::config::{self, Config, FlatEntry};
use crate::i18n::{tr, trf};
use crate::xml::Element;

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

fn count_groups(el: &Element) -> usize {
    el.children
        .iter()
        .filter(|c| c.name == "files")
        .map(|c| 1 + count_groups(c))
        .sum()
}

impl PStartApp {
    pub(super) fn stats_view(&self, ui: &mut egui::Ui, cfg: &Config, actions: &mut Vec<Action>) {
        let entries = cfg.flatten();
        let total: u64 = entries.iter().map(|e| e.runcount as u64).sum();
        let groups = cfg.files().map(count_groups).unwrap_or(0);
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
        ui.add_space(6.0);
        egui::Grid::new("stats-totals")
            .num_columns(2)
            .spacing([16.0, 4.0])
            .show(ui, |ui| {
                ui.label(tr("Raccourcis"));
                ui.strong(entries.len().to_string());
                ui.end_row();
                ui.label(tr("Groupes"));
                ui.strong(groups.to_string());
                ui.end_row();
                ui.label(tr("Lancements enregistrés"));
                ui.strong(total.to_string());
                ui.end_row();
                if let Some(n) = cfg.number(&["execution", "count"]) {
                    ui.label(tr("Démarrages de PStart"));
                    ui.strong(n.to_string());
                    ui.end_row();
                }
                ui.label(tr("Introuvables"));
                if checked < entries.len() {
                    ui.weak(trf("{} (vérification…)", &[&dead.len()]));
                } else {
                    ui.strong(dead.len().to_string());
                }
                ui.end_row();
            });

        ui.add_space(12.0);
        ui.label(egui::RichText::new(tr("Les plus lancés")).strong());
        let mut top: Vec<&FlatEntry> = entries.iter().filter(|e| e.runcount > 0).collect();
        top.sort_by_key(|e| std::cmp::Reverse(e.runcount));
        top.truncate(TOP);
        if top.is_empty() {
            ui.weak(tr("Aucun lancement enregistré."));
        } else {
            self.bars(ui, &top, actions);
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
                    .on_hover_text(tr("Retire ces raccourcis de PStart.xml"))
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
                    .small_button("✖")
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
    let width = ui.available_width();
    let label_w = (width * 0.42).clamp(90.0, 220.0);
    let h = 20.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, h), egui::Sense::click());
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    if resp.hovered() {
        painter.rect_filled(rect, 3.0, visuals.widgets.hovered.weak_bg_fill);
    }
    let mut x = rect.left() + 2.0;
    if let Some(tex) = icon {
        let r = egui::Rect::from_min_size(egui::pos2(x, rect.top() + 2.0), egui::vec2(16.0, 16.0));
        painter.image(
            tex.id(),
            r,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        x += 20.0;
    }
    let font = egui::TextStyle::Body.resolve(ui.style());
    let text_color = visuals.text_color();
    let galley = painter.layout_no_wrap(label.to_string(), font.clone(), text_color);
    // Libellé sur une seule ligne, coupé s'il est trop long.
    let clip = egui::Rect::from_min_max(
        egui::pos2(x, rect.top()),
        egui::pos2(rect.left() + label_w - 6.0, rect.bottom()),
    );
    painter.with_clip_rect(clip).galley(
        egui::pos2(x, rect.center().y - galley.size().y / 2.0),
        galley,
        text_color,
    );
    let bar_left = rect.left() + label_w;
    let bar_max = (rect.right() - bar_left - 44.0).max(10.0);
    let frac = if max == 0 {
        0.0
    } else {
        value as f32 / max as f32
    };
    let bar = egui::Rect::from_min_size(
        egui::pos2(bar_left, rect.top() + 4.0),
        egui::vec2(
            (bar_max * frac).max(if value > 0 { 2.0 } else { 0.0 }),
            h - 8.0,
        ),
    );
    painter.rect_filled(bar, 3.0, visuals.selection.bg_fill);
    painter.text(
        egui::pos2(bar.right() + 6.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        value.to_string(),
        font,
        visuals.weak_text_color(),
    );
    resp.on_hover_text(format!("{label} : {value}"))
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
