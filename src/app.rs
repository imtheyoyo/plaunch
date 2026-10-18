//! Interface egui : arbre des raccourcis, recherche, détails et statistiques.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

use eframe::egui;

use crate::config::{self, Config, ItemPath};
use crate::settings::{self, Settings};
use crate::state::Shared;
use crate::xml::Element;
use crate::{hotkey, icons, launcher, paths, tray, window};

const ICON_SIZE: f32 = 18.0;

#[derive(PartialEq, Clone, Copy)]
enum View {
    Tree,
    MostUsed,
    Recent,
}

/// Action demandée pendant le dessin, appliquée ensuite, une fois la
/// configuration déverrouillée.
enum Action {
    Select(ItemPath),
    Launch(ItemPath),
    Reveal(ItemPath),
    Reload,
    Open(PathBuf),
}

type Textures = HashMap<String, egui::TextureHandle>;

pub struct PStartApp {
    shared: Arc<Shared>,
    settings: Settings,
    load_error: Option<String>,
    search: String,
    selected: Option<ItemPath>,
    view: View,
    /// Existence des cibles, calculée en tâche de fond (les chemins réseau
    /// peuvent être lents à interroger).
    exists: HashMap<String, bool>,
    exists_rx: Option<Receiver<(String, bool)>>,
    textures: Textures,
    focus_search: bool,
    first_frame: bool,
    tray: Option<tray::Tray>,
    hotkey_info: Option<String>,
    _hotkey: Option<global_hotkey::GlobalHotKeyManager>,
}

impl PStartApp {
    pub fn new(cc: &eframe::CreationContext<'_>, file: Option<PathBuf>) -> Self {
        let shared = Arc::new(Shared::default());
        let _ = shared.ctx.set(cc.egui_ctx.clone());
        window::capture(cc);

        let settings = file
            .as_deref()
            .map(|f| settings::load(&settings::path_for(f)))
            .unwrap_or_default();

        let mut app = PStartApp {
            shared,
            settings,
            load_error: None,
            search: String::new(),
            selected: None,
            view: View::Tree,
            exists: HashMap::new(),
            exists_rx: None,
            textures: HashMap::new(),
            focus_search: true,
            first_frame: true,
            tray: None,
            hotkey_info: None,
            _hotkey: None,
        };
        if let Some(f) = file {
            app.open(&f);
        }
        app.init_desktop_integration();
        app
    }

    /// Icône de notification et raccourci global (Windows uniquement).
    fn init_desktop_integration(&mut self) {
        if !cfg!(windows) {
            return;
        }
        let mut problems = Vec::new();
        if self.settings.tray {
            match tray::Tray::new(&self.shared) {
                Ok(t) => self.tray = Some(t),
                Err(e) => problems.push(e),
            }
        }
        if let Some(spec) = self.settings.hotkey.clone() {
            match hotkey::register(&spec, self.shared.clone()) {
                Ok(m) => {
                    self._hotkey = Some(m);
                    self.hotkey_info = Some(spec);
                }
                Err(e) => problems.push(e),
            }
        }
        if !problems.is_empty() {
            self.shared.set_status(problems.join(" ; "));
        }
    }

    fn open(&mut self, file: &Path) {
        match Config::load(file) {
            Ok(cfg) => {
                let generation = self.shared.next_generation();
                let targets: Vec<String> = cfg.flatten().into_iter().map(|e| e.target).collect();
                self.shared
                    .set_status(format!("{} raccourcis chargés", targets.len()));
                self.start_existence_check(targets.clone());
                self.shared.icons.lock().unwrap().clear();
                self.textures.clear();
                *self.shared.config.lock().unwrap() = Some(cfg);
                icons::spawn_loader(targets, self.shared.clone(), generation);
                self.load_error = None;
                self.selected = None;
            }
            Err(e) => self.load_error = Some(e),
        }
    }

    fn reload(&mut self) {
        let file = self
            .shared
            .config
            .lock()
            .unwrap()
            .as_ref()
            .map(|c| c.file.clone());
        if let Some(file) = file {
            self.open(&file);
        }
    }

    fn start_existence_check(&mut self, targets: Vec<String>) {
        let (tx, rx) = mpsc::channel();
        let shared = self.shared.clone();
        std::thread::spawn(move || {
            for t in targets {
                let ok = paths::to_native(&t).exists();
                if tx.send((t, ok)).is_err() {
                    break;
                }
            }
            shared.repaint();
        });
        self.exists.clear();
        self.exists_rx = Some(rx);
    }

