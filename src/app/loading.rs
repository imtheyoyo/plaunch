//! Chargement du fichier, réglages, mises à jour, icônes et tâches de fond.

use super::*;

impl PLaunchApp {
    /// Surveille PLaunch.xml : une modification par un autre programme
    /// (synchronisation, autre PC, PLaunch) déclenche un rechargement.
    pub(super) fn watch_file(&self) {
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
    pub(super) fn scan_dir(&self) -> Option<PathBuf> {
        let raw = self.settings.scan_folder.as_deref()?;
        let resolved = paths::resolve(raw, &self.base_dir());
        Some(paths::to_native(&resolved))
    }

    /// Cherche les programmes absents de PLaunch.xml dans `dir`.
    /// `quiet` : au démarrage, ne rien afficher s'il n'y a rien.
    pub(super) fn start_scan(&mut self, dir: PathBuf, quiet: bool) {
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

    pub(super) fn poll_scan(&mut self) {
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
        // Affichage immédiat (au démarrage aussi), sauf si une autre
        // fenêtre est ouverte : le bouton de la barre d'état reste alors.
        let _ = quiet;
        if self.dialog.is_none() || matches!(self.dialog, Some(Dialog::Settings(_))) {
            self.apply(Action::ShowFound);
        }
    }

    pub(super) fn scan_start_menu(&self) {
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

    pub(super) fn open(&mut self, file: &Path) {
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
                *self.shared.usage.lock().unwrap() =
                    crate::usage::load(&crate::usage::path_for(&cfg.file));
                self.multi.clear();
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
                let dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
                self.drive_warnings = crate::drivecheck::check(&dir);
                for w in &self.drive_warnings {
                    crate::log::error(w);
                }
            }
            Err(e) => self.load_error = Some(e),
        }
    }

    /// Lance les raccourcis marqués « au démarrage » (`<autorun>`).
    pub(super) fn run_autorun(&self) {
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
    pub(super) fn check_drive(&mut self) {
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

    pub(super) fn update_autostart(&self) {
        let file = self.config_file();
        if let Err(e) = autostart::set(true, file.as_deref()) {
            self.shared.error(trf("Démarrage avec Windows : {}", &[&e]));
        }
    }

    pub(super) fn config_file(&self) -> Option<PathBuf> {
        self.shared
            .config
            .lock()
            .unwrap()
            .as_ref()
            .map(|c| c.file.clone())
    }

    /// Enregistre les réglages courants (hors fenêtre de réglages).
    pub(super) fn persist_settings(&self) {
        if let Some(path) = &self.settings_path {
            if let Err(e) = self.settings.save(path) {
                self.shared.error(e);
            }
        }
    }

    /// Vérifie en tâche de fond si une nouvelle version est publiée.
    pub(super) fn check_update(&self, quiet: bool) {
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

    pub(super) fn install_update(&self) {
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
    pub(super) fn toggle_compact(&mut self, ctx: &egui::Context) {
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
    pub(super) fn favorites(cfg: &Config) -> Vec<config::FlatEntry> {
        cfg.favorites()
    }

    /// Enregistre les notes si elles ont été modifiées.
    pub(super) fn flush_notes(&mut self) {
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

    pub(super) fn save_settings(&mut self) {
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
        self.shared
            .reuse_running
            .store(new.reuse_running, Ordering::SeqCst);
        self.shared
            .track_usage
            .store(new.track_usage, Ordering::SeqCst);
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
    pub(super) fn apply_icon(&mut self) -> Option<String> {
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

    pub(super) fn reload(&mut self) {
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

    pub(super) fn check_existence(&self, targets: Vec<String>) {
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
    pub(super) fn refresh_target(&self, target: String) {
        self.check_existence(vec![target.clone()]);
        let generation = self.shared.generation.load(Ordering::SeqCst);
        icons::spawn_loader(vec![target], self.shared.clone(), generation);
    }

    pub(super) fn poll_existence(&mut self) {
        while let Ok((t, ok)) = self.exists_rx.try_recv() {
            self.exists.insert(t, ok);
        }
    }

    /// Transforme en textures les icônes nouvellement extraites.
    pub(super) fn sync_textures(&mut self, ctx: &egui::Context) {
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

    pub(super) fn base_dir(&self) -> String {
        self.shared
            .config
            .lock()
            .unwrap()
            .as_ref()
            .map(|c| c.base_dir.clone())
            .unwrap_or_default()
    }

    pub(super) fn resolve_path(&self, raw: &str) -> String {
        paths::resolve(raw, &self.base_dir())
    }

    // ---- Application des actions --------------------------------------

    /// Fichier proposé à la création : celui demandé en argument s'il
    /// n'existe pas, sinon `PLaunch.xml` à côté de l'exécutable.
    pub(super) fn new_config_path(&self) -> Option<PathBuf> {
        if let Some(p) = &self.requested_file {
            if !p.exists() {
                return Some(p.clone());
            }
        }
        let beside = std::env::current_exe().ok()?.parent()?.join("PLaunch.xml");
        (!beside.exists()).then_some(beside)
    }

    /// Crée un PLaunch.xml vide, l'ouvre et y rattache les réglages.
    pub(super) fn create_config(&mut self, path: &Path) {
        if let Err(e) = Config::create_empty(path) {
            self.shared.error(e);
            return;
        }
        crate::log::info(&format!("nouveau fichier créé : {}", path.display()));
        if self.settings_path.is_none() {
            let sp = settings::path_for(path);
            self.settings_path = Some(sp.clone());
            if sp.exists() {
                self.settings = settings::load(&sp);
            } else {
                self.persist_settings();
            }
        }
        self.requested_file = Some(path.to_path_buf());
        self.open(path);
        self.shared.set_status(tr(
            "Nouveau fichier créé : ajoutez vos programmes avec ＋ ou en les déposant ici",
        ));
    }
}

/// Thème et taille du texte.
pub(super) fn apply_appearance(ctx: &egui::Context, s: &Settings) {
    ctx.set_theme(match s.theme {
        Theme::System => egui::ThemePreference::System,
        Theme::Light => egui::ThemePreference::Light,
        Theme::Dark => egui::ThemePreference::Dark,
    });
    ctx.set_zoom_factor(s.text_size as f32 / 100.0);
    crate::menutheme::apply(s.theme);
}
