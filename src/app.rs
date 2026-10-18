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

mod stats;

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
        };
        if let Some(f) = file {
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

    /// Surveille PStart.xml : une modification par un autre programme
    /// (synchronisation, autre PC, PStart) déclenche un rechargement.
    fn watch_file(&self) {
        let shared = self.shared.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(2));
            if shared.quitting.load(Ordering::SeqCst) {
                return;
            }
            let changed = shared
                .config
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(Config::changed_on_disk);
            if changed && !shared.file_changed.swap(true, Ordering::SeqCst) {
                shared.repaint();
            }
        });
    }

    /// Dossier de recherche de programmes (réglage, relatif au fichier).
    fn scan_dir(&self) -> Option<PathBuf> {
        let raw = self.settings.scan_folder.as_deref()?;
        let resolved = paths::resolve(raw, &self.base_dir());
        Some(paths::to_native(&resolved))
    }

    /// Cherche les programmes absents de PStart.xml dans `dir`.
    /// `quiet` : au démarrage, ne rien afficher s'il n'y a rien.
    fn start_scan(&mut self, dir: PathBuf, quiet: bool) {
        let (known, ignored) = {
            let guard = self.shared.config.lock().unwrap();
            let Some(cfg) = guard.as_ref() else { return };
            let known: HashSet<String> = cfg
                .flatten()
                .into_iter()
                .map(|e| paths::to_native(&e.target).to_string_lossy().to_lowercase())
                .collect();
            (known, self.settings.scan_ignored.clone())
        };
        let (tx, rx) = mpsc::channel();
        self.found_rx = Some((rx, quiet));
        if !quiet {
            self.shared.set_status(tr("Recherche de programmes…"));
        }
        let shared = self.shared.clone();
        std::thread::spawn(move || {
            let mut found = scan::scan(&dir, &known);
            found.retain(|f| !ignored.iter().any(|i| i.eq_ignore_ascii_case(&f.path)));
            if found.is_empty() && !quiet {
                shared.set_status(tr("Aucun nouveau programme trouvé"));
            }
            let _ = tx.send(found);
            shared.repaint();
        });
    }

    fn poll_scan(&mut self) {
        let Some((rx, quiet)) = &self.found_rx else {
            return;
        };
        let quiet = *quiet;
        let Ok(found) = rx.try_recv() else { return };
        self.found_rx = None;
        if found.is_empty() {
            return;
        }
        self.shared
            .set_status(trf("{} nouveaux programmes détectés", &[&found.len()]));
        self.found = found;
        // Recherche demandée : affichage immédiat. Au démarrage, seul le
        // bouton de la barre d'état le signale.
        if !quiet && (self.dialog.is_none() || matches!(self.dialog, Some(Dialog::Settings(_)))) {
            self.apply(Action::ShowFound);
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
                    .set_status(trf("{} raccourcis chargés", &[&targets.len()]));
                self.exists.clear();
                self.check_existence(targets.clone());
                targets.extend(group_icons(&cfg));
                self.shared.clear_history();
                self.shared.icons.lock().unwrap().clear();
                self.textures.clear();
                self.flush_notes();
                let notes = Notes::load(&notes::path_for(&cfg.file)).unwrap_or_else(|e| {
                    self.shared.set_status(trf("Notes illisibles : {}", &[&e]));
                    Notes {
                        file: notes::path_for(&cfg.file),
                        items: Vec::new(),
                    }
                });
                *self.shared.notes.lock().unwrap() = Some(notes);
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
                self.check_drive();
            }
            Err(e) => self.load_error = Some(e),
        }
    }

    /// Lance les raccourcis marqués « au démarrage » (`<autorun>`).
    fn run_autorun(&self) {
        let paths: Vec<ItemPath> = {
            let guard = self.shared.config.lock().unwrap();
            let Some(cfg) = guard.as_ref() else { return };
            cfg.flatten()
                .into_iter()
                .filter(|e| e.autorun)
                .map(|e| e.path)
                .collect()
        };
        for p in paths {
            self.shared.launch(&p);
        }
    }

    /// La clé USB a-t-elle changé de lettre depuis le dernier lancement ?
    fn check_drive(&mut self) {
        let (current, refs) = {
            let guard = self.shared.config.lock().unwrap();
            let Some(cfg) = guard.as_ref() else { return };
            let Some(current) = paths::drive_letter(&cfg.file.to_string_lossy()) else {
                return;
            };
            let refs = self
                .settings
                .last_drive
                .filter(|old| *old != current)
                .map(|old| (old, edit::count_drive_refs(cfg, old)));
            (current, refs)
        };
        if let Some((old, count)) = refs {
            if count > 0 {
                self.dialog = Some(Dialog::DriveFix {
                    old: old.to_string(),
                    new: current.to_string(),
                });
            }
            // Le démarrage avec Windows pointe encore vers l'ancienne lettre.
            if autostart::is_enabled() {
                self.update_autostart();
            }
        }
        if self.settings.last_drive != Some(current) {
            self.settings.last_drive = Some(current);
            self.persist_settings();
        }
    }

    fn update_autostart(&self) {
        let file = self.config_file();
        if let Err(e) = autostart::set(true, file.as_deref()) {
            self.shared.error(trf("Démarrage avec Windows : {}", &[&e]));
        }
    }

    fn config_file(&self) -> Option<PathBuf> {
        self.shared
            .config
            .lock()
            .unwrap()
            .as_ref()
            .map(|c| c.file.clone())
    }

    /// Enregistre les réglages courants (hors fenêtre de réglages).
    fn persist_settings(&self) {
        if let Some(path) = &self.settings_path {
            if let Err(e) = self.settings.save(path) {
                self.shared.error(e);
            }
        }
    }

    /// Vérifie en tâche de fond si une nouvelle version est publiée.
    fn check_update(&self, quiet: bool) {
        let shared = self.shared.clone();
        *shared.update.lock().unwrap() = update::State::Checking;
        std::thread::spawn(move || {
            if quiet {
                // Pas de concurrence avec le démarrage.
                std::thread::sleep(Duration::from_secs(5));
            }
            let state = match update::check() {
                Ok(Some(r)) => {
                    shared.set_status(trf("Nouvelle version disponible : {}", &[&r.version]));
                    update::State::Available(r)
                }
                Ok(None) => update::State::UpToDate,
                // Au démarrage, une absence de réseau n'est pas une erreur.
                Err(_) if quiet => update::State::Idle,
                Err(e) => update::State::Failed(e),
            };
            *shared.update.lock().unwrap() = state;
            shared.repaint();
        });
    }

    fn install_update(&self) {
        let release = match &*self.shared.update.lock().unwrap() {
            update::State::Available(r) => r.clone(),
            _ => return,
        };
        let shared = self.shared.clone();
        *shared.update.lock().unwrap() = update::State::Downloading;
        shared.set_status(trf("Téléchargement de la version {}…", &[&release.version]));
        std::thread::spawn(move || {
            let state = match update::install(&release) {
                Ok(()) => update::State::Installed,
                Err(e) => {
                    shared.error(e.clone());
                    update::State::Failed(e)
                }
            };
            *shared.update.lock().unwrap() = state;
            shared.repaint();
        });
    }

    /// Bascule entre la fenêtre complète et le mode compact.
    fn toggle_compact(&mut self, ctx: &egui::Context) {
        self.settings.compact = !self.settings.compact;
        self.persist_settings();
        let size = ctx.input(|i| i.viewport().inner_rect.map(|r| r.size()));
        let height = size.map_or(700.0, |s| s.y);
        let width = if self.settings.compact {
            if let Some(s) = size {
                self.wide_width = s.x.max(COMPACT_WIDTH + 100.0);
            }
            COMPACT_WIDTH
        } else {
            self.wide_width
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(width, height)));
    }

    /// Raccourcis épinglés, dans l'ordre du fichier.
    fn favorites(cfg: &Config) -> Vec<config::FlatEntry> {
        cfg.flatten().into_iter().filter(|e| e.favorite).collect()
    }

    /// Enregistre les notes si elles ont été modifiées.
    fn flush_notes(&mut self) {
        if self.notes_dirty.take().is_none() {
            return;
        }
        let guard = self.shared.notes.lock().unwrap();
        if let Some(notes) = guard.as_ref() {
            if let Err(e) = notes.save() {
                drop(guard);
                self.shared.error(e);
            }
        }
    }

    fn save_settings(&mut self) {
        let Some(Dialog::Settings(form)) = self.dialog.take() else {
            return;
        };
        let new = form.settings;
        let mut messages = Vec::new();
        if let Some(path) = &self.settings_path {
            if let Err(e) = new.save(path) {
                messages.push(e);
            }
        }
        *self.shared.hot_corner.lock().unwrap() = (
            new.hot_corner,
            new.hot_corner_delay,
            new.hot_corner_size,
            new.hot_corner_all_screens,
        );
        *self.shared.reminders.lock().unwrap() = new.reminders;
        if form.autostart != autostart::is_enabled() {
            let file = self
                .shared
                .config
                .lock()
                .unwrap()
                .as_ref()
                .map(|c| c.file.clone());
            if let Err(e) = autostart::set(form.autostart, file.as_deref()) {
                messages.push(trf("Démarrage avec Windows : {}", &[&e]));
            }
            self.shared.structure_version.fetch_add(1, Ordering::SeqCst);
        }
        let restart = new.hotkey != self.settings.hotkey || new.tray != self.settings.tray;
        let old = std::mem::replace(&mut self.settings, new);
        let ctx = self.shared.ctx.get().cloned();
        if old.language != self.settings.language {
            i18n::set_language(self.settings.language);
            if let Some(ctx) = &ctx {
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(window::title().into()));
            }
            // Menu de la zone de notification à reconstruire.
            self.shared.structure_version.fetch_add(1, Ordering::SeqCst);
        }
        if let Some(ctx) = &ctx {
            apply_appearance(ctx, &self.settings);
        }
        if old.compact != self.settings.compact {
            // La bascule redimensionne la fenêtre et enregistre le réglage.
            self.settings.compact = old.compact;
            if let Some(ctx) = &ctx {
                self.toggle_compact(ctx);
            }
        }
        let icon_key = |s: &Settings| (s.icon_shape, s.icon_color, s.icon_file.clone());
        if icon_key(&old) != icon_key(&self.settings) {
            if let Some(e) = self.apply_icon() {
                messages.push(e);
            }
        }
        if messages.is_empty() {
            messages.push(if restart {
                tr("Réglages enregistrés (raccourci global et zone de notification : au prochain démarrage)").into()
            } else {
                tr("Réglages enregistrés").into()
            });
        }
        self.shared.set_status(messages.join(" ; "));
    }

    /// Applique l'icône des réglages à la fenêtre et à la zone de
    /// notification ; renvoie un message si le fichier choisi est illisible.
    fn apply_icon(&mut self) -> Option<String> {
        let path = self.settings.icon_path(self.settings_path.as_deref());
        let img = self.settings.icon(self.settings_path.as_deref());
        if let Some(ctx) = self.shared.ctx.get() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Icon(Some(Arc::new(
                egui::IconData {
                    rgba: img.rgba.clone(),
                    width: img.width,
                    height: img.height,
                },
            ))));
        }
        let mut problem = None;
        if let Some(t) = &self.tray {
            if let Err(e) = t.set_icon(&img) {
                problem = Some(trf("Icône : {}", &[&e]));
            }
        }
        if path.is_some_and(|p| !paths::to_native(&p).is_file()) {
            problem = Some(trf("Icône : {}", &[&tr("fichier introuvable")]));
        }
        problem
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
                let ok = paths::is_url(&t) || paths::to_native(&t).exists();
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
                    self.shared.error(e);
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
                    self.shared.set_status(tr("Supprimé"));
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
            Action::OpenSettings => {
                self.dialog = Some(Dialog::Settings(SettingsForm {
                    settings: self.settings.clone(),
                    autostart: autostart::is_enabled(),
                    settings_path: self.settings_path.clone(),
                    preview: None,
                }))
            }
            Action::SaveSettings => self.save_settings(),
            Action::NotesChanged => self.notes_dirty = Some(Instant::now()),
            Action::ToggleFavorite(p) => {
                let pinned = self.shared.modify(|c| {
                    let el = c.item(&p).ok_or(tr("élément introuvable"))?;
                    let on = !EntryOptions::of(el).favorite;
                    edit::set_favorite(c, &p, on)?;
                    Ok(on)
                });
                if let Some(on) = pinned {
                    self.shared.set_status(if on {
                        tr("Ajouté aux favoris")
                    } else {
                        tr("Retiré des favoris")
                    });
                }
            }
            Action::LaunchFavorite(n) => {
                let path = {
                    let guard = self.shared.config.lock().unwrap();
                    guard
                        .as_ref()
                        .and_then(|c| Self::favorites(c).into_iter().nth(n))
                        .map(|e| e.path)
                };
                if let Some(p) = path {
                    self.apply(Action::Launch(p));
                }
            }
            Action::ToggleGroup(key) => {
                if !self.open_groups.remove(&key) {
                    self.open_groups.insert(key);
                }
            }
            Action::ToggleCompact => {
                if let Some(ctx) = self.shared.ctx.get().cloned() {
                    self.toggle_compact(&ctx);
                }
            }
            Action::OpenDriveFix => {
                let current = self
                    .config_file()
                    .and_then(|f| paths::drive_letter(&f.to_string_lossy()));
                self.dialog = Some(Dialog::DriveFix {
                    old: String::new(),
                    new: current.map(String::from).unwrap_or_default(),
                });
            }
            Action::FixDrive { old, new } => {
                self.dialog = None;
                if let Some(n) = self
                    .shared
                    .modify(|c| Ok(edit::retarget_drive(c, old, new)))
                {
                    if let Some(icon) = &self.settings.icon_file {
                        if let Some(fixed) = paths::replace_drive(icon, old, new) {
                            self.settings.icon_file = Some(fixed);
                            self.persist_settings();
                        }
                    }
                    if autostart::is_enabled() {
                        self.update_autostart();
                    }
                    self.reload();
                    self.shared
                        .set_status(trf("{} chemins corrigés ({}: → {}:)", &[&n, &old, &new]));
                }
            }
            Action::AskCleanup(paths) => {
                if !paths.is_empty() {
                    self.dialog = Some(Dialog::ConfirmCleanup(paths));
                }
            }
            Action::Cleanup(paths) => {
                self.dialog = None;
                if let Some(n) = self.shared.modify(|c| edit::delete_many(c, paths)) {
                    self.selected = None;
                    self.shared
                        .set_status(trf("{} raccourcis supprimés", &[&n]));
                }
            }
            Action::Restore(file) => {
                self.dialog = None;
                self.restore(&file);
            }
            Action::CheckUpdate => self.check_update(false),
            Action::InstallUpdate => self.install_update(),
            Action::Undo => self.undo_redo(false),
            Action::Redo => self.undo_redo(true),
            Action::LaunchGroup { path, confirmed } => {
                let (entries, name) = {
                    let guard = self.shared.config.lock().unwrap();
                    let Some(cfg) = guard.as_ref() else { return };
                    let name = cfg
                        .item(&path)
                        .map(|e| config::display_name(e).to_string())
                        .unwrap_or_default();
                    (edit::group_entries(cfg, &path), name)
                };
                if entries.is_empty() {
                    self.shared
                        .set_status(tr("Ce groupe ne contient aucun raccourci"));
                } else if entries.len() > 8 && !confirmed {
                    self.dialog = Some(Dialog::ConfirmLaunchGroup {
                        path,
                        name,
                        count: entries.len(),
                    });
                } else {
                    self.dialog = None;
                    for p in &entries {
                        self.shared.launch(p);
                    }
                    self.shared
                        .set_status(trf("{} programmes lancés ({})", &[&entries.len(), &name]));
                }
            }
            Action::NewSeparator(at) => {
                let selected = at.or_else(|| self.selected.clone());
                self.shared.modify(|c| {
                    let (parent, index) = edit::insertion_point(c, selected.as_deref());
                    edit::insert(c, &parent, index, NewItem::Separator)
                });
            }
            Action::NewLink(at) => {
                self.open_form(at, false);
                if let Some(Dialog::Form(form)) = &mut self.dialog {
                    form.path = "https://".into();
                }
            }
            Action::AskEject => self.ask_eject(),
            Action::Eject { force } => self.start_eject(force),
            Action::ShowFound => {
                if !self.found.is_empty() {
                    let by_category = self.found.iter().any(|f| f.category.is_some());
                    self.dialog = Some(Dialog::Import {
                        items: self.found.iter().cloned().map(|f| (true, f)).collect(),
                        by_category,
                        group: tr("Nouveaux programmes").to_string(),
                    });
                }
            }
            Action::Import {
                items,
                by_category,
                group,
            } => {
                self.dialog = None;
                self.import(items, by_category, group);
            }
            Action::IgnoreFound(paths) => {
                self.found.retain(|f| !paths.contains(&f.path));
                self.settings.scan_ignored.extend(paths);
                self.persist_settings();
                self.dialog = None;
                self.apply(Action::ShowFound);
            }
            Action::MakePortable => {
                if let Some(n) = self.shared.modify(|c| Ok(edit::make_portable(c))) {
                    self.shared
                        .set_status(trf("{} chemins rendus relatifs", &[&n]));
                }
            }
            Action::OpenLog => {
                let path = crate::log::path();
                if let Err(e) = launcher::launch(&path.to_string_lossy(), None) {
                    self.shared.error(e);
                }
            }
        }
    }

    fn undo_redo(&self, redo: bool) {
        if self.shared.undo_redo(redo) {
            self.shared.set_status(if redo {
                tr("Modification rétablie")
            } else {
                tr("Modification annulée")
            });
        } else {
            self.shared.set_status(tr("Rien à annuler"));
        }
    }

    /// Ajoute les programmes trouvés, dans un groupe par catégorie ou dans
    /// un groupe unique (une seule étape d'annulation).
    fn import(&mut self, items: Vec<scan::Found>, by_category: bool, group: String) {
        if items.is_empty() {
            return;
        }
        let base = self.base_dir();
        let group = if group.trim().is_empty() {
            tr("Nouveaux programmes").to_string()
        } else {
            group.trim().to_string()
        };
        let added: Vec<String> = items.iter().map(|f| f.path.clone()).collect();
        let count = items.len();
        let result = self.shared.modify(|c| {
            for f in items {
                let name = match (&f.category, by_category) {
                    (Some(cat), true) => cat.clone(),
                    _ => group.clone(),
                };
                let parent = edit::find_or_create_group(c, &name)?;
                edit::insert(
                    c,
                    &parent,
                    None,
                    NewItem::Entry {
                        name: f.name,
                        path: paths::make_relative(&f.path, &base),
                        directory: None,
                        options: EntryOptions::default(),
                    },
                )?;
            }
            Ok(())
        });
        if result.is_some() {
            self.found.retain(|f| !added.contains(&f.path));
            self.shared
                .set_status(trf("{} raccourcis ajoutés", &[&count]));
            for t in added {
                self.refresh_target(paths::normalize(&t));
            }
        }
    }

    fn ask_eject(&mut self) {
        let Some(drive) = self.shared.eject_drive() else {
            self.shared
                .set_status(tr("Le lecteur de PStart-rs ne peut pas être éjecté"));
            return;
        };
        self.flush_notes();
        self.dialog = Some(Dialog::Eject {
            drive,
            procs: eject::processes_on(drive),
            stubborn: false,
        });
    }

    /// Ferme les programmes de la clé en tâche de fond, puis éjecte.
    fn start_eject(&mut self, force: bool) {
        let Some(Dialog::Eject { drive, procs, .. }) = &self.dialog else {
            return;
        };
        let drive = *drive;
        let pids: Vec<u32> = procs.iter().map(|p| p.pid).collect();
        *self.eject_state.lock().unwrap() = EjectState::Waiting;
        crate::log::info(&format!(
            "éjection de {drive}: ({} programmes ouverts)",
            pids.len()
        ));
        let state = self.eject_state.clone();
        let shared = self.shared.clone();
        std::thread::spawn(move || {
            if force {
                eject::kill(&pids);
            } else {
                eject::close_gracefully(&pids);
            }
            let mut remaining = eject::processes_on(drive);
            for _ in 0..16 {
                if remaining.is_empty() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(500));
                remaining = eject::processes_on(drive);
            }
            *state.lock().unwrap() = if remaining.is_empty() {
                EjectState::Ready
            } else {
                EjectState::Remaining(remaining)
            };
            shared.repaint();
        });
    }

    /// Suite de l'éjection, sur le fil de l'interface.
    fn poll_eject(&mut self) {
        let state = std::mem::take(&mut *self.eject_state.lock().unwrap());
        match state {
            EjectState::Idle => {}
            EjectState::Waiting => *self.eject_state.lock().unwrap() = EjectState::Waiting,
            EjectState::Remaining(procs) => {
                if let Some(Dialog::Eject { drive, .. }) = self.dialog {
                    self.dialog = Some(Dialog::Eject {
                        drive,
                        procs,
                        stubborn: true,
                    });
                }
            }
            EjectState::Ready => {
                let Some(Dialog::Eject { drive, .. }) = self.dialog else {
                    return;
                };
                self.dialog = None;
                match eject::eject(drive) {
                    Ok(()) => {
                        self.flush_notes();
                        crate::log::info("éjection lancée, fermeture de PStart-rs");
                        tray::quit(&self.shared);
                    }
                    Err(e) => self.shared.error(e),
                }
            }
        }
    }

    fn restore(&mut self, zip: &Path) {
        let Some(config) = self.config_file() else {
            return;
        };
        self.flush_notes();
        match backup::restore(zip, &backup::Targets::for_config(&config)) {
            Ok(names) => {
                if let Some(path) = &self.settings_path {
                    self.settings = settings::load(path);
                    i18n::set_language(self.settings.language);
                    if let Some(ctx) = self.shared.ctx.get() {
                        apply_appearance(ctx, &self.settings);
                    }
                    *self.shared.reminders.lock().unwrap() = self.settings.reminders;
                }
                self.open(&config);
                self.shared
                    .set_status(trf("Restauré : {}", &[&names.join(", ")]));
            }
            Err(e) => self.shared.error(e),
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
                        options: EntryOptions::default(),
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
                1 => tr("Raccourci ajouté").to_string(),
                n => trf("{} raccourcis ajoutés", &[&n]),
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
            options: EntryOptions::default(),
            icon: String::new(),
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
                    options: EntryOptions::default(),
                    icon: el.attr("icon").unwrap_or("").to_string(),
                    error: None,
                    focus: true,
                }
            } else {
                EditForm {
                    kind: FormKind::EditEntry(path),
                    name,
                    path: config::entry_path(el).to_string(),
                    directory: el.child_text("directory").unwrap_or("").to_string(),
                    options: EntryOptions::of(el),
                    icon: String::new(),
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
                    .set_title(tr("Ajouter des raccourcis"))
                    .set_directory(&start)
                    .pick_files()
                {
                    self.apply(Action::AddFiles(files));
                }
            }
            Browse::IconFile => {
                let Some(Dialog::Settings(form)) = &mut self.dialog else {
                    return;
                };
                let dir = form
                    .settings_path
                    .as_deref()
                    .and_then(Path::parent)
                    .map(|d| d.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let picked = rfd::FileDialog::new()
                    .set_title(tr("Choisir une icône"))
                    .set_directory(paths::to_native(&dir))
                    .add_filter(tr("Icônes et programmes"), &["ico", "exe", "dll"])
                    .add_filter(tr("Tous les fichiers"), &["*"])
                    .pick_file();
                if let Some(p) = picked {
                    let full = p.to_string_lossy().into_owned();
                    form.settings.icon_file = Some(paths::make_relative(&full, &dir));
                }
            }
            Browse::BackupExport => {
                let Some(config) = self.config_file() else {
                    return;
                };
                let dir = config.parent().map(Path::to_path_buf).unwrap_or_default();
                let dest = rfd::FileDialog::new()
                    .set_title(tr("Enregistrer la sauvegarde"))
                    .set_directory(&dir)
                    .set_file_name(backup::default_name())
                    .add_filter("Zip", &["zip"])
                    .save_file();
                if let Some(dest) = dest {
                    self.flush_notes();
                    match backup::export(&dest, &backup::Targets::for_config(&config)) {
                        Ok(n) => self.shared.set_status(trf(
                            "Sauvegarde créée ({} fichiers) : {}",
                            &[&n, &dest.display()],
                        )),
                        Err(e) => self.shared.error(e),
                    }
                }
            }
            Browse::ScanFolder => {
                let start_dir = self.scan_dir().unwrap_or_else(|| start.clone());
                let picked = rfd::FileDialog::new()
                    .set_title(tr("Dossier où chercher des programmes"))
                    .set_directory(&start_dir)
                    .pick_folder();
                if let Some(dir) = picked {
                    let full = dir.to_string_lossy().into_owned();
                    let rel = paths::make_relative(&full, &base);
                    let changed = self.settings.scan_folder.as_deref() != Some(rel.as_str());
                    self.settings.scan_folder = Some(rel.clone());
                    if let Some(Dialog::Settings(form)) = &mut self.dialog {
                        form.settings.scan_folder = Some(rel);
                    }
                    if changed {
                        self.persist_settings();
                    }
                    self.start_scan(dir, false);
                }
            }
            Browse::GroupIcon => {
                let Some(Dialog::Form(form)) = &mut self.dialog else {
                    return;
                };
                let picked = rfd::FileDialog::new()
                    .set_title(tr("Choisir une icône"))
                    .set_directory(&start)
                    .add_filter(tr("Icônes et programmes"), &["ico", "exe", "dll"])
                    .add_filter(tr("Tous les fichiers"), &["*"])
                    .pick_file();
                if let Some(p) = picked {
                    form.icon = paths::make_relative(&p.to_string_lossy(), &base);
                }
            }
            Browse::BackupRestore => {
                let dir = self
                    .config_file()
                    .and_then(|c| c.parent().map(Path::to_path_buf))
                    .unwrap_or_default();
                let picked = rfd::FileDialog::new()
                    .set_title(tr("Restaurer une sauvegarde"))
                    .set_directory(&dir)
                    .add_filter("Zip", &["zip"])
                    .pick_file();
                if let Some(file) = picked {
                    match backup::contents(&file) {
                        Ok(contents) if contents.iter().any(|c| c == backup::CONFIG) => {
                            self.dialog = Some(Dialog::ConfirmRestore { file, contents });
                        }
                        Ok(_) => self
                            .shared
                            .set_status(trf("{} absent de l'archive", &[&backup::CONFIG])),
                        Err(e) => self.shared.error(trf("restauration : {}", &[&e])),
                    }
                }
            }
            Browse::EntryPath | Browse::WorkingDir => {
                let Some(Dialog::Form(form)) = &mut self.dialog else {
                    return;
                };
                let picked = if matches!(kind, Browse::EntryPath) {
                    rfd::FileDialog::new()
                        .set_title(tr("Choisir le programme ou le document"))
                        .set_directory(&start)
                        .pick_file()
                } else {
                    rfd::FileDialog::new()
                        .set_title(tr("Choisir le dossier de travail"))
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
        let base_dir = self.base_dir();
        let Some(Dialog::Form(form)) = &mut self.dialog else {
            return;
        };
        let name = form.name.trim().to_string();
        let path = form.path.trim().to_string();
        let directory = form.directory.trim().to_string();
        if name.is_empty() {
            form.error = Some(tr("Le nom est obligatoire.").into());
            return;
        }
        if !form.is_group() && path.is_empty() {
            form.error = Some(tr("Le chemin est obligatoire.").into());
            return;
        }
        let dir = (!directory.is_empty()).then_some(directory);
        let options = form.options.clone();
        let result = match &form.kind {
            FormKind::NewEntry { parent, index } => {
                let (parent, index) = (parent.clone(), *index);
                let item = NewItem::Entry {
                    name,
                    path: path.clone(),
                    directory: dir,
                    options: form.options.clone(),
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
                    edit::update_entry(c, &p, &name, &path, dir.as_deref(), &options)?;
                    Ok(p.clone())
                })
            }
            FormKind::RenameGroup(p) => {
                let p = p.clone();
                let icon = form.icon.trim().to_string();
                if !icon.is_empty() {
                    let target = paths::resolve(&icon, &base_dir);
                    let generation = self.shared.generation.load(Ordering::SeqCst);
                    icons::spawn_loader(vec![target], self.shared.clone(), generation);
                }
                self.shared.modify(|c| {
                    edit::rename(c, &p, &name)?;
                    edit::set_group_icon(c, &p, Some(&icon))?;
                    Ok(p.clone())
                })
            }
        };
        if let Some(p) = result {
            let is_group = form.is_group();
            self.selected = Some(p);
            self.dialog = None;
            self.shared.set_status(tr("Enregistré"));
            if !is_group {
                let target = self.resolve_path(&path);
                self.refresh_target(target);
            }
        }
    }

    // ---- Dessin -------------------------------------------------------

    fn top_bar(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>, cfg: Option<&Config>) {
        let compact = self.settings.compact;
        ui.horizontal(|ui| {
            if compact {
                // Mode compact : vues et boutons regroupés dans un menu.
                ui.menu_button("☰", |ui| {
                    for v in View::ALL {
                        if ui.selectable_label(self.view == v, v.label()).clicked() {
                            self.view = v;
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui.button(tr("Nouveau raccourci…")).clicked() {
                        actions.push(Action::NewEntry(None));
                        ui.close();
                    }
                    if ui.button(tr("Réglages")).clicked() {
                        actions.push(Action::OpenSettings);
                        ui.close();
                    }
                    if ui.button(tr("Fenêtre complète")).clicked() {
                        actions.push(Action::ToggleCompact);
                        ui.close();
                    }
                });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !compact {
                    if ui.button("⚙").on_hover_text(tr("Réglages")).clicked() {
                        actions.push(Action::OpenSettings);
                    }
                    if ui.button("◧").on_hover_text(tr("Mode compact")).clicked() {
                        actions.push(Action::ToggleCompact);
                    }
                    if ui
                        .button("⟳")
                        .on_hover_text(tr("Recharger le fichier"))
                        .clicked()
                    {
                        actions.push(Action::Reload);
                    }
                    let can_redo = !self.shared.redo.lock().unwrap().is_empty();
                    let can_undo = !self.shared.undo.lock().unwrap().is_empty();
                    if ui
                        .add_enabled(can_redo, egui::Button::new("↷"))
                        .on_hover_text(tr("Rétablir (Ctrl+Y)"))
                        .clicked()
                    {
                        actions.push(Action::Redo);
                    }
                    if ui
                        .add_enabled(can_undo, egui::Button::new("↶"))
                        .on_hover_text(tr("Annuler (Ctrl+Z)"))
                        .clicked()
                    {
                        actions.push(Action::Undo);
                    }
                    ui.add_enabled_ui(cfg.is_some(), |ui| {
                        ui.menu_button("＋", |ui| {
                            if ui.button(tr("Nouveau raccourci…")).clicked() {
                                actions.push(Action::NewEntry(None));
                            }
                            if ui.button(tr("Nouveau groupe…")).clicked() {
                                actions.push(Action::NewGroup(None));
                            }
                            if ui.button(tr("Nouveau lien web…")).clicked() {
                                actions.push(Action::NewLink(None));
                            }
                            if ui.button(tr("Séparateur")).clicked() {
                                actions.push(Action::NewSeparator(None));
                            }
                            ui.separator();
                            if ui.button(tr("Ajouter des fichiers…")).clicked() {
                                actions.push(Action::Browse(Browse::AddFiles));
                            }
                            if ui.button(tr("Rechercher des programmes…")).clicked() {
                                actions.push(Action::Browse(Browse::ScanFolder));
                            }
                        })
                        .response
                        .on_hover_text(tr("Ajouter (vous pouvez aussi déposer des fichiers ici)"));
                    });
                }
                let search = egui::TextEdit::singleline(&mut self.search)
                    .hint_text(tr("Rechercher… (Ctrl+F)"))
                    .desired_width(ui.available_width());
                let resp = ui.add(search);
                self.search_id = Some(resp.id);
                if self.focus_search {
                    resp.request_focus();
                    self.focus_search = false;
                }
            });
        });
        if !compact {
            ui.horizontal_wrapped(|ui| {
                for v in View::ALL {
                    ui.selectable_value(&mut self.view, v, v.label());
                }
            });
        }
        if let Some(cfg) = cfg {
            self.favorites_bar(ui, cfg, actions);
        }
    }

    /// Barre des favoris : icônes (ou initiales) cliquables.
    fn favorites_bar(&self, ui: &mut egui::Ui, cfg: &Config, actions: &mut Vec<Action>) {
        let favorites = Self::favorites(cfg);
        if favorites.is_empty() {
            return;
        }
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (i, e) in favorites.iter().enumerate() {
                let tip = if i < 9 {
                    format!("{}  (Ctrl+{})", e.name, i + 1)
                } else {
                    e.name.clone()
                };
                let resp = match self.textures.get(&e.target) {
                    Some(tex) => ui.add(egui::Button::image(icon_image(tex, 20.0))),
                    None => {
                        let initial: String = e.name.chars().take(2).collect();
                        ui.add(egui::Button::new(initial).min_size(egui::vec2(28.0, 28.0)))
                    }
                };
                let resp = resp.on_hover_text(tip);
                if resp.clicked() {
                    actions.push(Action::Launch(e.path.clone()));
                }
                resp.context_menu(|ui| {
                    if ui.button(tr("Retirer des favoris")).clicked() {
                        actions.push(Action::ToggleFavorite(e.path.clone()));
                        ui.close();
                    }
                    if ui.button(tr("Modifier…")).clicked() {
                        actions.push(Action::Edit(e.path.clone()));
                        ui.close();
                    }
                });
            }
        });
    }

    fn central(&self, ui: &mut egui::Ui, cfg: Option<&Config>) -> Vec<Action> {
        let mut actions = Vec::new();
        let Some(cfg) = cfg else {
            self.welcome(ui);
            return actions;
        };
        self.visible.borrow_mut().clear();
        let rows = Rows {
            selected: &self.selected,
            exists: &self.exists,
            textures: &self.textures,
            single: cfg.single_click(),
            visible: &self.visible,
            open: &self.open_groups,
            scroll_to: self.scroll_to_selected,
        };
        let query = fuzzy::fold(self.search.trim());
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
                            let mut keys = Vec::new();
                            rows.tree(ui, cfg, files, &mut path, &mut keys, &mut actions);
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
                    View::Notes => self.notes_view(ui, cfg, &mut actions),
                    View::Stats => self.stats_view(ui, cfg, &mut actions),
                }
            });
        actions
    }

    fn notes_view(&self, ui: &mut egui::Ui, cfg: &Config, actions: &mut Vec<Action>) {
        let palette = cfg.palette();
        let mut guard = self.shared.notes.lock().unwrap();
        let Some(notes) = guard.as_mut() else {
            ui.weak(tr("Notes indisponibles."));
            return;
        };
        let mut changed = false;
        ui.horizontal(|ui| {
            if ui.button(tr("＋ Nouvelle note")).clicked() {
                notes.items.insert(0, Note::new(cfg.default_note_color()));
                changed = true;
            }
            ui.weak(trf("Enregistrées dans {}", &[&notes::FILE_NAME]));
        });
        ui.add_space(6.0);
        if notes.items.is_empty() {
            ui.weak(tr(
                "Aucune note. Les alarmes s'affichent même fenêtre masquée.",
            ));
        }
        let mut delete = None;
        for (i, note) in notes.items.iter_mut().enumerate() {
            let (c, del) = note_card(ui, i, note, &palette);
            changed |= c;
            if del {
                delete = Some(i);
            }
            ui.add_space(6.0);
        }
        if let Some(i) = delete {
            notes.items.remove(i);
            changed = true;
        }
        if changed {
            actions.push(Action::NotesChanged);
        }
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
            let hits = pstart_hits(cfg, query);
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
            let hits: Vec<&StartItem> = start_hits(&start, query)
                .into_iter()
                .filter(|i| i.source == source)
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
            ui.weak(tr("Aucun résultat."));
        }
    }

    fn welcome(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.heading("PStart-rs");
            ui.add_space(12.0);
            ui.label(tr("Glissez un fichier PStart.xml sur cette fenêtre,"));
            ui.label(tr("ou placez pstart-rs.exe à côté de votre PStart.xml."));
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
            ui.weak(tr("Sélectionnez un raccourci."));
            ui.add_space(16.0);
            self.shortcuts_help(ui);
            return actions;
        };
        let Some(el) = cfg.item(sel) else {
            return actions;
        };
        if el.name == "files" {
            ui.heading(config::display_name(el));
            ui.weak(trf("{} raccourcis", &[&edit::count_entries(el)]));
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                if ui.button(tr("▶ Tout lancer")).clicked() {
                    actions.push(Action::LaunchGroup {
                        path: sel.clone(),
                        confirmed: false,
                    });
                }
                if ui.button(tr("＋ Raccourci")).clicked() {
                    actions.push(Action::NewEntry(Some(sel.clone())));
                }
                if ui.button(tr("＋ Groupe")).clicked() {
                    actions.push(Action::NewGroup(Some(sel.clone())));
                }
                if ui.button(tr("Renommer")).clicked() {
                    actions.push(Action::Edit(sel.clone()));
                }
                if ui.button(tr("Supprimer")).clicked() {
                    actions.push(Action::AskDelete(sel.clone()));
                }
            });
            return actions;
        }
        let target = cfg.resolve(el);
        let opts = EntryOptions::of(el);
        ui.horizontal(|ui| {
            if let Some(tex) = self.textures.get(&target) {
                ui.add(icon_image(tex, 32.0));
            }
            ui.heading(config::display_name(el));
        });
        if !opts.description.is_empty() {
            ui.add(egui::Label::new(egui::RichText::new(&opts.description).italics()).wrap());
        }
        ui.add_space(6.0);
        egui::Grid::new("details")
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.label(tr("Chemin"));
                ui.add(egui::Label::new(config::entry_path(el)).wrap());
                ui.end_row();
                ui.label(tr("Cible"));
                ui.add(egui::Label::new(&target).wrap());
                ui.end_row();
                if !opts.parameters.is_empty() {
                    ui.label(tr("Paramètres"));
                    ui.add(
                        egui::Label::new(egui::RichText::new(&opts.parameters).monospace()).wrap(),
                    );
                    ui.end_row();
                }
                if let Some(dir) = cfg.working_dir(el) {
                    ui.label(tr("Dossier"));
                    ui.add(egui::Label::new(dir).wrap());
                    ui.end_row();
                }
                let mut flags = Vec::new();
                if opts.admin {
                    flags.push(tr("administrateur"));
                }
                if opts.window != WindowState::Normal {
                    flags.push(opts.window.label());
                }
                if opts.autorun {
                    flags.push(tr("au démarrage"));
                }
                if !opts.hotkey.is_empty() {
                    ui.label(tr("Raccourci clavier"));
                    ui.label(&opts.hotkey);
                    ui.end_row();
                }
                if !flags.is_empty() {
                    ui.label(tr("Options"));
                    ui.label(flags.join(", "));
                    ui.end_row();
                }
                ui.label(tr("État"));
                match self.exists.get(&target) {
                    Some(true) => ui.label(tr("présent")),
                    Some(false) => ui.colored_label(ui.visuals().warn_fg_color, tr("introuvable")),
                    None => ui.weak(tr("vérification…")),
                };
                ui.end_row();
                ui.label(tr("Lancements"));
                ui.label(config::runcount(el).to_string());
                ui.end_row();
                ui.label(tr("Dernier"));
                ui.label(el.child_text("rundate").unwrap_or(tr("jamais")));
                ui.end_row();
            });
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button(tr("▶ Lancer")).clicked() {
                actions.push(Action::Launch(sel.clone()));
            }
            if ui.button(tr("Modifier…")).clicked() {
                actions.push(Action::Edit(sel.clone()));
            }
            if ui.button(tr("Emplacement")).clicked() {
                actions.push(Action::Reveal(sel.clone()));
            }
            let pin = if opts.favorite {
                tr("★ Retirer des favoris")
            } else {
                tr("☆ Ajouter aux favoris")
            };
            if ui.button(pin).clicked() {
                actions.push(Action::ToggleFavorite(sel.clone()));
            }
            if ui.button(tr("Supprimer")).clicked() {
                actions.push(Action::AskDelete(sel.clone()));
            }
        });
        actions
    }

    fn shortcuts_help(&self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new(tr("Raccourcis")).strong());
        ui.weak(tr("Entrée : lancer"));
        ui.weak(tr("F2 : modifier, Suppr : supprimer"));
        ui.weak(tr("Ctrl+F : rechercher, Échap : effacer"));
        ui.weak(tr("Glisser-déposer : réorganiser"));
        ui.weak(tr("Flèches : naviguer, ouvrir ou fermer un groupe"));
        ui.weak(tr("Ctrl+1 à 9 : lancer un favori"));
        ui.weak(tr("Ctrl+Z / Ctrl+Y : annuler / rétablir"));
        match &self.hotkey_info {
            Some(hk) => ui.weak(trf("{} : afficher / masquer", &[&hk])),
            None => ui.weak(tr("Raccourci global : désactivé")),
        };
        ui.add_space(8.0);
        ui.weak(trf("Réglages : {}", &[&settings::FILE_NAME]));
    }

    fn dialog_ui(&mut self, ctx: &egui::Context, cfg: Option<&Config>, actions: &mut Vec<Action>) {
        let base = cfg.map(|c| c.base_dir.clone()).unwrap_or_default();
        let upd = self.shared.update.lock().unwrap().clone();
        let waiting = matches!(*self.eject_state.lock().unwrap(), EjectState::Waiting);
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        let modal = egui::Modal::new(egui::Id::unique("pstart-dialog")).show(ctx, |ui| {
            // La fenêtre peut être étroite (ou le texte agrandi).
            let screen = ctx.content_rect();
            ui.set_width(460.0f32.min(screen.width() - 40.0).max(200.0));
            ui.set_max_height(screen.height() - 40.0);
            match dialog {
                Dialog::Form(form) => form_ui(ui, form, &base, actions),
                Dialog::Settings(form) => {
                    egui::ScrollArea::vertical()
                        .show(ui, |ui| settings_ui(ui, form, &upd, actions));
                }
                Dialog::ConfirmDelete {
                    path,
                    name,
                    count,
                    group,
                } => {
                    ui.heading(tr("Supprimer"));
                    ui.add_space(8.0);
                    if *group {
                        ui.label(trf(
                            "Supprimer le groupe « {} » et ses {} raccourcis ?",
                            &[name, count],
                        ));
                    } else {
                        ui.label(trf("Supprimer le raccourci « {} » ?", &[name]));
                    }
                    ui.weak(tr("Le programme lui-même n'est pas supprimé."));
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button(tr("Supprimer")).clicked() {
                            actions.push(Action::Delete(path.clone()));
                        }
                        if ui.button(tr("Annuler")).clicked() {
                            actions.push(Action::CloseDialog);
                        }
                    });
                }
                Dialog::DriveFix { old, new } => drive_fix_ui(ui, cfg, old, new, actions),
                Dialog::ConfirmCleanup(paths) => {
                    ui.heading(tr("Nettoyage"));
                    ui.add_space(8.0);
                    ui.label(trf(
                        "Supprimer les {} raccourcis dont la cible est introuvable ?",
                        &[&paths.len()],
                    ));
                    if let Some(cfg) = cfg {
                        egui::ScrollArea::vertical()
                            .max_height(200.0)
                            .show(ui, |ui| {
                                for p in paths.iter() {
                                    if let Some(el) = cfg.item(p) {
                                        ui.weak(format!("• {}", config::display_name(el)));
                                    }
                                }
                            });
                    }
                    ui.weak(tr(
                        "Pensez à brancher les lecteurs réseau ou amovibles avant.",
                    ));
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button(tr("Supprimer")).clicked() {
                            actions.push(Action::Cleanup(paths.clone()));
                        }
                        if ui.button(tr("Annuler")).clicked() {
                            actions.push(Action::CloseDialog);
                        }
                    });
                }
                Dialog::ConfirmLaunchGroup { path, name, count } => {
                    ui.heading(tr("Lancer le groupe"));
                    ui.add_space(8.0);
                    ui.label(trf(
                        "Lancer les {} programmes du groupe « {} » ?",
                        &[count, name],
                    ));
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button(tr("Lancer")).clicked() {
                            actions.push(Action::LaunchGroup {
                                path: path.clone(),
                                confirmed: true,
                            });
                        }
                        if ui.button(tr("Annuler")).clicked() {
                            actions.push(Action::CloseDialog);
                        }
                    });
                }
                Dialog::Eject {
                    drive,
                    procs,
                    stubborn,
                } => eject_ui(ui, *drive, procs, *stubborn, waiting, actions),
                Dialog::Import {
                    items,
                    by_category,
                    group,
                } => import_ui(ui, items, by_category, group, actions),
                Dialog::ConfirmRestore { file, contents } => {
                    ui.heading(tr("Restaurer une sauvegarde"));
                    ui.add_space(8.0);
                    ui.label(file.display().to_string());
                    ui.label(trf("Fichiers remplacés : {}", &[&contents.join(", ")]));
                    ui.weak(tr(
                        "Les fichiers actuels sont conservés en « .avant-restauration ».",
                    ));
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button(tr("Restaurer")).clicked() {
                            actions.push(Action::Restore(file.clone()));
                        }
                        if ui.button(tr("Annuler")).clicked() {
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
        // Ctrl+1…9 : favoris.
        const DIGITS: [egui::Key; 9] = [
            egui::Key::Num1,
            egui::Key::Num2,
            egui::Key::Num3,
            egui::Key::Num4,
            egui::Key::Num5,
            egui::Key::Num6,
            egui::Key::Num7,
            egui::Key::Num8,
            egui::Key::Num9,
        ];
        let favorite = ctx.input(|i| {
            if !i.modifiers.command {
                return None;
            }
            DIGITS.iter().position(|k| i.key_pressed(*k))
        });
        if let Some(n) = favorite {
            actions.push(Action::LaunchFavorite(n));
        }
        let (undo, redo) = ctx.input(|i| {
            let z = i.modifiers.command && i.key_pressed(egui::Key::Z);
            (
                z && !i.modifiers.shift,
                (z && i.modifiers.shift) || (i.modifiers.command && i.key_pressed(egui::Key::Y)),
            )
        });
        // Dans la recherche vide, Ctrl+Z annule aussi la dernière modification.
        let search_empty = self.search.is_empty()
            && self
                .search_id
                .is_some_and(|id| ctx.memory(|m| m.has_focus(id)));
        if !typing || search_empty {
            if undo {
                actions.push(Action::Undo);
            }
            if redo {
                actions.push(Action::Redo);
            }
        }
        let search_focused = self
            .search_id
            .is_some_and(|id| ctx.memory(|m| m.has_focus(id)));
        if !typing || search_focused {
            self.navigate(ctx, search_focused, actions);
        }
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
        // Entrée dans une note ou un autre champ ne doit rien lancer.
        if enter && (!typing || search_focused) {
            self.enter_pressed(actions);
        }
    }

    /// Flèches : déplacement dans les lignes visibles ; droite/gauche
    /// ouvrent ou ferment un groupe (arborescence uniquement).
    fn navigate(&mut self, ctx: &egui::Context, search_focused: bool, actions: &mut Vec<Action>) {
        let (up, down, left, right) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
            )
        });
        if !(up || down || left || right) {
            return;
        }
        let visible = self.visible.borrow().clone();
        if visible.is_empty() {
            return;
        }
        let pos = self
            .selected
            .as_ref()
            .and_then(|s| visible.iter().position(|p| p == s));
        if up || down {
            let next = match (pos, down) {
                (None, _) => 0,
                (Some(i), true) => (i + 1).min(visible.len() - 1),
                (Some(i), false) => i.saturating_sub(1),
            };
            self.selected = Some(visible[next].clone());
            self.scroll_to_selected = true;
            return;
        }
        // Gauche/droite : réservées au champ de recherche quand il a le focus.
        if search_focused || !self.search.trim().is_empty() || self.view != View::Tree {
            return;
        }
        let Some(sel) = self.selected.clone() else {
            return;
        };
        let (is_group, key) = {
            let guard = self.shared.config.lock().unwrap();
            let Some(cfg) = guard.as_ref() else { return };
            (
                cfg.item(&sel).is_some_and(|e| e.name == "files"),
                group_key_at(cfg, &sel),
            )
        };
        let open = key.as_ref().is_some_and(|k| self.open_groups.contains(k));
        if right && is_group {
            if open {
                // Déjà ouvert : premier élément du groupe.
                if let Some(i) = pos.filter(|i| i + 1 < visible.len()) {
                    if visible[i + 1].starts_with(&sel) {
                        self.selected = Some(visible[i + 1].clone());
                        self.scroll_to_selected = true;
                    }
                }
            } else if let Some(k) = key {
                actions.push(Action::ToggleGroup(k));
            }
        } else if left {
            if is_group && open {
                if let Some(k) = key {
                    actions.push(Action::ToggleGroup(k));
                }
            } else if sel.len() > 1 {
                self.selected = Some(sel[..sel.len() - 1].to_vec());
                self.scroll_to_selected = true;
            }
        }
    }

    /// Entrée : lance la sélection, ou le premier résultat de recherche.
    fn enter_pressed(&self, actions: &mut Vec<Action>) {
        let q = fuzzy::fold(self.search.trim());
        if q.is_empty() {
            if let Some(p) = self.selected.clone() {
                actions.push(Action::Launch(p));
            }
            return;
        }
        // Résultat choisi avec les flèches.
        if let Some(sel) = &self.selected {
            if self.visible.borrow().contains(sel) {
                actions.push(Action::Launch(sel.clone()));
                return;
            }
        }
        let guard = self.shared.config.lock().unwrap();
        if let Some(cfg) = guard.as_ref() {
            let opts = cfg.search_options();
            if opts.pstart {
                if let Some(e) = pstart_hits(cfg, &q).into_iter().next() {
                    actions.push(Action::Launch(e.path));
                    return;
                }
            }
            let start = self.shared.start_menu.lock().unwrap();
            let first = start_hits(&start, &q).into_iter().find(|i| match i.source {
                Source::CurrentUser => opts.current_user,
                Source::AllUsers => opts.all_users,
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
        self.flush_notes();
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

fn form_ui(ui: &mut egui::Ui, form: &mut EditForm, base: &str, actions: &mut Vec<Action>) {
    ui.heading(form.title());
    ui.add_space(8.0);
    let mut submit = false;
    egui::Grid::new("form")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label(tr("Nom"));
            let name = ui.add(egui::TextEdit::singleline(&mut form.name).desired_width(300.0));
            if form.focus {
                name.request_focus();
                form.focus = false;
            }
            submit |= name.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.end_row();
            if matches!(form.kind, FormKind::RenameGroup(_)) {
                ui.label(tr("Icône"));
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut form.icon)
                            .desired_width(240.0)
                            .hint_text(tr("(aucune)")),
                    );
                    if ui.button(tr("Parcourir…")).clicked() {
                        actions.push(Action::Browse(Browse::GroupIcon));
                    }
                });
                ui.end_row();
            }
            if !form.is_group() {
                ui.label(tr("Chemin"));
                ui.horizontal(|ui| {
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut form.path)
                            .desired_width(240.0)
                            .hint_text("_Dossier\\programme.exe"),
                    );
                    submit |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.button(tr("Parcourir…")).clicked() {
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
                ui.label(tr("Dossier"));
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut form.directory)
                            .desired_width(240.0)
                            .hint_text(tr("(dossier du programme)")),
                    );
                    if ui
                        .button("…")
                        .on_hover_text(tr("Choisir le dossier"))
                        .clicked()
                    {
                        actions.push(Action::Browse(Browse::WorkingDir));
                    }
                });
                ui.end_row();

                let o = &mut form.options;
                ui.label(tr("Paramètres"));
                let r = ui.add(
                    egui::TextEdit::singleline(&mut o.parameters)
                        .desired_width(300.0)
                        .hint_text(tr("(aucun)")),
                );
                submit |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.end_row();
                ui.label(tr("Description"));
                ui.add(egui::TextEdit::singleline(&mut o.description).desired_width(300.0));
                ui.end_row();
                ui.label(tr("Raccourci clavier"));
                let valid = o.hotkey.trim().is_empty() || hotkey::parse(&o.hotkey).is_ok();
                let mut hk = egui::TextEdit::singleline(&mut o.hotkey)
                    .desired_width(160.0)
                    .hint_text(tr("ex. Ctrl+Alt+V"));
                if !valid {
                    hk = hk.text_color(ui.visuals().error_fg_color);
                }
                ui.add(hk).on_hover_text(tr(
                    "Lance ce programme depuis n'importe où, même fenêtre masquée",
                ));
                ui.end_row();
                ui.label(tr("Fenêtre"));
                egui::ComboBox::from_id_salt("windowstate")
                    .selected_text(o.window.label())
                    .show_ui(ui, |ui| {
                        for w in WindowState::ALL {
                            ui.selectable_value(&mut o.window, w, w.label());
                        }
                    });
                ui.end_row();
                ui.label(tr("Options"));
                ui.vertical(|ui| {
                    ui.add_enabled(
                        cfg!(windows),
                        egui::Checkbox::new(&mut o.admin, tr("Exécuter en tant qu'administrateur")),
                    );
                    ui.checkbox(&mut o.autorun, tr("Lancer au démarrage de PStart-rs"));
                    ui.checkbox(&mut o.favorite, tr("Épingler dans les favoris"));
                });
                ui.end_row();
            }
        });
    if !form.is_group() {
        ui.weak(tr(
            "Les chemins sur le même lecteur sont enregistrés en relatif.",
        ));
        ui.weak(tr(
            "Variables : %PSTART% (dossier de PStart.xml), %DRIVE% (son lecteur), %USERPROFILE%…",
        ));
    }
    if let Some(e) = &form.error {
        ui.colored_label(ui.visuals().error_fg_color, e);
    }
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        if ui.button(tr("Enregistrer")).clicked() {
            submit = true;
        }
        if ui.button(tr("Annuler")).clicked() {
            actions.push(Action::CloseDialog);
        }
    });
    if submit {
        actions.push(Action::SubmitForm);
    }
}

