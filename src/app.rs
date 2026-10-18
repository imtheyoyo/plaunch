//! Interface egui : arbre des raccourcis, recherche, édition et statistiques.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use eframe::egui;

use crate::config::{self, Config, ItemPath, SearchOptions};
use crate::edit::{self, NewItem};
use crate::settings::{self, Settings};
use crate::startmenu::{self, Source, StartItem};
use crate::state::Shared;
use crate::xml::Element;
use crate::{hotkey, icons, launcher, paths, tray, window};

const ICON_SIZE: f32 = 18.0;
const MAX_HITS: usize = 60;

#[derive(PartialEq, Clone, Copy)]
enum View {
    Tree,
    MostUsed,
    Recent,
}

/// Charge utile d'un glisser-déposer interne.
#[derive(Clone)]
struct DragItem(ItemPath);

/// Ce que le sélecteur de fichiers doit remplir.
#[derive(Clone, Copy)]
enum Browse {
    EntryPath,
    WorkingDir,
    AddFiles,
}

/// Action demandée pendant le dessin, appliquée ensuite, une fois la
/// configuration déverrouillée.
enum Action {
    Select(ItemPath),
    Launch(ItemPath),
    Reveal(ItemPath),
    RevealPath(String),
    Reload,
    Open(PathBuf),
    LaunchExternal {
        name: String,
        path: String,
    },
    AddExternal {
        name: String,
        path: String,
    },
    AddFiles(Vec<PathBuf>),
    NewEntry(Option<ItemPath>),
    NewGroup(Option<ItemPath>),
    Edit(ItemPath),
    AskDelete(ItemPath),
    Delete(ItemPath),
    Move {
        from: ItemPath,
        to_parent: ItemPath,
        to_index: usize,
    },
    Browse(Browse),
    SubmitForm,
    CloseDialog,
}

enum FormKind {
    NewEntry {
        parent: ItemPath,
        index: Option<usize>,
    },
    EditEntry(ItemPath),
    NewGroup {
        parent: ItemPath,
        index: Option<usize>,
    },
    RenameGroup(ItemPath),
}

struct EditForm {
    kind: FormKind,
    name: String,
    path: String,
    directory: String,
    error: Option<String>,
    focus: bool,
}

impl EditForm {
    fn is_group(&self) -> bool {
        matches!(
            self.kind,
            FormKind::NewGroup { .. } | FormKind::RenameGroup(_)
        )
    }

    fn title(&self) -> &'static str {
        match self.kind {
            FormKind::NewEntry { .. } => "Nouveau raccourci",
            FormKind::EditEntry(_) => "Modifier le raccourci",
            FormKind::NewGroup { .. } => "Nouveau groupe",
            FormKind::RenameGroup(_) => "Renommer le groupe",
        }
    }
}

enum Dialog {
    Form(EditForm),
    ConfirmDelete {
        path: ItemPath,
        name: String,
        count: usize,
        group: bool,
    },
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
    exists_tx: Sender<(String, bool)>,
    exists_rx: Receiver<(String, bool)>,
    textures: Textures,
    dialog: Option<Dialog>,
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

        let (exists_tx, exists_rx) = mpsc::channel();
        let mut app = PStartApp {
            shared,
            settings,
            load_error: None,
            search: String::new(),
            selected: None,
            view: View::Tree,
            exists: HashMap::new(),
            exists_tx,
            exists_rx,
            textures: HashMap::new(),
            dialog: None,
            focus_search: true,
            first_frame: true,
            tray: None,
            hotkey_info: None,
            _hotkey: None,
        };
        if let Some(f) = file {
            app.open(&f);
        }
        app.scan_start_menu();
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

    fn scan_start_menu(&self) {
        let shared = self.shared.clone();
        std::thread::spawn(move || {
            let items = startmenu::scan();
            let paths: Vec<String> = items.iter().map(|i| i.path.clone()).collect();
            *shared.start_menu.lock().unwrap() = items;
            let generation = shared.generation.load(Ordering::SeqCst);
            icons::spawn_loader(paths, shared.clone(), generation);
            shared.repaint();
        });
    }

