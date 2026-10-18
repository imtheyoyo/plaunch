//! Interface egui : arbre des raccourcis, recherche, détails et statistiques.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use eframe::egui;

use crate::config::{self, Config, ItemPath};
use crate::launcher;
use crate::paths;

#[derive(PartialEq, Clone, Copy)]
enum View {
    Tree,
    MostUsed,
    Recent,
}

/// Action demandée pendant le dessin, appliquée ensuite (pour éviter
/// d'emprunter la configuration en écriture pendant qu'on la parcourt).
enum Action {
    Select(ItemPath),
    Launch(ItemPath),
    Reveal(ItemPath),
}

pub struct PStartApp {
    config: Option<Config>,
    load_error: Option<String>,
    search: String,
    selected: Option<ItemPath>,
    view: View,
    status: String,
    /// Existence des cibles, calculée en tâche de fond (les chemins réseau
    /// peuvent être lents à interroger).
    exists: HashMap<String, bool>,
    exists_rx: Option<Receiver<(String, bool)>>,
    focus_search: bool,
}

impl PStartApp {
    pub fn new(file: Option<PathBuf>) -> Self {
        let mut app = PStartApp {
            config: None,
            load_error: None,
            search: String::new(),
            selected: None,
            view: View::Tree,
            status: String::new(),
            exists: HashMap::new(),
            exists_rx: None,
            focus_search: true,
        };
        if let Some(f) = file {
            app.open(&f);
        }
        app
    }

    fn open(&mut self, file: &Path) {
        match Config::load(file) {
            Ok(cfg) => {
                let n = cfg.flatten().len();
                self.status = format!("{n} raccourcis chargés");
                self.start_existence_check(&cfg);
                self.config = Some(cfg);
                self.load_error = None;
                self.selected = None;
            }
            Err(e) => {
                self.load_error = Some(e);
            }
        }
    }

    fn reload(&mut self) {
        if let Some(file) = self.config.as_ref().map(|c| c.file.clone()) {
            self.open(&file);
        }
    }