/// Carte d'une note. Renvoie (modifiée, à supprimer).
fn note_card(ui: &mut egui::Ui, i: usize, note: &mut Note, palette: &[[u8; 3]]) -> (bool, bool) {
    let [r, g, b] = note.color;
    let fill = egui::Color32::from_rgb(r, g, b);
    let ink = egui::Color32::from_gray(25);
    let mut changed = false;
    let mut delete = false;
    egui::Frame::new()
        .fill(fill)
        .corner_radius(6.0)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let v = ui.visuals_mut();
            v.override_text_color = Some(ink);
            v.extreme_bg_color = lighten(fill);
            v.widgets.inactive.weak_bg_fill = darken(fill);
            v.widgets.inactive.bg_fill = darken(fill);
            v.widgets.hovered.weak_bg_fill = darken(darken(fill));
            v.widgets.inactive.fg_stroke.color = ink;
            v.widgets.hovered.fg_stroke.color = ink;
            v.widgets.active.fg_stroke.color = ink;
            changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut note.title)
                        .hint_text(tr("Titre"))
                        .font(egui::TextStyle::Heading)
                        .desired_width(f32::INFINITY),
                )
                .changed();
            changed |= ui
                .add(
                    egui::TextEdit::multiline(&mut note.text)
                        .hint_text(tr("Texte de la note"))
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                )
                .changed();
            ui.horizontal_wrapped(|ui| {
                for &c in palette {
                    let color = egui::Color32::from_rgb(c[0], c[1], c[2]);
                    let (rect, resp) =
                        ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::click());
                    ui.painter().rect_filled(rect, 3.0, color);
                    let stroke_w = if c == note.color { 2.0 } else { 1.0 };
                    ui.painter().rect_stroke(
                        rect,
                        3.0,
                        egui::Stroke::new(stroke_w, ink),
                        egui::StrokeKind::Inside,
                    );
                    if resp.on_hover_text(tr("Couleur")).clicked() && note.color != c {
                        note.color = c;
                        changed = true;
                    }
                }
                ui.add_space(8.0);
                ui.label(tr("Alarme"));
                let valid = notes::parse_input(&note.alarm_input).is_some()
                    || note.alarm_input.trim().is_empty();
                let mut edit = egui::TextEdit::singleline(&mut note.alarm_input)
                    .hint_text(tr("AAAA/MM/JJ HH:MM"))
                    .desired_width(120.0);
                if !valid {
                    edit = edit.text_color(egui::Color32::from_rgb(180, 0, 0));
                }
                let resp = ui.add(edit);
                if resp.changed() && note.apply_input() {
                    changed = true;
                }
                if resp.lost_focus() && note.apply_input() {
                    note.sync_input();
                }
                if note.alarm.is_some() {
                    let before = note.repeat;
                    egui::ComboBox::from_id_salt(("repeat", i))
                        .selected_text(note.repeat.label())
                        .show_ui(ui, |ui| {
                            for r in Repeat::ALL {
                                ui.selectable_value(&mut note.repeat, r, r.label());
                            }
                        });
                    changed |= note.repeat != before;
                }
                if ui.small_button(tr("Supprimer")).clicked() {
                    delete = true;
                }
            });
        });
    (changed, delete)
}

