//! Interface egui : arbre des raccourcis, recherche, édition et statistiques.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::alarms::ReminderStyle;
use crate::config::{self, Config, EntryOptions, ItemPath, SearchOptions, WindowState};
use crate::edit::{self, NewItem};
use crate::hotcorner::Corner;
use crate::i18n::{self, Language};
use crate::i18n::{tr, trf};
use crate::icons::{IconColor, IconShape};
use crate::notes::{Note, Notes, Repeat};
use crate::settings::{self, Settings, Theme};
use crate::startmenu::{self, Source, StartItem};
use crate::state::Shared;
use crate::xml::Element;
use crate::{
    alarms, autostart, backup, eject, fuzzy, hotcorner, hotkey, icons, launcher, notes, paths,
    scan, tray, update, window,
};

mod actions;
mod dialogs;
mod input;
mod loading;
mod stats;
mod tree;
mod views;
mod widgets;

use dialogs::*;
use loading::apply_appearance;
use tree::*;
use views::*;

const ICON_SIZE: f32 = 18.0;
const MAX_HITS: usize = 60;

#[derive(PartialEq, Clone, Copy)]
enum View {
    Tree,
    MostUsed,
    Recent,
    Notes,
    Stats,
}

impl View {
    const ALL: [View; 5] = [
        View::Tree,
        View::MostUsed,
        View::Recent,
        View::Notes,
        View::Stats,
    ];

    fn label(self) -> &'static str {
        match self {
            View::Tree => tr("Arborescence"),
            View::MostUsed => tr("Plus utilisés"),
            View::Recent => tr("Récents"),
            View::Notes => tr("Notes"),
            View::Stats => tr("Statistiques"),
        }
    }
}

/// Largeur de la fenêtre en mode compact.
pub const COMPACT_WIDTH: f32 = 340.0;

/// Favori déplacé dans la barre (position).
#[derive(Clone, Copy)]
struct FavDrag(usize);

/// Charge utile d'un glisser-déposer interne.
#[derive(Clone)]
struct DragItem(ItemPath);

/// Ce que le sélecteur de fichiers doit remplir.
#[derive(Clone, Copy)]
enum Browse {
    EntryPath,
    WorkingDir,
    AddFiles,
    IconFile,
    BackupExport,
    BackupRestore,
    NewConfig,
    OpenConfig,
    ScanFolder,
    GroupIcon,
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
    OpenSettings,
    SaveSettings,
    NotesChanged,
    ToggleFavorite(ItemPath),
    LaunchFavorite(usize),
    ToggleGroup(String),
    ToggleCompact,
    OpenDriveFix,
    FixDrive {
        old: char,
        new: char,
    },
    AskCleanup(Vec<ItemPath>),
    Cleanup(Vec<ItemPath>),
    Restore(PathBuf),
    CheckUpdate,
    InstallUpdate,
    Undo,
    Redo,
    LaunchGroup {
        path: ItemPath,
        confirmed: bool,
    },
    NewSeparator(Option<ItemPath>),
    NewLink(Option<ItemPath>),
    AskEject,
    Eject {
        force: bool,
    },
    ShowFound,
    Import {
        items: Vec<scan::Found>,
        by_category: bool,
        group: String,
    },
    IgnoreFound(Vec<String>),
    MakePortable,
    OpenLog,
    MoveFavorite {
        from: usize,
        to: usize,
    },
    /// Ctrl+clic (ajoute/retire) ou Maj+clic (plage).
    SelectMulti {
        path: ItemPath,
        range: bool,
    },
    ClearMulti,
    LaunchNew(ItemPath),
    Duplicate(ItemPath),
    LaunchMany(Vec<ItemPath>),
    PinMany(Vec<ItemPath>, bool),
    MoveMany {
        paths: Vec<ItemPath>,
        target: ItemPath,
    },
    AskDeleteMany(Vec<ItemPath>),
    Rollback,
    CreateConfig(PathBuf),
}

/// Éjection en cours (fermeture des programmes en tâche de fond).
#[derive(Default)]
enum EjectState {
    #[default]
    Idle,
    Waiting,
    Remaining(Vec<eject::Proc>),
    Ready,
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
    options: EntryOptions,
    /// Icône d'un groupe.
    icon: String,
    /// Groupes où créer l'élément (chemin, libellé « A › B »).
    groups: Vec<(ItemPath, String)>,
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
            FormKind::NewEntry { .. } => tr("Nouveau raccourci"),
            FormKind::EditEntry(_) => tr("Modifier le raccourci"),
            FormKind::NewGroup { .. } => tr("Nouveau groupe"),
            FormKind::RenameGroup(_) => tr("Renommer le groupe"),
        }
    }
}

