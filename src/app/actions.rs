//! Application des actions demandées par l'interface.

use super::*;

impl PLaunchApp {
    pub(super) fn apply(&mut self, action: Action) {
        match action {
            Action::Select(p) => {
                self.multi.clear();
                self.selected = Some(p);
            }
            Action::SelectMulti { path, range } => self.select_multi(path, range),
            Action::ClearMulti => self.multi.clear(),
            Action::LaunchNew(p) => {
                self.selected = Some(p.clone());
                self.shared.launch_mode(&p, true);
            }
            Action::Duplicate(p) => {
                if let Some(copy) = self.shared.modify(|c| edit::duplicate(c, &p)) {
                    self.reveal(&copy);
                    self.shared.set_status(tr("Raccourci dupliqué"));
                }
            }
            Action::LaunchMany(paths) => {
                let entries: Vec<ItemPath> = {
                    let guard = self.shared.config.lock().unwrap();
                    let Some(cfg) = guard.as_ref() else { return };
                    paths
                        .into_iter()
                        .filter(|p| cfg.item(p).is_some_and(|e| e.name == "file"))
                        .collect()
                };
                for p in &entries {
                    self.shared.launch(p);
                }
                self.shared
                    .set_status(trf("{} programmes lancés", &[&entries.len()]));
            }
            Action::PinMany(paths, on) => {
                if let Some(n) = self.shared.modify(|c| edit::set_favorites(c, &paths, on)) {
                    self.shared.set_status(if on {
                        trf("{} raccourcis ajoutés aux favoris", &[&n])
                    } else {
                        trf("{} raccourcis retirés des favoris", &[&n])
                    });
                }
            }
            Action::MoveMany { paths, target } => {
                if let Some(n) = self.shared.modify(|c| edit::move_many(c, paths, &target)) {
                    self.multi.clear();
                    self.reveal(&target);
                    let place = {
                        let guard = self.shared.config.lock().unwrap();
                        guard
                            .as_ref()
                            .map(|c| group_label(c, &target))
                            .unwrap_or_default()
                    };
                    self.shared
                        .set_status(trf("{} éléments déplacés dans « {} »", &[&n, &place]));
                }
            }
            Action::AskDeleteMany(paths) => {
                if !paths.is_empty() {
                    self.dialog = Some(Dialog::ConfirmDeleteMany(paths));
                }
            }
            Action::Rollback => match update::rollback() {
                Ok(v) => {
                    self.shared
                        .set_status(trf("Retour à la version {} : redémarrage…", &[&v]));
                    self.flush_notes();
                    crate::monitor::save_usage(&self.shared);
                    match update::relaunch() {
                        Ok(()) => tray::quit(&self.shared),
                        Err(e) => self.shared.error(e),
                    }
                }
                Err(e) => self.shared.error(e),
            },
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
                    self.multi.clear();
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
                let created = self.shared.modify(|c| {
                    let (parent, index) = edit::insertion_point(c, selected.as_deref());
                    edit::insert(c, &parent, index, NewItem::Separator)
                });
                if let Some(p) = created {
                    let parent = p[..p.len() - 1].to_vec();
                    self.reveal(&parent);
                    self.selected = selected;
                    let place = self.location_label(&p);
                    self.shared
                        .set_status(trf("Séparateur ajouté dans « {} »", &[&place]));
                }
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
            Action::MoveFavorite { from, to } => {
                let order = {
                    let guard = self.shared.config.lock().unwrap();
                    let Some(cfg) = guard.as_ref() else { return };
                    let mut paths: Vec<ItemPath> =
                        cfg.favorites().into_iter().map(|e| e.path).collect();
                    if from >= paths.len() || to >= paths.len() || from == to {
                        return;
                    }
                    let p = paths.remove(from);
                    paths.insert(to, p);
                    paths
                };
                self.shared.modify(|c| edit::reorder_favorites(c, &order));
            }
            Action::CreateConfig(path) => self.create_config(&path),
            Action::OpenLog => {
                let path = crate::log::path();
                if let Err(e) = launcher::launch(&path.to_string_lossy(), None) {
                    self.shared.error(e);
                }
            }
        }
    }

    /// Ctrl+clic : ajoute ou retire ; Maj+clic : toute la plage depuis la
    /// sélection courante (dans l'ordre affiché).
    fn select_multi(&mut self, path: ItemPath, range: bool) {
        if self.multi.is_empty() {
            if let Some(s) = &self.selected {
                self.multi.push(s.clone());
            }
        }
        if range {
            let visible = self.visible.borrow().clone();
            let anchor = self
                .selected
                .as_ref()
                .and_then(|s| visible.iter().position(|p| p == s));
            let end = visible.iter().position(|p| *p == path);
            if let (Some(a), Some(b)) = (anchor, end) {
                let (a, b) = (a.min(b), a.max(b));
                self.multi = visible[a..=b].to_vec();
                return;
            }
        }
        if let Some(i) = self.multi.iter().position(|p| *p == path) {
            self.multi.remove(i);
        } else {
            self.multi.push(path.clone());
        }
        self.selected = Some(path);
    }