fn lighten(c: egui::Color32) -> egui::Color32 {
    let f = |v: u8| v + (255 - v) / 2;
    egui::Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
}

fn darken(c: egui::Color32) -> egui::Color32 {
    let f = |v: u8| (v as u16 * 85 / 100) as u8;
    egui::Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
}

/// Correction de la lettre de lecteur.
fn drive_fix_ui(
    ui: &mut egui::Ui,
    cfg: Option<&Config>,
    old: &mut String,
    new: &mut String,
    actions: &mut Vec<Action>,
) {
    ui.heading(tr("Lettre de lecteur"));
    ui.add_space(8.0);
    ui.label(tr(
        "Les chemins absolus qui commencent par l'ancienne lettre seront modifiés.",
    ));
    let letter = |s: &str| {
        let mut c = s.trim().chars();
        match (c.next(), c.next()) {
            (Some(l), None) | (Some(l), Some(':')) if l.is_ascii_alphabetic() => {
                Some(l.to_ascii_uppercase())
            }
            _ => None,
        }
    };
    ui.add_space(6.0);
    egui::Grid::new("drive").num_columns(2).show(ui, |ui| {
        ui.label(tr("Ancienne lettre"));
        let r = ui.add(
            egui::TextEdit::singleline(old)
                .desired_width(40.0)
                .char_limit(2),
        );
        if old.is_empty() && ui.memory(|m| m.focused().is_none()) {
            r.request_focus();
        }
        ui.end_row();
        ui.label(tr("Nouvelle lettre"));
        ui.add(
            egui::TextEdit::singleline(new)
                .desired_width(40.0)
                .char_limit(2),
        );
        ui.end_row();
    });
    let (o, n) = (letter(old), letter(new));
    let count = match (cfg, o) {
        (Some(cfg), Some(o)) => edit::count_drive_refs(cfg, o),
        _ => 0,
    };
    ui.add_space(4.0);
    if o.is_some() {
        ui.weak(trf("{} chemins concernés", &[&count]));
    }
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        let ok = matches!((o, n), (Some(a), Some(b)) if a != b) && count > 0;
        if ui
            .add_enabled(ok, egui::Button::new(tr("Corriger")))
            .clicked()
        {
            if let (Some(old), Some(new)) = (o, n) {
                actions.push(Action::FixDrive { old, new });
            }
        }
        if ui.button(tr("Ignorer")).clicked() {
            actions.push(Action::CloseDialog);
        }
    });
}