    fn start_existence_check(&mut self, cfg: &Config) {
        let targets: Vec<String> = cfg.flatten().into_iter().map(|e| e.target).collect();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for t in targets {
                let ok = paths::to_native(&t).exists();
                if tx.send((t, ok)).is_err() {
                    break;
                }
            }
        });
        self.exists.clear();
        self.exists_rx = Some(rx);
    }

    fn poll_existence(&mut self) {
        if let Some(rx) = &self.exists_rx {
            while let Ok((t, ok)) = rx.try_recv() {
                self.exists.insert(t, ok);
            }
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Select(p) => self.selected = Some(p),
            Action::Launch(p) => {
                self.selected = Some(p.clone());
                self.launch(&p);
            }
            Action::Reveal(p) => {
                let Some(cfg) = &self.config else { return };
                let Some(el) = cfg.item(&p) else { return };
                let target = cfg.resolve(el);
                if let Err(e) = launcher::reveal(&target) {
                    self.status = e;
                }
            }
        }
    }

    fn launch(&mut self, path: &[usize]) {
        let Some(cfg) = &mut self.config else { return };
        let Some(el) = cfg.item(path) else { return };
        let name = config::display_name(el).to_string();
        let target = cfg.resolve(el);
        let workdir = cfg.working_dir(el);
        match launcher::launch(&target, workdir.as_deref()) {
            Ok(()) => {
                let now = chrono::Local::now().format(config::DATE_FORMAT).to_string();
                if let Some(el) = cfg.item_mut(path) {
                    config::record_run(el, &now);
                }
                self.status = match cfg.save() {
                    Ok(()) => format!("Lancé : {name}"),
                    Err(e) => format!("Lancé : {name} (statistiques non enregistrées : {e})"),
                };
            }
            Err(e) => self.status = format!("Échec : {e}"),
        }
    }

    // ---- Dessin -------------------------------------------------------

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let search = egui::TextEdit::singleline(&mut self.search)
                .hint_text("Rechercher… (Ctrl+F)")
                .desired_width(f32::INFINITY);
            let resp = ui.add(search);
            if self.focus_search {
                resp.request_focus();
                self.focus_search = false;
            }
        });
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.view, View::Tree, "Arborescence");
            ui.selectable_value(&mut self.view, View::MostUsed, "Plus utilisés");
            ui.selectable_value(&mut self.view, View::Recent, "Récents");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("⟳")
                    .on_hover_text("Recharger le fichier")
                    .clicked()
                {
                    self.reload();
                }
            });
        });
    }

    fn central(&mut self, ui: &mut egui::Ui) -> Vec<Action> {
        let mut actions = Vec::new();
        let Some(cfg) = &self.config else {
            self.welcome(ui);
            return actions;
        };
        let single = cfg.single_click();
        let query = self.search.trim().to_lowercase();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if !query.is_empty() {
                    let hits: Vec<_> = cfg
                        .flatten()
                        .into_iter()
                        .filter(|e| matches(e, &query))
                        .collect();
                    if hits.is_empty() {
                        ui.weak("Aucun résultat.");
                    }
                    for e in &hits {
                        flat_row(
                            ui,
                            e,
                            &self.selected,
                            &self.exists,
                            single,
                            true,
                            &mut actions,
                        );
                    }
                    return;
                }
                match self.view {
                    View::Tree => {
                        if let Some(files) = cfg.files() {
                            let mut path = Vec::new();
                            tree(
                                ui,
                                cfg,
                                files,
                                &mut path,
                                &self.selected,
                                &self.exists,
                                single,
                                &mut actions,
                            );
                        }
                    }
                    View::MostUsed => {
                        let mut all = cfg.flatten();
                        all.retain(|e| e.runcount > 0);
                        all.sort_by_key(|e| std::cmp::Reverse(e.runcount));
                        for e in all.iter().take(40) {
                            flat_row(
                                ui,
                                e,
                                &self.selected,
                                &self.exists,
                                single,
                                false,
                                &mut actions,
                            );
                        }
                    }
                    View::Recent => {
                        let mut all = cfg.flatten();
                        all.retain(|e| e.rundate.is_some());
                        // Le format AAAA/MM/JJ HH:MM:SS se trie comme du texte.
                        all.sort_by(|a, b| b.rundate.cmp(&a.rundate));
                        for e in all.iter().take(40) {
                            flat_row(
                                ui,
                                e,
                                &self.selected,
                                &self.exists,
                                single,
                                false,
                                &mut actions,
                            );
                        }
                    }
                }
            });
        actions
    }

    fn welcome(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.heading("PStart-rs");
            ui.add_space(12.0);
            ui.label("Glissez un fichier PStart.xml sur cette fenêtre,");
            ui.label("ou placez pstart-rs.exe à côté de votre PStart.xml.");
            if let Some(e) = &self.load_error {
                ui.add_space(12.0);
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
        });
    }

    fn details(&mut self, ui: &mut egui::Ui) -> Vec<Action> {
        let mut actions = Vec::new();
        let (Some(cfg), Some(sel)) = (&self.config, &self.selected) else {
            ui.weak("Sélectionnez un raccourci.");
            return actions;
        };
        let Some(el) = cfg.item(sel) else {
            return actions;
        };
        if el.name != "file" {
            ui.heading(config::display_name(el));
            return actions;
        }
        let target = cfg.resolve(el);
        ui.heading(config::display_name(el));
        ui.add_space(6.0);
        egui::Grid::new("details")
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.label("Chemin");
                ui.add(egui::Label::new(config::entry_path(el)).wrap());
                ui.end_row();
                ui.label("Cible");
                ui.add(egui::Label::new(&target).wrap());
                ui.end_row();
                if let Some(dir) = cfg.working_dir(el) {
                    ui.label("Dossier");
                    ui.add(egui::Label::new(dir).wrap());
                    ui.end_row();
                }
                ui.label("État");
                match self.exists.get(&target) {
                    Some(true) => ui.label("présent"),
                    Some(false) => ui.colored_label(ui.visuals().warn_fg_color, "introuvable"),
                    None => ui.weak("vérification…"),
                };
                ui.end_row();
                ui.label("Lancements");
                ui.label(config::runcount(el).to_string());
                ui.end_row();
                ui.label("Dernier");
                ui.label(el.child_text("rundate").unwrap_or("jamais"));
                ui.end_row();
            });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button("▶ Lancer").clicked() {
                actions.push(Action::Launch(sel.clone()));
            }
            if ui.button("Ouvrir l'emplacement").clicked() {
                actions.push(Action::Reveal(sel.clone()));
            }
        });
        actions
    }

    fn handle_input(&mut self, ctx: &egui::Context) -> Vec<Action> {
        let mut actions = Vec::new();
        // Fichier déposé sur la fenêtre.
        let dropped: Option<PathBuf> =
            ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()));
        if let Some(p) = dropped {
            self.open(&p);
        }
        let (ctrl_f, escape, enter) = ctx.input(|i| {
            (
                i.modifiers.command && i.key_pressed(egui::Key::F),
                i.key_pressed(egui::Key::Escape),
                i.key_pressed(egui::Key::Enter),
            )
        });
        if ctrl_f {
            self.focus_search = true;
        }
        if escape {
            self.search.clear();
        }
        if enter {
            // Entrée : lance la sélection, ou le premier résultat de recherche.
            let target = self.selected.clone().or_else(|| {
                let q = self.search.trim().to_lowercase();
                if q.is_empty() {
                    return None;
                }
                self.config
                    .as_ref()?
                    .flatten()
                    .into_iter()
                    .find(|e| matches(e, &q))
                    .map(|e| e.path)
            });
            if let Some(p) = target {
                actions.push(Action::Launch(p));
            }
        }
        actions
    }
}