    fn poll_existence(&mut self) {
        if let Some(rx) = &self.exists_rx {
            loop {
                match rx.try_recv() {
                    Ok((t, ok)) => {
                        self.exists.insert(t, ok);
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.exists_rx = None;
                        break;
                    }
                }
            }
        }
    }

    /// Transforme en textures les icônes nouvellement extraites.
    fn sync_textures(&mut self, ctx: &egui::Context) {
        let icons = self.shared.icons.lock().unwrap();
        if icons.len() == self.textures.len() {
            return;
        }
        for (target, img) in icons.iter() {
            if self.textures.contains_key(target) {
                continue;
            }
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [img.width as usize, img.height as usize],
                &img.rgba,
            );
            let tex = ctx.load_texture(target, image, egui::TextureOptions::LINEAR);
            self.textures.insert(target.clone(), tex);
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Select(p) => self.selected = Some(p),
            Action::Launch(p) => {
                self.selected = Some(p.clone());
                self.shared.launch(&p);
            }
            Action::Reveal(p) => {
                let target = {
                    let guard = self.shared.config.lock().unwrap();
                    guard
                        .as_ref()
                        .and_then(|c| c.item(&p).map(|el| c.resolve(el)))
                };
                if let Some(t) = target {
                    if let Err(e) = launcher::reveal(&t) {
                        self.shared.set_status(e);
                    }
                }
            }
            Action::Reload => self.reload(),
            Action::Open(p) => self.open(&p),
        }
    }

    // ---- Dessin -------------------------------------------------------

    fn top_bar(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
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
                    actions.push(Action::Reload);
                }
            });
        });
    }

    fn central(&self, ui: &mut egui::Ui, cfg: Option<&Config>) -> Vec<Action> {
        let mut actions = Vec::new();
        let Some(cfg) = cfg else {
            self.welcome(ui);
            return actions;
        };
        let rows = Rows {
            selected: &self.selected,
            exists: &self.exists,
            textures: &self.textures,
            single: cfg.single_click(),
        };
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
                        rows.flat(ui, e, true, &mut actions);
                    }
                    return;
                }
                match self.view {
                    View::Tree => {
                        if let Some(files) = cfg.files() {
                            let mut path = Vec::new();
                            rows.tree(ui, cfg, files, &mut path, &mut actions);
                        }
                    }
                    View::MostUsed => {
                        let mut all = cfg.flatten();
                        all.retain(|e| e.runcount > 0);
                        all.sort_by_key(|e| std::cmp::Reverse(e.runcount));
                        for e in all.iter().take(40) {
                            rows.flat(ui, e, false, &mut actions);
                        }
                    }
                    View::Recent => {
                        let mut all = cfg.flatten();
                        all.retain(|e| e.rundate.is_some());
                        // Le format AAAA/MM/JJ HH:MM:SS se trie comme du texte.
                        all.sort_by(|a, b| b.rundate.cmp(&a.rundate));
                        for e in all.iter().take(40) {
                            rows.flat(ui, e, false, &mut actions);
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

    fn details(&self, ui: &mut egui::Ui, cfg: Option<&Config>) -> Vec<Action> {
        let mut actions = Vec::new();
        let Some(cfg) = cfg else {
            return actions;
        };
        let Some(sel) = &self.selected else {
            ui.weak("Sélectionnez un raccourci.");
            ui.add_space(16.0);
            self.shortcuts_help(ui);
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
        ui.horizontal(|ui| {
            if let Some(tex) = self.textures.get(&target) {
                ui.add(
                    egui::Image::from_texture(egui::load::SizedTexture::from_handle(tex))
                        .fit_to_exact_size(egui::vec2(32.0, 32.0)),
                );
            }
            ui.heading(config::display_name(el));
        });
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

    fn shortcuts_help(&self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Raccourcis").strong());
        ui.weak("Entrée : lancer");
        ui.weak("Ctrl+F : rechercher, Échap : effacer");
        match &self.hotkey_info {
            Some(hk) => ui.weak(format!("{hk} : afficher / masquer")),
            None => ui.weak("Raccourci global : désactivé"),
        };
        ui.add_space(8.0);
        ui.weak(format!("Réglages : {}", settings::FILE_NAME));
    }

    fn handle_input(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        // Fichier déposé sur la fenêtre.
        let dropped: Option<PathBuf> =
            ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()));
        if let Some(p) = dropped {
            actions.push(Action::Open(p));
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
            if self.search.is_empty() && self.tray.is_some() {
                window::hide();
            }
            self.search.clear();
        }
        if enter {
            // Entrée : lance la sélection, ou le premier résultat de recherche.
            let q = self.search.trim().to_lowercase();
            let target = if !q.is_empty() {
                let guard = self.shared.config.lock().unwrap();
                guard
                    .as_ref()
                    .and_then(|c| c.flatten().into_iter().find(|e| matches(e, &q)))
                    .map(|e| e.path)
            } else {
                self.selected.clone()
            };
            if let Some(p) = target {
                actions.push(Action::Launch(p));
            }
        }
    }

    fn handle_close(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }
        let quitting = self.shared.quitting.load(Ordering::SeqCst);
        if !quitting && self.settings.close_to_tray && self.tray.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            window::hide();
        }
    }
}