fn settings_ui(
    ui: &mut egui::Ui,
    form: &mut SettingsForm,
    upd: &update::State,
    actions: &mut Vec<Action>,
) {
    ui.heading(tr("Réglages"));
    ui.add_space(8.0);
    refresh_preview(ui.ctx(), form);
    let preview = form.preview.as_ref().map(|(_, t)| t.clone());
    let s = &mut form.settings;
    egui::Grid::new("settings")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label(egui::RichText::new(tr("Apparence")).strong());
            ui.end_row();

            ui.label(tr("Langue"));
            egui::ComboBox::from_id_salt("language")
                .selected_text(s.language.label())
                .show_ui(ui, |ui| {
                    for l in Language::ALL {
                        ui.selectable_value(&mut s.language, l, l.label());
                    }
                });
            ui.end_row();

            ui.label(tr("Thème"));
            ui.horizontal(|ui| {
                for t in Theme::ALL {
                    ui.selectable_value(&mut s.theme, t, t.label());
                }
            });
            ui.end_row();

            ui.label(tr("Taille du texte"));
            ui.add(
                egui::DragValue::new(&mut s.text_size)
                    .range(settings::TEXT_SIZE_RANGE)
                    .speed(1.0)
                    .suffix(" %"),
            );
            ui.end_row();

            ui.label(tr("Icône"));
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    if let Some(tex) = &preview {
                        ui.add(icon_image(tex, 32.0));
                    }
                    let custom = s.icon_file.is_some();
                    ui.add_enabled_ui(!custom, |ui| {
                        egui::ComboBox::from_id_salt("icon_shape")
                            .selected_text(s.icon_shape.label())
                            .width(90.0)
                            .show_ui(ui, |ui| {
                                for shape in IconShape::ALL {
                                    ui.selectable_value(&mut s.icon_shape, shape, shape.label());
                                }
                            });
                        for c in IconColor::ALL {
                            if color_swatch(ui, c, s.icon_color == c).clicked() {
                                s.icon_color = c;
                            }
                        }
                    });
                });
                ui.horizontal(|ui| {
                    if ui
                        .button(tr("Fichier…"))
                        .on_hover_text(tr("Icône d'un fichier .ico ou .exe"))
                        .clicked()
                    {
                        actions.push(Action::Browse(Browse::IconFile));
                    }
                    if let Some(f) = s.icon_file.clone() {
                        ui.add(egui::Label::new(egui::RichText::new(&f).weak()).truncate());
                        if ui.small_button(tr("Retirer")).clicked() {
                            s.icon_file = None;
                        }
                    }
                });
            });
            ui.end_row();

            ui.separator();
            ui.separator();
            ui.end_row();

            ui.label(tr("Raccourci global"));
            let mut hk = s.hotkey.clone().unwrap_or_default();
            let valid = hk.trim().is_empty() || hk.parse::<global_hotkey::hotkey::HotKey>().is_ok();
            let mut edit = egui::TextEdit::singleline(&mut hk)
                .hint_text(tr("désactivé"))
                .desired_width(180.0);
            if !valid {
                edit = edit.text_color(ui.visuals().error_fg_color);
            }
            if ui
                .add(edit)
                .on_hover_text(tr("Ex. Ctrl+Shift+Space, Ctrl+Alt+P"))
                .changed()
            {
                s.hotkey = (!hk.trim().is_empty()).then(|| hk.trim().to_string());
            }
            ui.end_row();

            ui.label(tr("Zone de notification"));
            ui.vertical(|ui| {
                ui.checkbox(&mut s.tray, tr("Icône dans la zone de notification"));
                ui.add_enabled(
                    s.tray,
                    egui::Checkbox::new(&mut s.close_to_tray, tr("La croix masque la fenêtre")),
                );
                ui.add_enabled(
                    s.tray,
                    egui::Checkbox::new(&mut s.start_hidden, tr("Démarrer masqué")),
                );
            });
            ui.end_row();

            ui.label(tr("Coin actif"));
            ui.vertical(|ui| {
                egui::ComboBox::from_id_salt("corner")
                    .selected_text(s.hot_corner.label())
                    .show_ui(ui, |ui| {
                        for c in Corner::ALL {
                            ui.selectable_value(&mut s.hot_corner, c, c.label());
                        }
                    });
                ui.add_enabled_ui(s.hot_corner != Corner::None, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut s.hot_corner_delay)
                                .range(0..=3000)
                                .suffix(" ms"),
                        );
                        ui.label(tr("délai"));
                        ui.add(
                            egui::DragValue::new(&mut s.hot_corner_size)
                                .range(1..=100)
                                .suffix(" px"),
                        );
                        ui.label(tr("taille du coin"));
                    });
                    ui.checkbox(&mut s.hot_corner_all_screens, tr("Sur tous les écrans"));
                });
            });
            ui.end_row();

            ui.label(tr("Rappels"));
            egui::ComboBox::from_id_salt("reminders")
                .selected_text(s.reminders.label())
                .show_ui(ui, |ui| {
                    for r in ReminderStyle::ALL {
                        ui.selectable_value(&mut s.reminders, r, r.label());
                    }
                });
            ui.end_row();

            ui.label(tr("Fenêtre"));
            ui.checkbox(&mut s.compact, tr("Mode compact (recherche et liste)"));
            ui.end_row();

            ui.label("Windows");
            ui.add_enabled(
                cfg!(windows),
                egui::Checkbox::new(&mut form.autostart, tr("Démarrer avec Windows")),
            );
            ui.end_row();

            ui.separator();
            ui.separator();
            ui.end_row();

            ui.label(egui::RichText::new(tr("Outils")).strong());
            ui.end_row();

            ui.label(tr("Sauvegarde"));
            ui.horizontal(|ui| {
                if ui
                    .button(tr("Exporter…"))
                    .on_hover_text(tr("PStart.xml, notes et réglages dans un fichier zip"))
                    .clicked()
                {
                    actions.push(Action::Browse(Browse::BackupExport));
                }
                if ui.button(tr("Restaurer…")).clicked() {
                    actions.push(Action::Browse(Browse::BackupRestore));
                }
            });
            ui.end_row();

            ui.label(tr("Chemins"));
            ui.horizontal_wrapped(|ui| {
                if ui.button(tr("Corriger une lettre de lecteur…")).clicked() {
                    actions.push(Action::OpenDriveFix);
                }
                if ui
                    .button(tr("Rendre les chemins portables"))
                    .on_hover_text(tr(
                        "Les chemins absolus sur le lecteur de PStart.xml deviennent relatifs",
                    ))
                    .clicked()
                {
                    actions.push(Action::MakePortable);
                }
            });
            ui.end_row();

            ui.label(tr("Programmes"));
            ui.vertical(|ui| {
                if ui
                    .button(tr("Rechercher des programmes…"))
                    .on_hover_text(tr(
                        "PortableApps.com, exécutables et raccourcis .lnk d'un dossier",
                    ))
                    .clicked()
                {
                    actions.push(Action::Browse(Browse::ScanFolder));
                }
                if let Some(dir) = &s.scan_folder {
                    ui.weak(trf("Dossier : {}", &[dir]));
                }
                ui.add_enabled(
                    s.scan_folder.is_some(),
                    egui::Checkbox::new(
                        &mut s.scan_on_start,
                        tr("Signaler les nouveaux programmes au démarrage"),
                    ),
                );
                if !s.scan_ignored.is_empty()
                    && ui
                        .small_button(trf(
                            "Oublier les {} programmes ignorés",
                            &[&s.scan_ignored.len()],
                        ))
                        .clicked()
                {
                    s.scan_ignored.clear();
                }
            });
            ui.end_row();

            ui.label(tr("Clé USB"));
            if ui.button(tr("Éjecter la clé…")).clicked() {
                actions.push(Action::AskEject);
            }
            ui.end_row();

            ui.label(tr("Journal"));
            ui.horizontal(|ui| {
                if ui.button(tr("Ouvrir le journal")).clicked() {
                    actions.push(Action::OpenLog);
                }
                ui.weak(crate::log::FILE_NAME);
            });
            ui.end_row();

            ui.label(tr("Mises à jour"));
            ui.vertical(|ui| {
                ui.checkbox(&mut s.check_updates, tr("Vérifier au démarrage"));
                ui.horizontal_wrapped(|ui| {
                    ui.weak(trf("Version {}", &[&update::current_version()]));
                    match upd {
                        update::State::Checking => {
                            ui.spinner();
                        }
                        update::State::Downloading | update::State::Installed => {
                            ui.spinner();
                            ui.label(tr("téléchargement…"));
                        }
                        update::State::UpToDate => {
                            ui.label(tr("à jour"));
                        }
                        update::State::Failed(e) => {
                            ui.colored_label(ui.visuals().warn_fg_color, e);
                        }
                        _ => {}
                    }
                });
                ui.horizontal(|ui| {
                    let busy = matches!(
                        upd,
                        update::State::Checking
                            | update::State::Downloading
                            | update::State::Installed
                    );
                    if let update::State::Available(r) = upd {
                        if ui
                            .add_enabled(
                                !busy,
                                egui::Button::new(trf("Installer la version {}", &[&r.version])),
                            )
                            .clicked()
                        {
                            actions.push(Action::InstallUpdate);
                        }
                        ui.hyperlink_to(tr("Nouveautés"), &r.page);
                    } else if ui
                        .add_enabled(!busy, egui::Button::new(tr("Vérifier maintenant")))
                        .clicked()
                    {
                        actions.push(Action::CheckUpdate);
                    }
                });
            });
            ui.end_row();
        });
    ui.add_space(6.0);
    ui.weak(tr(
        "Le raccourci global et l'icône de notification s'activent au prochain démarrage.",
    ));
    ui.weak(trf("Fichier : {}", &[&settings::FILE_NAME]));
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        if ui.button(tr("Enregistrer")).clicked() {
            actions.push(Action::SaveSettings);
        }
        if ui.button(tr("Annuler")).clicked() {
            actions.push(Action::CloseDialog);
        }
    });
}