/// Ce dont dépend l'aperçu de l'icône.
type IconKey = (IconShape, IconColor, Option<String>);

struct SettingsForm {
    settings: Settings,
    autostart: bool,
    settings_path: Option<PathBuf>,
    preview: Option<(IconKey, egui::TextureHandle)>,
}

enum Dialog {
    Form(EditForm),
    Settings(SettingsForm),
    ConfirmDelete {
        path: ItemPath,
        name: String,
        count: usize,
        group: bool,
    },
    /// Changement de lettre de lecteur (détecté ou demandé).
    DriveFix {
        old: String,
        new: String,
    },
    ConfirmCleanup(Vec<ItemPath>),
    ConfirmDeleteMany(Vec<ItemPath>),
    ConfirmRestore {
        file: PathBuf,
        contents: Vec<String>,
    },
    ConfirmLaunchGroup {
        path: ItemPath,
        name: String,
        count: usize,
    },
    Eject {
        drive: char,
        procs: Vec<eject::Proc>,
        /// La fermeture propre n'a pas suffi.
        stubborn: bool,
    },
    Import {
        items: Vec<(bool, scan::Found)>,
        by_category: bool,
        group: String,
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
    hotkeys: Option<hotkey::Hotkeys>,
    settings_path: Option<PathBuf>,
    /// Notes modifiées, à enregistrer après un court délai.
    notes_dirty: Option<Instant>,
    search_id: Option<egui::Id>,
    /// Lignes visibles à la dernière image (navigation au clavier).
    visible: RefCell<Vec<ItemPath>>,
    /// Groupes ouverts, désignés par la suite de leurs noms.
    open_groups: HashSet<String>,
    /// Faire défiler jusqu'à la sélection à la prochaine image.
    scroll_to_selected: bool,
    /// Largeur de la fenêtre avant le passage en mode compact.
    wide_width: f32,
    eject_state: Arc<std::sync::Mutex<EjectState>>,
    /// Nouveaux programmes détectés au démarrage.
    found: Vec<scan::Found>,
    /// Résultat d'une recherche en cours, et si elle est silencieuse.
    found_rx: Option<(Receiver<Vec<scan::Found>>, bool)>,
    /// Fichier demandé au lancement (argument), même s'il n'existe pas.
    requested_file: Option<PathBuf>,
    /// Sélection multiple (Ctrl+clic, Maj+clic).
    multi: Vec<ItemPath>,
}

impl PStartApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        file: Option<PathBuf>,
        settings: Settings,
    ) -> Self {
        let shared = Arc::new(Shared::default());
        let _ = shared.ctx.set(cc.egui_ctx.clone());
        window::capture(cc);

        let settings_path = file.as_deref().map(settings::path_for);
        i18n::set_language(settings.language);
        apply_appearance(&cc.egui_ctx, &settings);
        *shared.hot_corner.lock().unwrap() = (
            settings.hot_corner,
            settings.hot_corner_delay,
            settings.hot_corner_size,
            settings.hot_corner_all_screens,
        );
        *shared.reminders.lock().unwrap() = settings.reminders;
        shared
            .reuse_running
            .store(settings.reuse_running, Ordering::SeqCst);
        shared
            .track_usage
            .store(settings.track_usage, Ordering::SeqCst);

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
            hotkeys: None,
            settings_path,
            notes_dirty: None,
            search_id: None,
            visible: RefCell::new(Vec::new()),
            open_groups: HashSet::new(),
            scroll_to_selected: false,
            wide_width: 620.0,
            eject_state: Arc::default(),
            found: Vec::new(),
            found_rx: None,
            requested_file: file.clone(),
            multi: Vec::new(),
        };
        if let Some(f) = file.filter(|f| f.exists()) {
            app.open(&f);
            // Pas de relance des programmes « au démarrage » après une mise à jour.
            if std::env::var_os(update::RESTART_ENV).is_none() {
                app.run_autorun();
            }
        }
        if app.settings.check_updates {
            app.check_update(true);
        }
        app.scan_start_menu();
        app.init_desktop_integration();
        app.watch_file();
        if app.settings.scan_on_start {
            if let Some(dir) = app.scan_dir() {
                app.start_scan(dir, true);
            }
        }
        alarms::spawn(app.shared.clone());
        hotcorner::spawn(app.shared.clone());
        crate::monitor::spawn(app.shared.clone());
        app
    }

    /// Icône de notification et raccourci global (Windows uniquement).
    fn init_desktop_integration(&mut self) {
        if !cfg!(windows) {
            return;
        }
        let mut problems = Vec::new();
        if self.settings.tray {
            let icon = self.settings.icon(self.settings_path.as_deref());
            match tray::Tray::new(&self.shared, &icon) {
                Ok(t) => self.tray = Some(t),
                Err(e) => problems.push(e),
            }
        }
        match hotkey::init(self.settings.hotkey.as_deref(), self.shared.clone()) {
            Ok((hk, problem)) => {
                match problem {
                    Some(p) => problems.push(p),
                    None => self.hotkey_info = self.settings.hotkey.clone(),
                }
                self.hotkeys = Some(hk);
            }
            Err(e) => problems.push(e),
        }
        if !problems.is_empty() {
            self.shared.error(problems.join(" ; "));
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
        if self.shared.show_notes.swap(false, Ordering::SeqCst) {
            self.view = View::Notes;
            self.search.clear();
        }
        if let Some(t) = self.notes_dirty {
            if t.elapsed() >= Duration::from_millis(800) {
                self.flush_notes();
            } else {
                ctx.request_repaint_after(Duration::from_millis(900));
            }
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
        if let Some(h) = &mut self.hotkeys {
            let problems = h.sync(&self.shared);
            if !problems.is_empty() {
                self.shared.error(problems.join(" ; "));
            }
        }
        self.poll_scan();
        self.poll_eject();
        if self.shared.ask_eject.swap(false, Ordering::SeqCst) {
            self.ask_eject();
        }
        // PStart.xml modifié ailleurs : rechargement (pas pendant une saisie).
        let editing = matches!(
            self.dialog,
            Some(Dialog::Form(_)) | Some(Dialog::Settings(_))
        );
        if !editing && self.shared.file_changed.swap(false, Ordering::SeqCst) {
            crate::log::info("PStart.xml modifié par un autre programme : rechargement");
            let selected = self.selected.clone();
            self.reload();
            self.selected = selected;
            self.shared
                .set_status(tr("PStart.xml modifié par un autre programme : rechargé"));
        }
        // Mise à jour installée : relance puis fermeture.
        if *self.shared.update.lock().unwrap() == update::State::Installed {
            *self.shared.update.lock().unwrap() = update::State::Idle;
            self.flush_notes();
            match update::relaunch() {
                Ok(()) => tray::quit(&self.shared),
                Err(e) => self.shared.error(e),
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
                self.top_bar(ui, &mut actions, cfg);
                ui.add_space(2.0);
            });
            egui::Panel::bottom("status").show(ui, |ui| {
                ui.horizontal(|ui| {
                    if !self.found.is_empty()
                        && ui
                            .small_button(trf("＋ {} nouveaux programmes", &[&self.found.len()]))
                            .clicked()
                    {
                        actions.push(Action::ShowFound);
                    }
                    if let update::State::Available(r) = &*shared.update.lock().unwrap() {
                        if ui
                            .small_button(format!("⬆ {}", r.version))
                            .on_hover_text(tr("Nouvelle version : ouvrir les réglages"))
                            .clicked()
                        {
                            actions.push(Action::OpenSettings);
                        }
                    }
                    ui.label(&status);
                    if let Some(cfg) = cfg {
                        let file = cfg.file.display().to_string();
                        ui.separator();
                        ui.add(egui::Label::new(egui::RichText::new(&file).weak()).truncate())
                            .on_hover_text(&file);
                    }
                });
            });
            if !self.settings.compact {
                egui::Panel::right("details")
                    .resizable(true)
                    .default_size(250.0)
                    .min_size(200.0)
                    .show(ui, |ui| {
                        ui.add_space(6.0);
                        actions.extend(self.details(ui, cfg));
                    });
            }
            egui::CentralPanel::default().show(ui, |ui| {
                actions.extend(self.central(ui, cfg));
            });
            self.scroll_to_selected = false;
            self.dialog_ui(&ctx, cfg, &mut actions);
        }

        for a in actions {
            self.apply(a);
        }
    }
}