    /// Montre un élément : groupes parents ouverts, sélection, défilement.
    pub(super) fn reveal(&mut self, path: &[usize]) {
        {
            let guard = self.shared.config.lock().unwrap();
            if let Some(cfg) = guard.as_ref() {
                for n in 1..path.len() {
                    if let Some(k) = group_key_at(cfg, &path[..n]) {
                        self.open_groups.insert(k);
                    }
                }
            }
        }
        self.search.clear();
        self.view = View::Tree;
        if !path.is_empty() {
            self.selected = Some(path.to_vec());
        }
        self.scroll_to_selected = true;
    }

    /// « Racine » ou « Groupe › Sous-groupe » de l'élément.
    pub(super) fn location_label(&self, path: &[usize]) -> String {
        let guard = self.shared.config.lock().unwrap();
        let Some(cfg) = guard.as_ref() else {
            return String::new();
        };
        group_label(cfg, &path[..path.len().saturating_sub(1)])
    }

    pub(super) fn undo_redo(&self, redo: bool) {
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
    pub(super) fn import(&mut self, items: Vec<scan::Found>, by_category: bool, group: String) {
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

    pub(super) fn ask_eject(&mut self) {
        let Some(drive) = self.shared.eject_drive() else {
            self.shared
                .set_status(tr("Le lecteur de PLaunch ne peut pas être éjecté"));
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
    pub(super) fn start_eject(&mut self, force: bool) {
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
    pub(super) fn poll_eject(&mut self) {
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
                        crate::log::info("éjection lancée, fermeture de PLaunch");
                        tray::quit(&self.shared);
                    }
                    Err(e) => self.shared.error(e),
                }
            }
        }
    }

    pub(super) fn restore(&mut self, zip: &Path) {
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
    pub(super) fn add_entries(&mut self, items: Vec<(String, String)>) {
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
            self.reveal(&p);
            self.shared.set_status(match count {
                1 => tr("Raccourci ajouté").to_string(),
                n => trf("{} raccourcis ajoutés", &[&n]),
            });
            for t in targets {
                self.refresh_target(t);
            }
        }
    }

    pub(super) fn open_form(&mut self, at: Option<ItemPath>, group: bool) {
        let (parent, index) = {
            let guard = self.shared.config.lock().unwrap();
            let Some(cfg) = guard.as_ref() else { return };
            let sel = at.as_deref().or(self.selected.as_deref());
            edit::insertion_point(cfg, sel)
        };
        let groups = {
            let guard = self.shared.config.lock().unwrap();
            guard.as_ref().map(group_choices).unwrap_or_default()
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
            groups,
            error: None,
            focus: true,
        }));
    }

    pub(super) fn open_edit(&mut self, path: ItemPath) {
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
                    groups: Vec::new(),
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
                    groups: Vec::new(),
                    error: None,
                    focus: true,
                }
            }
        };
        self.dialog = Some(Dialog::Form(form));
    }

    pub(super) fn ask_delete(&mut self, path: ItemPath) {
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
    pub(super) fn browse(&mut self, kind: Browse) {
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
            Browse::NewConfig => {
                let dir = std::env::current_exe()
                    .ok()
                    .and_then(|e| e.parent().map(Path::to_path_buf))
                    .unwrap_or_default();
                let picked = rfd::FileDialog::new()
                    .set_title(tr("Créer un nouveau PLaunch.xml"))
                    .set_directory(&dir)
                    .set_file_name("PLaunch.xml")
                    .add_filter("PLaunch", &["xml"])
                    .save_file();
                if let Some(p) = picked {
                    self.create_config(&p);
                }
            }
            Browse::OpenConfig => {
                let dir = std::env::current_exe()
                    .ok()
                    .and_then(|e| e.parent().map(Path::to_path_buf))
                    .unwrap_or_default();
                let picked = rfd::FileDialog::new()
                    .set_title(tr("Ouvrir un PLaunch.xml…"))
                    .set_directory(&dir)
                    .add_filter("PLaunch", &["xml"])
                    .pick_file();
                if let Some(p) = picked {
                    if self.settings_path.is_none() {
                        let sp = settings::path_for(&p);
                        self.settings = settings::load(&sp);
                        self.settings_path = Some(sp);
                    }
                    self.open(&p);
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

    pub(super) fn submit_form(&mut self) {
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
            let created = matches!(
                form.kind,
                FormKind::NewEntry { .. } | FormKind::NewGroup { .. }
            );
            self.dialog = None;
            self.reveal(&p);
            if created {
                let place = self.location_label(&p);
                self.shared.set_status(trf("Créé dans « {} »", &[&place]));
            } else {
                self.shared.set_status(tr("Enregistré"));
            }
            if !is_group {
                let target = self.resolve_path(&path);
                self.refresh_target(target);
            }
        }
    }

    // ---- Dessin -------------------------------------------------------
}