    fn open(&mut self, file: &Path) {
        match Config::load(file) {
            Ok(cfg) => {
                let generation = self.shared.next_generation();
                let mut targets: Vec<String> =
                    cfg.flatten().into_iter().map(|e| e.target).collect();
                self.shared
                    .set_status(format!("{} raccourcis chargés", targets.len()));
                self.exists.clear();
                self.check_existence(targets.clone());
                self.shared.icons.lock().unwrap().clear();
                self.textures.clear();
                *self.shared.config.lock().unwrap() = Some(cfg);
                // Les icônes du menu Démarrer sont recalculées avec le reste.
                targets.extend(
                    self.shared
                        .start_menu
                        .lock()
                        .unwrap()
                        .iter()
                        .map(|i| i.path.clone()),
                );
                icons::spawn_loader(targets, self.shared.clone(), generation);
                self.load_error = None;
                self.selected = None;
                self.dialog = None;
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

    fn check_existence(&self, targets: Vec<String>) {
        let tx = self.exists_tx.clone();
        let shared = self.shared.clone();
        std::thread::spawn(move || {
            for t in targets {
                let ok = paths::to_native(&t).exists();
                if tx.send((t, ok)).is_err() {
                    return;
                }
            }
            shared.repaint();
        });
    }

    /// Vérifie et charge l'icône d'une cible ajoutée ou modifiée.
    fn refresh_target(&self, target: String) {
        self.check_existence(vec![target.clone()]);
        let generation = self.shared.generation.load(Ordering::SeqCst);
        icons::spawn_loader(vec![target], self.shared.clone(), generation);
    }

    fn poll_existence(&mut self) {
        while let Ok((t, ok)) = self.exists_rx.try_recv() {
            self.exists.insert(t, ok);
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

    fn base_dir(&self) -> String {
        self.shared
            .config
            .lock()
            .unwrap()
            .as_ref()
            .map(|c| c.base_dir.clone())
            .unwrap_or_default()
    }

    fn resolve_path(&self, raw: &str) -> String {
        paths::resolve(raw, &self.base_dir())
    }

    // ---- Application des actions --------------------------------------

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
                    self.apply(Action::RevealPath(t));
                }
            }
            Action::RevealPath(t) => {
                if let Err(e) = launcher::reveal(&t) {
                    self.shared.set_status(e);
                }
            }
            Action::Reload => self.reload(),
            Action::Open(p) => self.open(&p),
            Action::LaunchExternal { name, path } => self.shared.launch_external(&name, &path),
            Action::AddExternal { name, path } => {
                let rel = paths::make_relative(&path, &self.base_dir());
                self.add_entries(vec![(name, rel)]);
            }
            Action::AddFiles(files) => {
                let base = self.base_dir();
                let items = files
                    .iter()
                    .filter_map(|f| {
                        let name = f
                            .file_stem()
                            .or(f.file_name())?
                            .to_string_lossy()
                            .into_owned();
                        let full = f.to_string_lossy().into_owned();
                        Some((name, paths::make_relative(&full, &base)))
                    })
                    .collect();
                self.add_entries(items);
            }
            Action::NewEntry(at) => self.open_form(at, false),
            Action::NewGroup(at) => self.open_form(at, true),
            Action::Edit(p) => self.open_edit(p),
            Action::AskDelete(p) => self.ask_delete(p),
            Action::Delete(p) => {
                if self.shared.modify(|c| edit::delete(c, &p)).is_some() {
                    self.selected = None;
                    self.shared.set_status("Supprimé");
                }
                self.dialog = None;
            }
            Action::Move {
                from,
                to_parent,
                to_index,
            } => {
                if to_parent.starts_with(&from) {
                    return; // déposé sur lui-même
                }
                if let Some(p) = self
                    .shared
                    .modify(|c| edit::move_item(c, &from, &to_parent, to_index))
                {
                    self.selected = Some(p);
                }
            }
            Action::Browse(kind) => self.browse(kind),
            Action::SubmitForm => self.submit_form(),
            Action::CloseDialog => self.dialog = None,
        }
    }

    /// Ajoute des raccourcis à l'emplacement courant (après la sélection).
    fn add_entries(&mut self, items: Vec<(String, String)>) {
        if items.is_empty() {
            return;
        }
        let selected = self.selected.clone();
        let count = items.len();
        let targets: Vec<String> = items.iter().map(|(_, p)| self.resolve_path(p)).collect();
        let last = self.shared.modify(|c| {
            let (parent, mut index) = edit::insertion_point(c, selected.as_deref());
            let mut last = None;
            for (name, path) in items {
                let p = edit::insert(
                    c,
                    &parent,
                    index,
                    NewItem::Entry {
                        name,
                        path,
                        directory: None,
                    },
                )?;
                index = Some(p.last().copied().unwrap_or(0) + 1);
                last = Some(p);
            }
            Ok(last)
        });
        if let Some(Some(p)) = last {
            self.selected = Some(p);
            self.shared.set_status(match count {
                1 => "Raccourci ajouté".to_string(),
                n => format!("{n} raccourcis ajoutés"),
            });
            for t in targets {
                self.refresh_target(t);
            }
        }
    }

    fn open_form(&mut self, at: Option<ItemPath>, group: bool) {
        let (parent, index) = {
            let guard = self.shared.config.lock().unwrap();
            let Some(cfg) = guard.as_ref() else { return };
            let sel = at.as_deref().or(self.selected.as_deref());
            edit::insertion_point(cfg, sel)
        };
        let kind = if group {
            FormKind::NewGroup { parent, index }
        } else {
            FormKind::NewEntry { parent, index }
        };
        self.dialog = Some(Dialog::Form(EditForm {
            kind,
            name: String::new(),
            path: String::new(),
            directory: String::new(),
            error: None,
            focus: true,
        }));
    }

    fn open_edit(&mut self, path: ItemPath) {
        let form = {
            let guard = self.shared.config.lock().unwrap();
            let Some(el) = guard.as_ref().and_then(|c| c.item(&path)) else {
                return;
            };
            let name = el.attr("name").unwrap_or("").trim().to_string();
            if el.name == "files" {
                EditForm {
                    kind: FormKind::RenameGroup(path),
                    name,
                    path: String::new(),
                    directory: String::new(),
                    error: None,
                    focus: true,
                }
            } else {
                EditForm {
                    kind: FormKind::EditEntry(path),
                    name,
                    path: config::entry_path(el).to_string(),
                    directory: el.child_text("directory").unwrap_or("").to_string(),
                    error: None,
                    focus: true,
                }
            }
        };
        self.dialog = Some(Dialog::Form(form));
    }

    fn ask_delete(&mut self, path: ItemPath) {
        let guard = self.shared.config.lock().unwrap();
        let Some(el) = guard.as_ref().and_then(|c| c.item(&path)) else {
            return;
        };
        let dialog = Dialog::ConfirmDelete {
            name: config::display_name(el).to_string(),
            count: edit::count_entries(el),
            group: el.name == "files",
            path,
        };
        drop(guard);
        self.dialog = Some(dialog);
    }

    /// Ouvre le sélecteur de fichiers (hors verrou : la boîte de dialogue
    /// fait tourner sa propre boucle de messages).
    fn browse(&mut self, kind: Browse) {
        let base = self.base_dir();
        let start = paths::to_native(&base);
        match kind {
            Browse::AddFiles => {
                if let Some(files) = rfd::FileDialog::new()
                    .set_title("Ajouter des raccourcis")
                    .set_directory(&start)
                    .pick_files()
                {
                    self.apply(Action::AddFiles(files));
                }
            }
            Browse::EntryPath | Browse::WorkingDir => {
                let Some(Dialog::Form(form)) = &mut self.dialog else {
                    return;
                };
                let picked = if matches!(kind, Browse::EntryPath) {
                    rfd::FileDialog::new()
                        .set_title("Choisir le programme ou le document")
                        .set_directory(&start)
                        .pick_file()
                } else {
                    rfd::FileDialog::new()
                        .set_title("Choisir le dossier de travail")
                        .set_directory(&start)
                        .pick_folder()
                };
                let Some(picked) = picked else { return };
                let full = picked.to_string_lossy().into_owned();
                let rel = paths::make_relative(&full, &base);
                if matches!(kind, Browse::EntryPath) {
                    form.path = rel;
                    if form.name.trim().is_empty() {
                        if let Some(stem) = picked.file_stem() {
                            form.name = stem.to_string_lossy().into_owned();
                        }
                    }
                } else {
                    form.directory = rel;
                }
            }
        }
    }

    fn submit_form(&mut self) {
        let Some(Dialog::Form(form)) = &mut self.dialog else {
            return;
        };
        let name = form.name.trim().to_string();
        let path = form.path.trim().to_string();
        let directory = form.directory.trim().to_string();
        if name.is_empty() {
            form.error = Some("Le nom est obligatoire.".into());
            return;
        }
        if !form.is_group() && path.is_empty() {
            form.error = Some("Le chemin est obligatoire.".into());
            return;
        }
        let dir = (!directory.is_empty()).then_some(directory);
        let result = match &form.kind {
            FormKind::NewEntry { parent, index } => {
                let (parent, index) = (parent.clone(), *index);
                let item = NewItem::Entry {
                    name,
                    path: path.clone(),
                    directory: dir,
                };
                self.shared
                    .modify(|c| edit::insert(c, &parent, index, item))
            }
            FormKind::NewGroup { parent, index } => {
                let (parent, index) = (parent.clone(), *index);
                self.shared
                    .modify(|c| edit::insert(c, &parent, index, NewItem::Group { name }))
            }
            FormKind::EditEntry(p) => {
                let p = p.clone();
                self.shared.modify(|c| {
                    edit::update_entry(c, &p, &name, &path, dir.as_deref())?;
                    Ok(p.clone())
                })
            }
            FormKind::RenameGroup(p) => {
                let p = p.clone();
                self.shared.modify(|c| {
                    edit::rename(c, &p, &name)?;
                    Ok(p.clone())
                })
            }
        };
        if let Some(p) = result {
            let is_group = form.is_group();
            self.selected = Some(p);
            self.dialog = None;
            self.shared.set_status("Enregistré");
            if !is_group {
                let target = self.resolve_path(&path);
                self.refresh_target(target);
            }
        }
    }

    // ---- Dessin -------------------------------------------------------

    fn top_bar(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>, has_config: bool) {
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
                ui.add_enabled_ui(has_config, |ui| {
                    ui.menu_button("＋", |ui| {
                        if ui.button("Nouveau raccourci…").clicked() {
                            actions.push(Action::NewEntry(None));
                        }
                        if ui.button("Nouveau groupe…").clicked() {
                            actions.push(Action::NewGroup(None));
                        }
                        if ui.button("Ajouter des fichiers…").clicked() {
                            actions.push(Action::Browse(Browse::AddFiles));
                        }
                    })
                    .response
                    .on_hover_text("Ajouter (vous pouvez aussi déposer des fichiers ici)");
                });
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
                    self.search_results(ui, cfg, &rows, &query, &mut actions);
                    return;
                }
                match self.view {
                    View::Tree => {
                        if let Some(files) = cfg.files() {
                            let mut path = Vec::new();
                            rows.tree(ui, cfg, files, &mut path, &mut actions);
                            rows.end_drop_zone(ui, files.children.len(), &mut actions);
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

    fn search_results(
        &self,
        ui: &mut egui::Ui,
        cfg: &Config,
        rows: &Rows,
        query: &str,
        actions: &mut Vec<Action>,
    ) {
        let opts = cfg.search_options();
        let mut any = false;
        if opts.pstart {
            let hits: Vec<_> = cfg
                .flatten()
                .into_iter()
                .filter(|e| matches(e, query))
                .take(MAX_HITS)
                .collect();
            if !hits.is_empty() {
                any = true;
                section(ui, "PStart", tag_color(&opts, None, ui));
                for e in &hits {
                    rows.flat(ui, e, true, actions);
                }
            }
        }
        let start = self.shared.start_menu.lock().unwrap();
        for source in [Source::CurrentUser, Source::AllUsers] {
            let enabled = match source {
                Source::CurrentUser => opts.current_user,
                Source::AllUsers => opts.all_users,
            };
            if !enabled {
                continue;
            }
            let hits: Vec<&StartItem> = start
                .iter()
                .filter(|i| i.source == source && i.name.to_lowercase().contains(query))
                .take(MAX_HITS)
                .collect();
            if hits.is_empty() {
                continue;
            }
            any = true;
            section(ui, source.label(), tag_color(&opts, Some(source), ui));
            for item in hits {
                rows.start_item(ui, item, actions);
            }
        }
        if !any {
            ui.weak("Aucun résultat.");
        }
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
        if el.name == "files" {
            ui.heading(config::display_name(el));
            ui.weak(format!("{} raccourcis", edit::count_entries(el)));
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                if ui.button("＋ Raccourci").clicked() {
                    actions.push(Action::NewEntry(Some(sel.clone())));
                }
                if ui.button("＋ Groupe").clicked() {
                    actions.push(Action::NewGroup(Some(sel.clone())));
                }
                if ui.button("Renommer").clicked() {
                    actions.push(Action::Edit(sel.clone()));
                }
                if ui.button("Supprimer").clicked() {
                    actions.push(Action::AskDelete(sel.clone()));
                }
            });
            return actions;
        }
        let target = cfg.resolve(el);
        ui.horizontal(|ui| {
            if let Some(tex) = self.textures.get(&target) {
                ui.add(icon_image(tex, 32.0));
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
        ui.horizontal_wrapped(|ui| {
            if ui.button("▶ Lancer").clicked() {
                actions.push(Action::Launch(sel.clone()));
            }
            if ui.button("Modifier…").clicked() {
                actions.push(Action::Edit(sel.clone()));
            }
            if ui.button("Emplacement").clicked() {
                actions.push(Action::Reveal(sel.clone()));
            }
            if ui.button("Supprimer").clicked() {
                actions.push(Action::AskDelete(sel.clone()));
            }
        });
        actions
    }

    fn shortcuts_help(&self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Raccourcis").strong());
        ui.weak("Entrée : lancer");
        ui.weak("F2 : modifier, Suppr : supprimer");
        ui.weak("Ctrl+F : rechercher, Échap : effacer");
        ui.weak("Glisser-déposer : réorganiser");
        match &self.hotkey_info {
            Some(hk) => ui.weak(format!("{hk} : afficher / masquer")),
            None => ui.weak("Raccourci global : désactivé"),
        };
        ui.add_space(8.0);
        ui.weak(format!("Réglages : {}", settings::FILE_NAME));
    }

    fn dialog_ui(&mut self, ctx: &egui::Context, cfg: Option<&Config>, actions: &mut Vec<Action>) {
        let base = cfg.map(|c| c.base_dir.clone()).unwrap_or_default();
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        let modal = egui::Modal::new(egui::Id::unique("pstart-dialog")).show(ctx, |ui| {
            ui.set_width(420.0);
            match dialog {
                Dialog::Form(form) => form_ui(ui, form, &base, actions),
                Dialog::ConfirmDelete {
                    path,
                    name,
                    count,
                    group,
                } => {
                    ui.heading("Supprimer");
                    ui.add_space(8.0);
                    if *group {
                        ui.label(format!(
                            "Supprimer le groupe « {name} » et ses {count} raccourcis ?"
                        ));
                    } else {
                        ui.label(format!("Supprimer le raccourci « {name} » ?"));
                    }
                    ui.weak("Le programme lui-même n'est pas supprimé.");
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button("Supprimer").clicked() {
                            actions.push(Action::Delete(path.clone()));
                        }
                        if ui.button("Annuler").clicked() {
                            actions.push(Action::CloseDialog);
                        }
                    });
                }
            }
        });
        if modal.should_close() {
            actions.push(Action::CloseDialog);
        }
    }

    fn handle_input(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        // Fichiers déposés depuis l'Explorateur : un PStart.xml est ouvert,
        // le reste est ajouté comme raccourcis.
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if !dropped.is_empty() {
            let is_config = dropped.len() == 1
                && dropped[0]
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("xml"));
            if is_config {
                actions.push(Action::Open(dropped[0].clone()));
            } else if self.dialog.is_none() {
                actions.push(Action::AddFiles(dropped));
            }
        }
        if self.dialog.is_some() {
            return;
        }
        let typing = ctx.egui_wants_keyboard_input();
        let (ctrl_f, escape, enter, f2, delete) = ctx.input(|i| {
            (
                i.modifiers.command && i.key_pressed(egui::Key::F),
                i.key_pressed(egui::Key::Escape),
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::F2),
                i.key_pressed(egui::Key::Delete),
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
        if let Some(sel) = self.selected.clone() {
            if f2 {
                actions.push(Action::Edit(sel.clone()));
            }
            if delete && !typing {
                actions.push(Action::AskDelete(sel));
            }
        }
        if enter {
            self.enter_pressed(actions);
        }
    }

    /// Entrée : lance la sélection, ou le premier résultat de recherche.
    fn enter_pressed(&self, actions: &mut Vec<Action>) {
        let q = self.search.trim().to_lowercase();
        if q.is_empty() {
            if let Some(p) = self.selected.clone() {
                actions.push(Action::Launch(p));
            }
            return;
        }
        let guard = self.shared.config.lock().unwrap();
        if let Some(cfg) = guard.as_ref() {
            let opts = cfg.search_options();
            if opts.pstart {
                if let Some(e) = cfg.flatten().into_iter().find(|e| matches(e, &q)) {
                    actions.push(Action::Launch(e.path));
                    return;
                }
            }
            let start = self.shared.start_menu.lock().unwrap();
            let first = start.iter().find(|i| {
                let enabled = match i.source {
                    Source::CurrentUser => opts.current_user,
                    Source::AllUsers => opts.all_users,
                };
                enabled && i.name.to_lowercase().contains(&q)
            });
            if let Some(i) = first {
                actions.push(Action::LaunchExternal {
                    name: i.name.clone(),
                    path: i.path.clone(),
                });
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
                self.top_bar(ui, &mut actions, cfg.is_some());
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
            self.dialog_ui(&ctx, cfg, &mut actions);
        }

        for a in actions {
            self.apply(a);
        }
    }
}

fn form_ui(ui: &mut egui::Ui, form: &mut EditForm, base: &str, actions: &mut Vec<Action>) {
    ui.heading(form.title());
    ui.add_space(8.0);
    let mut submit = false;
    egui::Grid::new("form")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label("Nom");
            let name = ui.add(egui::TextEdit::singleline(&mut form.name).desired_width(300.0));
            if form.focus {
                name.request_focus();
                form.focus = false;
            }
            submit |= name.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.end_row();
            if !form.is_group() {
                ui.label("Chemin");
                ui.horizontal(|ui| {
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut form.path)
                            .desired_width(240.0)
                            .hint_text("_Dossier\\programme.exe"),
                    );
                    submit |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.button("Parcourir…").clicked() {
                        actions.push(Action::Browse(Browse::EntryPath));
                    }
                });
                ui.end_row();
                ui.label("");
                if !form.path.trim().is_empty() {
                    let resolved = paths::resolve(&form.path, base);
                    ui.add(egui::Label::new(egui::RichText::new(resolved).weak().small()).wrap());
                }
                ui.end_row();
                ui.label("Dossier");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut form.directory)
                            .desired_width(240.0)
                            .hint_text("(dossier du programme)"),
                    );
                    if ui.button("…").on_hover_text("Choisir le dossier").clicked() {
                        actions.push(Action::Browse(Browse::WorkingDir));
                    }
                });
                ui.end_row();
            }
        });
    if !form.is_group() {
        ui.weak("Les chemins sur le même lecteur sont enregistrés en relatif.");
    }
    if let Some(e) = &form.error {
        ui.colored_label(ui.visuals().error_fg_color, e);
    }
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        if ui.button("Enregistrer").clicked() {
            submit = true;
        }
        if ui.button("Annuler").clicked() {
            actions.push(Action::CloseDialog);
        }
    });
    if submit {
        actions.push(Action::SubmitForm);
    }
}