/// Thème et taille du texte.
fn apply_appearance(ctx: &egui::Context, s: &Settings) {
    ctx.set_theme(match s.theme {
        Theme::System => egui::ThemePreference::System,
        Theme::Light => egui::ThemePreference::Light,
        Theme::Dark => egui::ThemePreference::Dark,
    });
    ctx.set_zoom_factor(s.text_size as f32 / 100.0);
}

/// Recalcule l'aperçu de l'icône quand les réglages changent.
fn refresh_preview(ctx: &egui::Context, form: &mut SettingsForm) {
    let s = &form.settings;
    let key: IconKey = (s.icon_shape, s.icon_color, s.icon_file.clone());
    if form.preview.as_ref().is_some_and(|(k, _)| *k == key) {
        return;
    }
    let img = s.icon(form.settings_path.as_deref());
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [img.width as usize, img.height as usize],
        &img.rgba,
    );
    let tex = ctx.load_texture("icon-preview", image, egui::TextureOptions::LINEAR);
    form.preview = Some((key, tex));
}

/// Pastille de couleur cliquable.
fn color_swatch(ui: &mut egui::Ui, color: IconColor, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click());
    let ([r, g, b], _) = color.gradient();
    let painter = ui.painter();
    painter.rect_filled(rect.shrink(2.0), 4.0, egui::Color32::from_rgb(r, g, b));
    if selected || resp.hovered() {
        let stroke = egui::Stroke::new(2.0, ui.visuals().strong_text_color());
        painter.rect_stroke(rect, 5.0, stroke, egui::StrokeKind::Inside);
    }
    resp.on_hover_text(color.label())
}

