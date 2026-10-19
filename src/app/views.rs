//! Barre du haut, favoris, vues centrales, accueil et panneau de détails.

use super::*;

impl PStartApp {
    pub(super) fn top_bar(
        &mut self,
        ui: &mut egui::Ui,
        actions: &mut Vec<Action>,
        cfg: Option<&Config>,
    ) {
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
    pub(super) fn favorites_bar(&self, ui: &mut egui::Ui, cfg: &Config, actions: &mut Vec<Action>) {
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
                let resp = resp
                    .on_hover_text(format!("{tip}\n{}", tr("Glisser pour déplacer")))
                    .interact(egui::Sense::drag());
                if resp.clicked() {
                    actions.push(Action::Launch(e.path.clone()));
                }
                // Glisser-déposer pour changer l'ordre.
                resp.dnd_set_drag_payload(FavDrag(i));
                if let Some(from) = resp.dnd_hover_payload::<FavDrag>() {
                    if from.0 != i {
                        let x = if from.0 < i {
                            resp.rect.right()
                        } else {
                            resp.rect.left()
                        };
                        ui.painter()
                            .vline(x, resp.rect.y_range(), ui.visuals().selection.stroke);
                    }
                }
                if let Some(from) = resp.dnd_release_payload::<FavDrag>() {
                    actions.push(Action::MoveFavorite {
                        from: from.0,
                        to: i,
                    });
                }
                let count = favorites.len();
                resp.context_menu(|ui| {
                    if ui
                        .add_enabled(i > 0, egui::Button::new(tr("◀ Déplacer à gauche")))
                        .clicked()
                    {
                        actions.push(Action::MoveFavorite { from: i, to: i - 1 });
                        ui.close();
                    }
                    if ui
                        .add_enabled(i + 1 < count, egui::Button::new(tr("Déplacer à droite ▶")))
                        .clicked()
                    {
                        actions.push(Action::MoveFavorite { from: i, to: i + 1 });
                        ui.close();
                    }
                    ui.separator();
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

    pub(super) fn central(&self, ui: &mut egui::Ui, cfg: Option<&Config>) -> Vec<Action> {
        let mut actions = Vec::new();
        let Some(cfg) = cfg else {
            self.welcome(ui, &mut actions);
            return actions;
        };
        self.visible.borrow_mut().clear();
        let running: HashSet<String> = self
            .shared
            .running
            .lock()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let rows = Rows {
            multi: &self.multi,
            running: &running,
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

    pub(super) fn notes_view(&self, ui: &mut egui::Ui, cfg: &Config, actions: &mut Vec<Action>) {
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

    pub(super) fn search_results(
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

    pub(super) fn welcome(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let target = self.new_config_path();
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.heading("PStart-rs");
            ui.add_space(12.0);
            ui.label(tr("Aucun fichier PStart.xml n'est ouvert."));
            if let Some(e) = &self.load_error {
                ui.add_space(6.0);
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            ui.add_space(16.0);
            if let Some(path) = &target {
                if ui
                    .add(egui::Button::new(
                        egui::RichText::new(tr("＋ Créer un nouveau PStart.xml")).strong(),
                    ))
                    .on_hover_text(path.display().to_string())
                    .clicked()
                {
                    actions.push(Action::CreateConfig(path.clone()));
                }
                ui.weak(path.display().to_string());
                ui.add_space(8.0);
            }
            ui.horizontal(|ui| {
                // Centrage approximatif des deux boutons.
                ui.add_space((ui.available_width() - 330.0).max(0.0) / 2.0);
                if ui.button(tr("Créer ailleurs…")).clicked() {
                    actions.push(Action::Browse(Browse::NewConfig));
                }
                if ui.button(tr("Ouvrir un PStart.xml…")).clicked() {
                    actions.push(Action::Browse(Browse::OpenConfig));
                }
            });
            ui.add_space(16.0);
            ui.weak(tr(
                "Vous pouvez aussi glisser un fichier PStart.xml sur cette fenêtre,",
            ));
            ui.weak(tr("ou placer pstart-rs.exe à côté de votre PStart.xml."));
        });
    }

    pub(super) fn details(&self, ui: &mut egui::Ui, cfg: Option<&Config>) -> Vec<Action> {
        let mut actions = Vec::new();
        let Some(cfg) = cfg else {
            return actions;
        };
        if self.multi.len() > 1 {
            self.multi_details(ui, cfg, &mut actions);
            return actions;
        }
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
                let used = self
                    .shared
                    .usage
                    .lock()
                    .unwrap()
                    .get(&crate::usage::key(config::entry_path(el)))
                    .copied()
                    .unwrap_or(0);
                if used > 0 {
                    ui.label(tr("Utilisation"));
                    ui.label(crate::usage::format_duration(used));
                    ui.end_row();
                }
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

    /// Panneau de la sélection multiple : actions groupées.
    fn multi_details(&self, ui: &mut egui::Ui, cfg: &Config, actions: &mut Vec<Action>) {
        let paths = self.multi.clone();
        ui.heading(trf("{} éléments sélectionnés", &[&paths.len()]));
        egui::ScrollArea::vertical()
            .max_height(160.0)
            .show(ui, |ui| {
                for p in &paths {
                    if let Some(el) = cfg.item(p) {
                        let mark = if el.name == "files" { "📁" } else { "•" };
                        ui.weak(format!("{mark} {}", config::display_name(el)));
                    }
                }
            });
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button(tr("▶ Tout lancer")).clicked() {
                actions.push(Action::LaunchMany(paths.clone()));
            }
            if ui.button(tr("☆ Ajouter aux favoris")).clicked() {
                actions.push(Action::PinMany(paths.clone(), true));
            }
            if ui.button(tr("★ Retirer des favoris")).clicked() {
                actions.push(Action::PinMany(paths.clone(), false));
            }
        });
        ui.add_space(6.0);
        ui.label(tr("Déplacer vers"));
        egui::ComboBox::from_id_salt("move-many")
            .selected_text(tr("Choisir un groupe…"))
            .width(ui.available_width().min(240.0))
            .show_ui(ui, |ui| {
                for (target, label) in group_choices(cfg) {
                    if ui.selectable_label(false, label).clicked() {
                        actions.push(Action::MoveMany {
                            paths: paths.clone(),
                            target,
                        });
                    }
                }
            });
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button(tr("Supprimer…")).clicked() {
                actions.push(Action::AskDeleteMany(paths.clone()));
            }
            if ui.button(tr("Désélectionner")).clicked() {
                actions.push(Action::ClearMulti);
            }
        });
        ui.add_space(8.0);
        ui.weak(tr("Ctrl+clic : ajouter ou retirer, Maj+clic : plage"));
    }

    pub(super) fn shortcuts_help(&self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new(tr("Raccourcis")).strong());
        ui.weak(tr("Entrée : lancer"));
        ui.weak(tr("F2 : modifier, Suppr : supprimer"));
        ui.weak(tr("Ctrl+F : rechercher, Échap : effacer"));
        ui.weak(tr("Glisser-déposer : réorganiser"));
        ui.weak(tr("Flèches : naviguer, ouvrir ou fermer un groupe"));
        ui.weak(tr("Ctrl+1 à 9 : lancer un favori"));
        ui.weak(tr("Ctrl+Z / Ctrl+Y : annuler / rétablir"));
        ui.weak(tr("Ctrl+clic, Maj+clic : sélection multiple"));
        match &self.hotkey_info {
            Some(hk) => ui.weak(trf("{} : afficher / masquer", &[&hk])),
            None => ui.weak(tr("Raccourci global : désactivé")),
        };
        ui.add_space(8.0);
        ui.weak(trf("Réglages : {}", &[&settings::FILE_NAME]));
    }
}

/// Raccourcis correspondant à la recherche, du plus pertinent au moins
/// pertinent (tolérance aux fautes, initiales, usage).
pub(super) fn pstart_hits(cfg: &Config, query: &str) -> Vec<config::FlatEntry> {
    let mut hits: Vec<(i32, config::FlatEntry)> = cfg
        .flatten()
        .into_iter()
        .filter_map(|e| fuzzy::rank(query, &e.name, &e.target, e.runcount).map(|s| (s, e)))
        .collect();
    hits.sort_by_key(|h| std::cmp::Reverse(h.0));
    hits.into_iter().take(MAX_HITS).map(|(_, e)| e).collect()
}

pub(super) fn start_hits<'a>(items: &'a [StartItem], query: &str) -> Vec<&'a StartItem> {
    let mut hits: Vec<(i32, &StartItem)> = items
        .iter()
        .filter_map(|i| fuzzy::score(query, &i.name).map(|s| (s, i)))
        .collect();
    hits.sort_by_key(|h| std::cmp::Reverse(h.0));
    hits.into_iter().take(MAX_HITS).map(|(_, i)| i).collect()
}

/// Libellé d'un groupe : « (racine) » ou « A › B ».
pub(super) fn group_label(cfg: &Config, path: &[usize]) -> String {
    if path.is_empty() {
        return tr("(racine)").to_string();
    }
    (1..=path.len())
        .filter_map(|n| {
            cfg.item(&path[..n])
                .map(|e| config::display_name(e).to_string())
        })
        .collect::<Vec<_>>()
        .join(" › ")
}

/// Tous les groupes, pour choisir où créer un élément.
pub(super) fn group_choices(cfg: &Config) -> Vec<(ItemPath, String)> {
    fn walk(cfg: &Config, el: &Element, path: &mut ItemPath, out: &mut Vec<(ItemPath, String)>) {
        for (i, c) in el.children.iter().enumerate() {
            if c.name == "files" {
                path.push(i);
                out.push((path.clone(), group_label(cfg, path)));
                walk(cfg, c, path, out);
                path.pop();
            }
        }
    }
    let mut out = vec![(Vec::new(), tr("(racine)").to_string())];
    if let Some(files) = cfg.files() {
        walk(cfg, files, &mut Vec::new(), &mut out);
    }
    out
}

/// Icônes de groupe (chemins résolus) à charger.
pub(super) fn group_icons(cfg: &Config) -> Vec<String> {
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

pub(super) fn tag_color(
    opts: &SearchOptions,
    source: Option<Source>,
    ui: &egui::Ui,
) -> egui::Color32 {
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

pub(super) fn section(ui: &mut egui::Ui, title: &str, color: egui::Color32) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(title).small().strong().color(color));
}

pub(super) fn icon_image(tex: &egui::TextureHandle, size: f32) -> egui::Image<'static> {
    egui::Image::from_texture(egui::load::SizedTexture::from_handle(tex))
        .fit_to_exact_size(egui::vec2(size, size))
}