impl eframe::App for PStartApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_existence();
        let mut actions = self.handle_input(&ctx);

        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(4.0);
            self.top_bar(ui);
            ui.add_space(2.0);
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                if let Some(cfg) = &self.config {
                    ui.separator();
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(cfg.file.display().to_string()).weak(),
                        )
                        .truncate(),
                    )
                    .on_hover_text(cfg.file.display().to_string());
                }
            });
        });
        egui::Panel::right("details")
            .resizable(true)
            .default_size(240.0)
            .show(ui, |ui| {
                ui.add_space(6.0);
                actions.extend(self.details(ui));
            });
        egui::CentralPanel::default().show(ui, |ui| {
            actions.extend(self.central(ui));
        });

        for a in actions {
            self.apply(a);
        }
        // Rafraîchit tant que la vérification des fichiers est en cours.
        if self.exists_rx.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
}

fn matches(e: &config::FlatEntry, query: &str) -> bool {
    e.name.to_lowercase().contains(query) || e.target.to_lowercase().contains(query)
}

#[allow(clippy::too_many_arguments)]
fn tree(
    ui: &mut egui::Ui,
    cfg: &Config,
    el: &crate::xml::Element,
    path: &mut ItemPath,
    selected: &Option<ItemPath>,
    exists: &HashMap<String, bool>,
    single: bool,
    actions: &mut Vec<Action>,
) {
    for (i, child) in el.children.iter().enumerate() {
        path.push(i);
        match child.name.as_str() {
            "files" => {
                let count = count_entries(child);
                let title = format!("{}  ({count})", config::display_name(child));
                egui::CollapsingHeader::new(title)
                    .id_salt(path.clone())
                    .default_open(false)
                    .show(ui, |ui| {
                        tree(ui, cfg, child, path, selected, exists, single, actions);
                    });
            }
            "file" => {
                let target = cfg.resolve(child);
                let missing = exists.get(&target) == Some(&false);
                entry_row(
                    ui,
                    config::display_name(child),
                    None,
                    missing,
                    selected.as_deref() == Some(path.as_slice()),
                    path,
                    single,
                    actions,
                );
            }
            _ => {}
        }
        path.pop();
    }
}

fn count_entries(el: &crate::xml::Element) -> usize {
    el.children
        .iter()
        .map(|c| match c.name.as_str() {
            "file" => 1,
            "files" => count_entries(c),
            _ => 0,
        })
        .sum()
}

fn flat_row(
    ui: &mut egui::Ui,
    e: &config::FlatEntry,
    selected: &Option<ItemPath>,
    exists: &HashMap<String, bool>,
    single: bool,
    show_groups: bool,
    actions: &mut Vec<Action>,
) {
    let info = if show_groups {
        e.groups.join(" › ")
    } else {
        match &e.rundate {
            Some(d) => format!("{} × · {}", e.runcount, d),
            None => format!("{} ×", e.runcount),
        }
    };
    entry_row(
        ui,
        &e.name,
        Some(&info),
        exists.get(&e.target) == Some(&false),
        selected.as_deref() == Some(e.path.as_slice()),
        &e.path,
        single,
        actions,
    );
}

#[allow(clippy::too_many_arguments)]
fn entry_row(
    ui: &mut egui::Ui,
    name: &str,
    info: Option<&str>,
    missing: bool,
    is_selected: bool,
    path: &[usize],
    single: bool,
    actions: &mut Vec<Action>,
) {
    let mut text = egui::RichText::new(name);
    if missing {
        text = text.color(ui.visuals().weak_text_color()).strikethrough();
    }
    ui.horizontal(|ui| {
        let resp = ui.selectable_label(is_selected, text);
        let resp = if missing {
            resp.on_hover_text("Cible introuvable")
        } else {
            resp
        };
        if let Some(info) = info {
            if !info.is_empty() {
                ui.weak(info);
            }
        }
        if resp.double_clicked() || (single && resp.clicked()) {
            actions.push(Action::Launch(path.to_vec()));
        } else if resp.clicked() {
            actions.push(Action::Select(path.to_vec()));
        }
        resp.context_menu(|ui| {
            if ui.button("Lancer").clicked() {
                actions.push(Action::Launch(path.to_vec()));
                ui.close();
            }
            if ui.button("Ouvrir l'emplacement").clicked() {
                actions.push(Action::Reveal(path.to_vec()));
                ui.close();
            }
        });
    });
}