/// Raccourcis correspondant à la recherche, du plus pertinent au moins
/// pertinent (tolérance aux fautes, initiales, usage).
fn pstart_hits(cfg: &Config, query: &str) -> Vec<config::FlatEntry> {
    let mut hits: Vec<(i32, config::FlatEntry)> = cfg
        .flatten()
        .into_iter()
        .filter_map(|e| fuzzy::rank(query, &e.name, &e.target, e.runcount).map(|s| (s, e)))
        .collect();
    hits.sort_by_key(|h| std::cmp::Reverse(h.0));
    hits.into_iter().take(MAX_HITS).map(|(_, e)| e).collect()
}

fn start_hits<'a>(items: &'a [StartItem], query: &str) -> Vec<&'a StartItem> {
    let mut hits: Vec<(i32, &StartItem)> = items
        .iter()
        .filter_map(|i| fuzzy::score(query, &i.name).map(|s| (s, i)))
        .collect();
    hits.sort_by_key(|h| std::cmp::Reverse(h.0));
    hits.into_iter().take(MAX_HITS).map(|(_, i)| i).collect()
}

/// Icônes de groupe (chemins résolus) à charger.
fn group_icons(cfg: &Config) -> Vec<String> {
    fn walk(el: &Element, base: &str, out: &mut Vec<String>) {
        for c in el.children.iter().filter(|c| c.name == "files") {
            if let Some(i) = c.attr("icon").filter(|i| !i.trim().is_empty()) {
                out.push(paths::resolve(i, base));
            }
            walk(c, base, out);
        }
    }
    let mut out = Vec::new();
    if let Some(files) = cfg.files() {
        walk(files, &cfg.base_dir, &mut out);
    }
    out
}