fn matches(e: &config::FlatEntry, query: &str) -> bool {
    e.name.to_lowercase().contains(query) || e.target.to_lowercase().contains(query)
}

fn tag_color(opts: &SearchOptions, source: Option<Source>, ui: &egui::Ui) -> egui::Color32 {
    let rgb = match source {
        None => opts.color_pstart,
        Some(Source::CurrentUser) => opts.color_current_user,
        Some(Source::AllUsers) => opts.color_all_users,
    };
    match rgb {
        Some([r, g, b]) if opts.colorize => egui::Color32::from_rgb(r, g, b),
        _ => ui.visuals().strong_text_color(),
    }
}

fn section(ui: &mut egui::Ui, title: &str, color: egui::Color32) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(title).small().strong().color(color));
}

fn icon_image(tex: &egui::TextureHandle, size: f32) -> egui::Image<'static> {
    egui::Image::from_texture(egui::load::SizedTexture::from_handle(tex))
        .fit_to_exact_size(egui::vec2(size, size))
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
                    let count = edit::count_entries(child);
                    let title = format!("{}  ({count})", config::display_name(child));
                    let resp = egui::CollapsingHeader::new(title)
                        .id_salt(path.clone())
                        .default_open(false)
                        .show(ui, |ui| {
                            self.tree(ui, cfg, child, path, actions);
                        });
                    let header = resp.header_response.interact(egui::Sense::drag());
                    self.group_header(ui, &header, path, actions);
                }
                "file" => {
                    let target = cfg.resolve(child);
                    let resp = self.row(
                        ui,
                        config::display_name(child),
                        &target,
                        None,
                        path,
                        true,
                        actions,
                    );
                    self.drop_target(ui, &resp, path, actions);
                }
                _ => {}
            }
            path.pop();
        }
    }

    fn group_header(
        &self,
        ui: &egui::Ui,
        header: &egui::Response,
        path: &[usize],
        actions: &mut Vec<Action>,
    ) {
        header.dnd_set_drag_payload(DragItem(path.to_vec()));
        if header.clicked() {
            actions.push(Action::Select(path.to_vec()));
        }
        header.context_menu(|ui| {
            if ui.button("Nouveau raccourci ici…").clicked() {
                actions.push(Action::NewEntry(Some(path.to_vec())));
                ui.close();
            }
            if ui.button("Nouveau groupe ici…").clicked() {
                actions.push(Action::NewGroup(Some(path.to_vec())));
                ui.close();
            }
            ui.separator();
            if ui.button("Renommer…").clicked() {
                actions.push(Action::Edit(path.to_vec()));
                ui.close();
            }
            if ui.button("Supprimer…").clicked() {
                actions.push(Action::AskDelete(path.to_vec()));
                ui.close();
            }
        });
        // Dépôt sur un groupe : en haut de l'en-tête = avant le groupe,
        // ailleurs = à la fin du groupe.
        let rect = header.rect;
        let before = ui
            .ctx()
            .pointer_interact_pos()
            .is_some_and(|p| p.y < rect.top() + rect.height() * 0.3);
        if header.dnd_hover_payload::<DragItem>().is_some() {
            let stroke = ui.visuals().selection.stroke;
            if before {
                ui.painter().hline(rect.x_range(), rect.top(), stroke);
            } else {
                ui.painter()
                    .rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside);
            }
        }
        if let Some(item) = header.dnd_release_payload::<DragItem>() {
            let (idx, parent) = path.split_last().expect("chemin non vide");
            let action = if before {
                Action::Move {
                    from: item.0.clone(),
                    to_parent: parent.to_vec(),
                    to_index: *idx,
                }
            } else {
                Action::Move {
                    from: item.0.clone(),
                    to_parent: path.to_vec(),
                    to_index: usize::MAX,
                }
            };
            actions.push(action);
        }
    }

    /// Dépôt sur un raccourci : avant ou après lui selon la position.
    fn drop_target(
        &self,
        ui: &egui::Ui,
        resp: &egui::Response,
        path: &[usize],
        actions: &mut Vec<Action>,
    ) {
        let rect = resp.rect;
        let after = ui
            .ctx()
            .pointer_interact_pos()
            .is_some_and(|p| p.y > rect.center().y);
        if resp.dnd_hover_payload::<DragItem>().is_some() {
            let y = if after { rect.bottom() } else { rect.top() };
            ui.painter()
                .hline(rect.x_range(), y, ui.visuals().selection.stroke);
        }
        if let Some(item) = resp.dnd_release_payload::<DragItem>() {
            let (idx, parent) = path.split_last().expect("chemin non vide");
            actions.push(Action::Move {
                from: item.0.clone(),
                to_parent: parent.to_vec(),
                to_index: idx + usize::from(after),
            });
        }
    }

    /// Zone sous l'arbre : dépôt à la fin de la racine.
    fn end_drop_zone(&self, ui: &mut egui::Ui, root_len: usize, actions: &mut Vec<Action>) {
        let height = ui.available_height().max(32.0);
        let (rect, resp) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), height),
            egui::Sense::hover(),
        );
        if resp.dnd_hover_payload::<DragItem>().is_some() {
            ui.painter().hline(
                rect.x_range(),
                rect.top() + 1.0,
                ui.visuals().selection.stroke,
            );
        }
        if let Some(item) = resp.dnd_release_payload::<DragItem>() {
            actions.push(Action::Move {
                from: item.0.clone(),
                to_parent: Vec::new(),
                to_index: root_len,
            });
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
        self.row(ui, &e.name, &e.target, Some(&info), &e.path, false, actions);
    }

    fn icon(&self, ui: &mut egui::Ui, target: &str) {
        match self.textures.get(target) {
            Some(tex) => {
                ui.add(icon_image(tex, ICON_SIZE));
            }
            None if !self.textures.is_empty() => {
                ui.add_space(ICON_SIZE + ui.spacing().item_spacing.x);
            }
            None => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn row(
        &self,
        ui: &mut egui::Ui,
        name: &str,
        target: &str,
        info: Option<&str>,
        path: &[usize],
        draggable: bool,
        actions: &mut Vec<Action>,
    ) -> egui::Response {
        let missing = self.exists.get(target) == Some(&false);
        let is_selected = self.selected.as_deref() == Some(path);
        let mut text = egui::RichText::new(name);
        if missing {
            text = text.color(ui.visuals().weak_text_color()).strikethrough();
        }
        ui.horizontal(|ui| {
            self.icon(ui, target);
            let mut resp = ui.selectable_label(is_selected, text);
            if draggable {
                resp = resp.interact(egui::Sense::drag());
                resp.dnd_set_drag_payload(DragItem(path.to_vec()));
            }
            if missing {
                resp = resp.on_hover_text("Cible introuvable");
            }
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
                ui.separator();
                if ui.button("Modifier…").clicked() {
                    actions.push(Action::Edit(path.to_vec()));
                    ui.close();
                }
                if ui.button("Supprimer…").clicked() {
                    actions.push(Action::AskDelete(path.to_vec()));
                    ui.close();
                }
            });
            resp
        })
        .inner
    }

    fn start_item(&self, ui: &mut egui::Ui, item: &StartItem, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            self.icon(ui, &item.path);
            let resp = ui.selectable_label(false, &item.name);
            let launch = || Action::LaunchExternal {
                name: item.name.clone(),
                path: item.path.clone(),
            };
            if resp.double_clicked() || (self.single && resp.clicked()) {
                actions.push(launch());
            }
            resp.context_menu(|ui| {
                if ui.button("Lancer").clicked() {
                    actions.push(launch());
                    ui.close();
                }
                if ui.button("Ajouter à PStart").clicked() {
                    actions.push(Action::AddExternal {
                        name: item.name.clone(),
                        path: item.path.clone(),
                    });
                    ui.close();
                }
                if ui.button("Ouvrir l'emplacement").clicked() {
                    actions.push(Action::RevealPath(item.path.clone()));
                    ui.close();
                }
            });
        });
    }
}