impl eframe::App for PStartApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_existence();
        self.sync_textures(&ctx);
        if let Some(t) = &mut self.tray {
            t.refresh(&self.shared);
        }
        if self.shared.focus_search.swap(false, Ordering::SeqCst) {
            self.focus_search = true;
        }
        if self.first_frame {
            self.first_frame = false;
            if self.settings.start_hidden && self.tray.is_some() {
                window::hide();
            }
        }
        self.handle_close(&ctx);

        let mut actions = Vec::new();
        self.handle_input(&ctx, &mut actions);

        let shared = self.shared.clone();
        let status = shared.status.lock().unwrap().clone();
        {
            let guard = shared.config.lock().unwrap();
            let cfg = guard.as_ref();

            egui::Panel::top("top").show(ui, |ui| {
                ui.add_space(4.0);
                self.top_bar(ui, &mut actions);
                ui.add_space(2.0);
            });
            egui::Panel::bottom("status").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(&status);
                    if let Some(cfg) = cfg {
                        let file = cfg.file.display().to_string();
                        ui.separator();
                        ui.add(egui::Label::new(egui::RichText::new(&file).weak()).truncate())
                            .on_hover_text(&file);
                    }
                });
            });
            egui::Panel::right("details")
                .resizable(true)
                .default_size(250.0)
                .show(ui, |ui| {
                    ui.add_space(6.0);
                    actions.extend(self.details(ui, cfg));
                });
            egui::CentralPanel::default().show(ui, |ui| {
                actions.extend(self.central(ui, cfg));
            });
        }

        for a in actions {
            self.apply(a);
        }
    }
}

fn matches(e: &config::FlatEntry, query: &str) -> bool {
    e.name.to_lowercase().contains(query) || e.target.to_lowercase().contains(query)
}

/// Contexte commun au dessin des lignes.
struct Rows<'a> {
    selected: &'a Option<ItemPath>,
    exists: &'a HashMap<String, bool>,
    textures: &'a Textures,
    single: bool,
}

impl Rows<'_> {
    fn tree(
        &self,
        ui: &mut egui::Ui,
        cfg: &Config,
        el: &Element,
        path: &mut ItemPath,
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
                            self.tree(ui, cfg, child, path, actions);
                        });
                }
                "file" => {
                    let target = cfg.resolve(child);
                    self.row(
                        ui,
                        config::display_name(child),
                        &target,
                        None,
                        path,
                        actions,
                    );
                }
                _ => {}
            }
            path.pop();
        }
    }

    fn flat(
        &self,
        ui: &mut egui::Ui,
        e: &config::FlatEntry,
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
        self.row(ui, &e.name, &e.target, Some(&info), &e.path, actions);
    }

    fn row(
        &self,
        ui: &mut egui::Ui,
        name: &str,
        target: &str,
        info: Option<&str>,
        path: &[usize],
        actions: &mut Vec<Action>,
    ) {
        let missing = self.exists.get(target) == Some(&false);
        let is_selected = self.selected.as_deref() == Some(path);
        let mut text = egui::RichText::new(name);
        if missing {
            text = text.color(ui.visuals().weak_text_color()).strikethrough();
        }
        ui.horizontal(|ui| {
            match self.textures.get(target) {
                Some(tex) => {
                    ui.add(
                        egui::Image::from_texture(egui::load::SizedTexture::from_handle(tex))
                            .fit_to_exact_size(egui::vec2(ICON_SIZE, ICON_SIZE)),
                    );
                }
                None if !self.textures.is_empty() => {
                    ui.add_space(ICON_SIZE + ui.spacing().item_spacing.x);
                }
                None => {}
            }
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
            if resp.double_clicked() || (self.single && resp.clicked()) {
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
}

fn count_entries(el: &Element) -> usize {
    el.children
        .iter()
        .map(|c| match c.name.as_str() {
            "file" => 1,
            "files" => count_entries(c),
            _ => 0,
        })
        .sum()
}