/// Éjection de la clé : programmes ouverts, puis confirmation.
fn eject_ui(
    ui: &mut egui::Ui,
    drive: char,
    procs: &[eject::Proc],
    stubborn: bool,
    waiting: bool,
    actions: &mut Vec<Action>,
) {
    ui.heading(trf("Éjecter la clé ({}:)", &[&drive]));
    ui.add_space(8.0);
    if waiting {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(tr("Fermeture des programmes…"));
        });
        return;
    }
    if procs.is_empty() {
        ui.label(tr("Aucun programme n'est ouvert depuis la clé."));
    } else {
        ui.label(if stubborn {
            tr("Ces programmes ne se sont pas fermés (enregistrement en attente ?) :")
        } else {
            tr("Programmes ouverts depuis la clé :")
        });
        egui::ScrollArea::vertical()
            .max_height(180.0)
            .show(ui, |ui| {
                for p in procs {
                    ui.label(format!("• {}", p.name())).on_hover_text(&p.path);
                }
            });
    }
    ui.add_space(6.0);
    ui.weak(tr("PStart-rs se fermera, puis Windows éjectera la clé."));
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        let label = if procs.is_empty() {
            tr("Éjecter")
        } else {
            tr("Fermer les programmes et éjecter")
        };
        if ui.button(label).clicked() {
            actions.push(Action::Eject { force: false });
        }
        if stubborn
            && ui
                .button(tr("Forcer la fermeture"))
                .on_hover_text(tr("Les modifications non enregistrées seront perdues"))
                .clicked()
        {
            actions.push(Action::Eject { force: true });
        }
        if ui.button(tr("Annuler")).clicked() {
            actions.push(Action::CloseDialog);
        }
    });
}

/// Ajout des programmes trouvés dans un dossier.
fn import_ui(
    ui: &mut egui::Ui,
    items: &mut [(bool, scan::Found)],
    by_category: &mut bool,
    group: &mut String,
    actions: &mut Vec<Action>,
) {
    ui.heading(trf("{} programmes trouvés", &[&items.len()]));
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui.small_button(tr("Tout cocher")).clicked() {
            items.iter_mut().for_each(|i| i.0 = true);
        }
        if ui.small_button(tr("Tout décocher")).clicked() {
            items.iter_mut().for_each(|i| i.0 = false);
        }
    });
    egui::ScrollArea::vertical()
        .max_height(260.0)
        .show(ui, |ui| {
            for (on, f) in items.iter_mut() {
                ui.horizontal(|ui| {
                    ui.checkbox(on, &f.name).on_hover_text(&f.path);
                    if let Some(c) = &f.category {
                        ui.weak(format!("({c})"));
                    }
                });
            }
        });
    ui.add_space(6.0);
    let has_categories = items.iter().any(|(_, f)| f.category.is_some());
    ui.add_enabled(
        has_categories,
        egui::Checkbox::new(by_category, tr("Un groupe par catégorie (ou dossier)")),
    );
    ui.horizontal(|ui| {
        ui.label(if *by_category && has_categories {
            tr("Groupe pour les autres :")
        } else {
            tr("Groupe :")
        });
        ui.text_edit_singleline(group);
    });
    ui.add_space(12.0);
    let chosen: Vec<scan::Found> = items
        .iter()
        .filter(|(on, _)| *on)
        .map(|(_, f)| f.clone())
        .collect();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !chosen.is_empty(),
                egui::Button::new(tr("Ajouter la sélection")),
            )
            .clicked()
        {
            actions.push(Action::Import {
                items: chosen.clone(),
                by_category: *by_category,
                group: group.clone(),
            });
        }
        if ui
            .add_enabled(
                !chosen.is_empty(),
                egui::Button::new(tr("Ignorer la sélection")),
            )
            .on_hover_text(tr("Ne plus signaler ces programmes"))
            .clicked()
        {
            actions.push(Action::IgnoreFound(
                chosen.iter().map(|f| f.path.clone()).collect(),
            ));
        }
        if ui.button(tr("Plus tard")).clicked() {
            actions.push(Action::CloseDialog);
        }
    });
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
    /// Lignes dessinées, dans l'ordre (navigation au clavier).
    visible: &'a RefCell<Vec<ItemPath>>,
    open: &'a HashSet<String>,
    scroll_to: bool,
}

/// Clé d'un groupe : la suite des noms depuis la racine.
fn group_key(names: &[String]) -> String {
    names.join("\u{1f}")
}

/// Clé du groupe situé à `path`.
fn group_key_at(cfg: &Config, path: &[usize]) -> Option<String> {
    let mut names = Vec::new();
    for n in 1..=path.len() {
        let el = cfg.item(&path[..n])?;
        if el.name != "files" {
            return None;
        }
        names.push(config::display_name(el).to_string());
    }
    Some(group_key(&names))
}

impl Rows<'_> {
    fn tree(
        &self,
        ui: &mut egui::Ui,
        cfg: &Config,
        el: &Element,
        path: &mut ItemPath,
        keys: &mut Vec<String>,
        actions: &mut Vec<Action>,
    ) {
        for (i, child) in el.children.iter().enumerate() {
            path.push(i);
            match child.name.as_str() {
                "files" => {
                    self.visible.borrow_mut().push(path.clone());
                    keys.push(config::display_name(child).to_string());
                    let key = group_key(keys);
                    let count = edit::count_entries(child);
                    let title = format!("{}  ({count})", config::display_name(child));
                    let open = self.open.contains(&key);
                    let selected = self.selected.as_deref() == Some(path.as_slice());
                    let icon = child
                        .attr("icon")
                        .filter(|i| !i.trim().is_empty())
                        .map(|i| paths::resolve(i, &cfg.base_dir))
                        .and_then(|t| self.textures.get(&t));
                    let id = ui.make_persistent_id(("pstart-group", path.clone()));
                    let mut state =
                        egui::collapsing_header::CollapsingState::load_with_default_open(
                            ui.ctx(),
                            id,
                            open,
                        );
                    state.set_open(open);
                    let (toggle, header, _) = state
                        .show_header(ui, |ui| {
                            if let Some(tex) = icon {
                                ui.add(icon_image(tex, ICON_SIZE));
                            }
                            ui.selectable_label(selected, title)
                        })
                        .body(|ui| {
                            self.tree(ui, cfg, child, path, keys, actions);
                        });
                    keys.pop();
                    let header = header.inner.interact(egui::Sense::drag());
                    if selected && self.scroll_to {
                        header.scroll_to_me(None);
                    }
                    if toggle.clicked() || header.clicked() {
                        actions.push(Action::ToggleGroup(key));
                    }
                    self.group_header(ui, &header, path, actions);
                }
                "separator" => {
                    let resp = ui
                        .add(egui::Separator::default().spacing(10.0))
                        .interact(egui::Sense::click());
                    resp.context_menu(|ui| {
                        if ui.button(tr("Supprimer le séparateur")).clicked() {
                            actions.push(Action::Delete(path.clone()));
                            ui.close();
                        }
                    });
                    self.drop_target(ui, &resp, path, actions);
                }
                "file" => {
                    self.visible.borrow_mut().push(path.clone());
                    let target = cfg.resolve(child);
                    let favorite = EntryOptions::of(child).favorite;
                    let resp = self.row(
                        ui,
                        config::display_name(child),
                        &target,
                        None,
                        path,
                        true,
                        favorite,
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
            if ui.button(tr("▶ Tout lancer")).clicked() {
                actions.push(Action::LaunchGroup {
                    path: path.to_vec(),
                    confirmed: false,
                });
                ui.close();
            }
            ui.separator();
            if ui.button(tr("Nouveau raccourci ici…")).clicked() {
                actions.push(Action::NewEntry(Some(path.to_vec())));
                ui.close();
            }
            if ui.button(tr("Nouveau groupe ici…")).clicked() {
                actions.push(Action::NewGroup(Some(path.to_vec())));
                ui.close();
            }
            if ui.button(tr("Nouveau lien web ici…")).clicked() {
                actions.push(Action::NewLink(Some(path.to_vec())));
                ui.close();
            }
            if ui.button(tr("Séparateur ici")).clicked() {
                actions.push(Action::NewSeparator(Some(path.to_vec())));
                ui.close();
            }
            ui.separator();
            if ui.button(tr("Renommer…")).clicked() {
                actions.push(Action::Edit(path.to_vec()));
                ui.close();
            }
            if ui.button(tr("Supprimer…")).clicked() {
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
        self.visible.borrow_mut().push(e.path.clone());
        self.row(
            ui,
            &e.name,
            &e.target,
            Some(&info),
            &e.path,
            false,
            e.favorite,
            actions,
        );
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
        favorite: bool,
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
                resp = resp.on_hover_text(tr("Cible introuvable"));
            }
            if is_selected && self.scroll_to {
                resp.scroll_to_me(None);
            }
            if favorite {
                ui.weak("★");
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
                if ui.button(tr("Lancer")).clicked() {
                    actions.push(Action::Launch(path.to_vec()));
                    ui.close();
                }
                if ui.button(tr("Ouvrir l'emplacement")).clicked() {
                    actions.push(Action::Reveal(path.to_vec()));
                    ui.close();
                }
                let pin = if favorite {
                    tr("Retirer des favoris")
                } else {
                    tr("Ajouter aux favoris")
                };
                if ui.button(pin).clicked() {
                    actions.push(Action::ToggleFavorite(path.to_vec()));
                    ui.close();
                }
                ui.separator();
                if ui.button(tr("Modifier…")).clicked() {
                    actions.push(Action::Edit(path.to_vec()));
                    ui.close();
                }
                if ui.button(tr("Supprimer…")).clicked() {
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
                if ui.button(tr("Lancer")).clicked() {
                    actions.push(launch());
                    ui.close();
                }
                if ui.button(tr("Ajouter à PStart")).clicked() {
                    actions.push(Action::AddExternal {
                        name: item.name.clone(),
                        path: item.path.clone(),
                    });
                    ui.close();
                }
                if ui.button(tr("Ouvrir l'emplacement")).clicked() {
                    actions.push(Action::RevealPath(item.path.clone()));
                    ui.close();
                }
            });
        });
    }
}
