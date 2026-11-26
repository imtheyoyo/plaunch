//! Barre du haut, favoris, vues centrales, accueil et panneau de détails.

use super::*;

impl PLaunchApp {
    pub(super) fn top_bar(
        &mut self,
        ui: &mut egui::Ui,
        actions: &mut Vec<Action>,
        cfg: Option<&Config>,
    ) {
        let compact = self.settings.compact;
        let tok = modern::tokens(ui.visuals());
        let can_eject = cfg.is_some() && self.shared.eject_drive().is_some();

        ui.horizontal(|ui| {
            if compact {
                // Mode compact : vues et boutons regroupés dans un menu.
                ui.menu_button(icon::LIST, |ui| {
                    for v in View::ALL {
                        if ui
                            .selectable_label(self.view == v, icon::with(v.icon(), v.label()))
                            .clicked()
                        {
                            self.view = v;
                            ui.close();
                        }
                    }
                    ui.separator();
                    if cfg.is_some() {
                        if ui
                            .button(icon::with(icon::PLUS, tr("Nouveau raccourci…")))
                            .clicked()
                        {
                            actions.push(Action::NewEntry(None));
                            ui.close();
                        }
                        if ui
                            .button(icon::with(icon::FOLDER, tr("Nouveau groupe…")))
                            .clicked()
                        {
                            actions.push(Action::NewGroup(None));
                            ui.close();
                        }
                        if ui.button(tr("Nouveau lien web…")).clicked() {
                            actions.push(Action::NewLink(None));
                            ui.close();
                        }
                        if ui.button(tr("Séparateur")).clicked() {
                            actions.push(Action::NewSeparator(None));
                            ui.close();
                        }
                        ui.separator();
                        let can_undo = !self.shared.undo.lock().unwrap().is_empty();
                        let can_redo = !self.shared.redo.lock().unwrap().is_empty();
                        if ui
                            .add_enabled(
                                can_undo,
                                egui::Button::new(icon::with(
                                    icon::ARROW_ARC_LEFT,
                                    tr("Annuler (Ctrl+Z)"),
                                )),
                            )
                            .clicked()
                        {
                            actions.push(Action::Undo);
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                can_redo,
                                egui::Button::new(icon::with(
                                    icon::ARROW_ARC_RIGHT,
                                    tr("Rétablir (Ctrl+Y)"),
                                )),
                            )
                            .clicked()
                        {
                            actions.push(Action::Redo);
                            ui.close();
                        }
                        if ui
                            .button(icon::with(
                                icon::ARROW_CLOCKWISE,
                                tr("Recharger le fichier"),
                            ))
                            .clicked()
                        {
                            actions.push(Action::Reload);
                            ui.close();
                        }
                        if can_eject
                            && ui
                                .button(icon::with(icon::EJECT, tr("Éjecter la clé…")))
                                .clicked()
                        {
                            actions.push(Action::AskEject);
                            ui.close();
                        }
                        ui.separator();
                    }
                    if ui
                        .button(icon::with(icon::GEAR_SIX, tr("Réglages")))
                        .clicked()
                    {
                        actions.push(Action::OpenSettings);
                        ui.close();
                    }
                    if ui
                        .button(icon::with(icon::ARROWS_OUT_SIMPLE, tr("Fenêtre complète")))
                        .clicked()
                    {
                        actions.push(Action::ToggleCompact);
                        ui.close();
                    }
                });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !compact {
                    if ui
                        .button(icon::GEAR_SIX)
                        .on_hover_text(tr("Réglages"))
                        .clicked()
                    {
                        actions.push(Action::OpenSettings);
                    }
                    if ui
                        .button(icon::ARROWS_IN_SIMPLE)
                        .on_hover_text(tr("Mode compact"))
                        .clicked()
                    {
                        actions.push(Action::ToggleCompact);
                    }
                    if can_eject
                        && ui
                            .button(icon::EJECT)
                            .on_hover_text(tr("Éjecter la clé…"))
                            .clicked()
                    {
                        actions.push(Action::AskEject);
                    }
                    if ui
                        .button(icon::ARROW_CLOCKWISE)
                        .on_hover_text(tr("Recharger le fichier"))
                        .clicked()
                    {
                        actions.push(Action::Reload);
                    }
                    let can_redo = !self.shared.redo.lock().unwrap().is_empty();
                    let can_undo = !self.shared.undo.lock().unwrap().is_empty();
                    if ui
                        .add_enabled(can_redo, egui::Button::new(icon::ARROW_ARC_RIGHT))
                        .on_hover_text(tr("Rétablir (Ctrl+Y)"))
                        .clicked()
                    {
                        actions.push(Action::Redo);
                    }
                    if ui
                        .add_enabled(can_undo, egui::Button::new(icon::ARROW_ARC_LEFT))
                        .on_hover_text(tr("Annuler (Ctrl+Z)"))
                        .clicked()
                    {
                        actions.push(Action::Undo);
                    }
                    ui.add_enabled_ui(cfg.is_some(), |ui| {
                        ui.menu_button(icon::PLUS, |ui| {
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

                // Omnibar de recherche moderne.
                let focused = self
                    .search_id
                    .is_some_and(|id| ui.ctx().memory(|m| m.has_focus(id)));
                let stroke = if focused {
                    egui::Stroke::new(1.5, tok.accent)
                } else {
                    egui::Stroke::new(1.0, tok.border)
                };
                egui::Frame::new()
                    .fill(tok.field)
                    .stroke(stroke)
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(icon::MAGNIFYING_GLASS).color(if focused {
                                    tok.accent
                                } else {
                                    tok.text_muted
                                }),
                            );
                            let clear_w = if self.search.is_empty() { 0.0 } else { 22.0 };
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut self.search)
                                    .frame(egui::Frame::NONE)
                                    .hint_text(tr("Rechercher… (Ctrl+F)"))
                                    .desired_width((ui.available_width() - clear_w).max(60.0)),
                            );
                            self.search_id = Some(resp.id);
                            if self.focus_search {
                                resp.request_focus();
                                self.focus_search = false;
                            }
                            if !self.search.is_empty()
                                && ui
                                    .add(egui::Button::new(icon::X).frame(false))
                                    .on_hover_text(tr("Effacer la recherche"))
                                    .clicked()
                            {
                                self.search.clear();
                                resp.request_focus();
                            }
                        });
                    });
            });
        });

        if !compact {
            ui.add_space(4.0);
            let alarms_count = self
                .shared
                .notes
                .lock()
                .unwrap()
                .as_ref()
                .map_or(0, |n| n.items.iter().filter(|i| i.alarm.is_some()).count());
            let missing_count = self
                .index
                .entries
                .iter()
                .filter(|e| self.exists.get(&e.entry.target) == Some(&false))
                .count();
            egui::Frame::new()
                .fill(tok.field)
                .corner_radius(8.0)
                .inner_margin(3.0)
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for v in View::ALL {
                            let active = self.view == v;
                            let mut text = icon::with(v.icon(), v.label());
                            if v == View::Notes && alarms_count > 0 {
                                text.push_str(&format!(" ({alarms_count})"));
                            } else if v == View::Stats && missing_count > 0 {
                                text.push_str(&format!(" ({missing_count})"));
                            }
                            let rich = if active {
                                egui::RichText::new(text).strong().color(tok.text_strong)
                            } else {
                                egui::RichText::new(text).color(tok.text_muted)
                            };
                            let btn = egui::Button::new(rich)
                                .fill(if active {
                                    tok.accent_soft
                                } else {
                                    egui::Color32::TRANSPARENT
                                })
                                .stroke(if active {
                                    egui::Stroke::new(1.0, tok.accent.gamma_multiply(0.45))
                                } else {
                                    egui::Stroke::NONE
                                })
                                .corner_radius(6.0);
                            if ui.add(btn).clicked() {
                                self.view = v;
                            }
                        }
                    });
                });
        }
        if let Some(cfg) = cfg {
            self.favorites_bar(ui, cfg, actions);
        }
        if !self.drive_warnings.is_empty() {
            ui.add_space(2.0);
            egui::Frame::new()
                .fill(ui.visuals().warn_fg_color.gamma_multiply(0.18))
                .corner_radius(6.0)
                .inner_margin(6.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            for w in &self.drive_warnings {
                                ui.colored_label(
                                    ui.visuals().warn_fg_color,
                                    icon::with(icon::WARNING, w),
                                );
                            }
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                            if ui
                                .small_button(icon::X)
                                .on_hover_text(tr("Masquer"))
                                .clicked()
                            {
                                actions.push(Action::DismissDriveWarnings);
                            }
                        });
                    });
                });
        }
    }

    /// Menu des profils (barre d'état) : changer, créer.
    pub(super) fn profile_menu(&self, ui: &mut egui::Ui, cfg: &Config, actions: &mut Vec<Action>) {
        let dir = cfg.file.parent().map(Path::to_path_buf).unwrap_or_default();
        let current = crate::profiles::display_name(&cfg.file);
        ui.menu_button(format!("📂 {current}"), |ui| {
            for p in crate::profiles::list(&dir) {
                let selected = p == cfg.file;
                if ui
                    .selectable_label(selected, crate::profiles::display_name(&p))
                    .clicked()
                {
                    if !selected {
                        actions.push(Action::SwitchProfile(p));
                    }
                    ui.close();
                }
            }
            ui.separator();
            if ui.button(tr("Nouveau profil…")).clicked() {
                actions.push(Action::AskNewProfile);
                ui.close();
            }
        })
        .response
        .on_hover_text(tr("Profil : fichier de raccourcis utilisé"));
    }

    /// Barre des favoris : icônes (ou initiales) cliquables dans un dock épuré.
    pub(super) fn favorites_bar(&self, ui: &mut egui::Ui, cfg: &Config, actions: &mut Vec<Action>) {
        let favorites = &self.index.favorites;
        let groups = &self.index.favorite_groups;
        if favorites.is_empty() && groups.is_empty() {
            return;
        }
        let tok = modern::tokens(ui.visuals());
        let running = self.shared.running.lock().unwrap();
        let ctrl_held = ui.input(|i| i.modifiers.command);
        ui.add_space(4.0);
        egui::Frame::new()
            .fill(tok.card)
            .stroke(egui::Stroke::new(1.0, tok.border))
            .corner_radius(8.0)
            .inner_margin(egui::Margin::symmetric(8, 5))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    for (i, e) in favorites.iter().enumerate() {
                        let tip = if i < 9 {
                            format!("{}  (Ctrl+{})", e.name, i + 1)
                        } else {
                            e.name.clone()
                        };
                        let is_running = running.contains_key(&crate::eject::path_key(&e.target));
                        let resp = match self.textures.get(&e.target) {
                            Some(tex) => ui.add(
                                egui::Button::image(icon_image(tex, 20.0))
                                    .min_size(egui::vec2(30.0, 30.0)),
                            ),
                            None => {
                                let initial: String = e.name.chars().take(2).collect();
                                ui.add(egui::Button::new(initial).min_size(egui::vec2(30.0, 30.0)))
                            }
                        };
                        if is_running {
                            let dot = egui::pos2(resp.rect.right() - 5.0, resp.rect.top() + 5.0);
                            ui.painter().circle_filled(dot, 3.0, tok.success);
                        }
                        if ctrl_held && i < 9 {
                            ui.painter().text(
                                egui::pos2(resp.rect.right() - 3.0, resp.rect.bottom() - 2.0),
                                egui::Align2::RIGHT_BOTTOM,
                                (i + 1).to_string(),
                                egui::FontId::proportional(9.0),
                                tok.accent,
                            );
                        }
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
                                ui.painter().vline(
                                    x,
                                    resp.rect.y_range(),
                                    ui.visuals().selection.stroke,
                                );
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
                                .add_enabled(
                                    i + 1 < count,
                                    egui::Button::new(tr("Déplacer à droite ▶")),
                                )
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
                    // Groupes épinglés : menu déroulant de leur contenu.
                    for g in groups {
                        let Some(el) = cfg.item(g) else { continue };
                        let label = icon::with(
                            icon::FOLDER,
                            format!("{} {}", config::display_name(el), icon::CARET_DOWN),
                        );
                        let resp = ui
                            .menu_button(label, |ui| {
                                if ui.button(tr("▶ Tout lancer")).clicked() {
                                    actions.push(Action::LaunchGroup {
                                        path: g.clone(),
                                        confirmed: false,
                                    });
                                    ui.close();
                                }
                                ui.separator();
                                self.group_menu(ui, cfg, el, g, actions);
                                ui.separator();
                                if ui.button(tr("Retirer de la barre")).clicked() {
                                    actions.push(Action::ToggleGroupFavorite(g.clone()));
                                    ui.close();
                                }
                            })
                            .response;
                        resp.on_hover_text(tr("Groupe épinglé : cliquer pour ouvrir"));
                    }
                });
            });
    }

    /// Contenu d'un groupe en menus imbriqués (barre des favoris).
    fn group_menu(
        &self,
        ui: &mut egui::Ui,
        cfg: &Config,
        el: &Element,
        path: &[usize],
        actions: &mut Vec<Action>,
    ) {
        if !el
            .children
            .iter()
            .any(|c| c.name == "file" || c.name == "files")
        {
            ui.weak(tr("(vide)"));
            return;
        }
        for (i, c) in el.children.iter().enumerate() {
            let mut p = path.to_vec();
            p.push(i);
            match c.name.as_str() {
                "files" => {
                    ui.menu_button(format!("📁 {}", config::display_name(c)), |ui| {
                        self.group_menu(ui, cfg, c, &p, actions);
                    });
                }
                "file" => {
                    let target = cfg.resolve(c);
                    let name = config::display_name(c);
                    let button = match self.textures.get(&target) {
                        Some(tex) => egui::Button::image_and_text(icon_image(tex, 16.0), name),
                        None => egui::Button::new(name),
                    };
                    if ui.add(button).clicked() {
                        actions.push(Action::Launch(p));
                        ui.close();
                    }
                }
                "separator" => {
                    ui.separator();
                }
                _ => {}
            }
        }
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
        let usage = self.shared.usage.lock().unwrap().clone();
        let rows = Rows {
            cfg,
            usage: &usage,
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
                            if files.children.is_empty() {
                                ui.add_space(12.0);
                                ui.weak(tr(
                                    "Aucun raccourci : ajoutez des programmes avec ＋ ou en les déposant ici.",
                                ));
                            }
                            rows.tree(ui, cfg, files, &mut path, &mut keys, &mut actions);
                            rows.end_drop_zone(ui, files.children.len(), &mut actions);
                        }
                    }
                    View::MostUsed => {
                        for e in self.index.most_used.iter().take(40) {
                            rows.flat(ui, e, false, &mut actions);
                        }
                    }
                    View::Recent => {
                        for e in self.index.recent.iter().take(40) {
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
        let two_cols = ui.available_width() >= 480.0 && notes.items.len() > 1;
        if two_cols {
            let mut left = Vec::new();
            let mut right = Vec::new();
            for (i, n) in notes.items.iter_mut().enumerate() {
                if i % 2 == 0 {
                    left.push((i, n));
                } else {
                    right.push((i, n));
                }
            }
            ui.columns(2, |cols| {
                for (i, note) in left {
                    let (c, del) = note_card(&mut cols[0], note, &palette);
                    changed |= c;
                    if del {
                        delete = Some(i);
                    }
                    cols[0].add_space(8.0);
                }
                for (i, note) in right {
                    let (c, del) = note_card(&mut cols[1], note, &palette);
                    changed |= c;
                    if del {
                        delete = Some(i);
                    }
                    cols[1].add_space(8.0);
                }
            });
        } else {
            for (i, note) in notes.items.iter_mut().enumerate() {
                let (c, del) = note_card(ui, note, &palette);
                changed |= c;
                if del {
                    delete = Some(i);
                }
                ui.add_space(6.0);
            }
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
        if opts.plaunch {
            let hits = plaunch_hits(&self.index, query);
            if !hits.is_empty() {
                any = true;
                section(ui, "PLaunch", tag_color(&opts, None, ui));
                for e in &hits {
                    rows.flat(ui, e, true, actions);
                }
            }
        }
        let start = self.shared.start_menu.lock().unwrap();
        let all_start = if opts.current_user || opts.all_users {
            start_hits(&start, query)
        } else {
            Vec::new()
        };
        for source in [Source::CurrentUser, Source::AllUsers] {
            let enabled = match source {
                Source::CurrentUser => opts.current_user,
                Source::AllUsers => opts.all_users,
            };
            if !enabled {
                continue;
            }
            let hits: Vec<&StartItem> = all_start
                .iter()
                .copied()
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
            ui.heading("PLaunch");
            ui.add_space(12.0);
            ui.label(tr("Aucun fichier PLaunch.xml n'est ouvert."));
            if let Some(e) = &self.load_error {
                ui.add_space(6.0);
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            ui.add_space(16.0);
            if let Some(path) = &target {
                if ui
                    .add(egui::Button::new(
                        egui::RichText::new(tr("＋ Créer un nouveau PLaunch.xml")).strong(),
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
                if ui.button(tr("Ouvrir un PLaunch.xml…")).clicked() {
                    actions.push(Action::Browse(Browse::OpenConfig));
                }
            });
            ui.add_space(16.0);
            ui.weak(tr(
                "Vous pouvez aussi glisser un fichier PLaunch.xml sur cette fenêtre,",
            ));
            ui.weak(tr("ou placer plaunch.exe à côté de votre PLaunch.xml."));
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
        let tok = modern::tokens(ui.visuals());
        let target = cfg.resolve(el);
        let opts = EntryOptions::of(el);
        let is_running = self
            .shared
            .running
            .lock()
            .unwrap()
            .contains_key(&crate::eject::path_key(&target));
        let exists_state = self.exists.get(&target).copied();

        egui::Frame::new()
            .fill(tok.card)
            .stroke(egui::Stroke::new(1.0, tok.border))
            .corner_radius(8.0)
            .inner_margin(10.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    match self.textures.get(&target) {
                        Some(tex) => {
                            ui.add(icon_image(tex, 36.0));
                        }
                        None => {
                            ui.label(
                                egui::RichText::new(icon::ROCKET_LAUNCH)
                                    .size(28.0)
                                    .color(tok.accent),
                            );
                        }
                    }
                    ui.vertical(|ui| {
                        ui.heading(config::display_name(el));
                        if is_running {
                            ui.colored_label(
                                tok.success,
                                icon::with(icon::CIRCLE_FILL, tr("En cours d'exécution")),
                            );
                        } else if exists_state == Some(false) {
                            ui.colored_label(
                                tok.warning,
                                icon::with(icon::WARNING, tr("Cible introuvable")),
                            );
                        }
                    });
                });
                if !opts.description.is_empty() {
                    ui.add_space(4.0);
                    ui.add(
                        egui::Label::new(egui::RichText::new(&opts.description).italics()).wrap(),
                    );
                }
            });

        ui.add_space(8.0);
        let launch_btn = egui::Button::new(
            egui::RichText::new(icon::with(icon::PLAY_FILL, tr("Lancer")))
                .color(egui::Color32::WHITE)
                .strong(),
        )
        .fill(tok.accent)
        .corner_radius(6.0)
        .min_size(egui::vec2(ui.available_width(), 30.0));
        if ui.add(launch_btn).clicked() {
            actions.push(Action::Launch(sel.clone()));
        }

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(icon::with(icon::PENCIL_SIMPLE, tr("Modifier…")))
                .clicked()
            {
                actions.push(Action::Edit(sel.clone()));
            }
            if ui
                .button(icon::with(icon::FOLDER_OPEN, tr("Emplacement")))
                .clicked()
            {
                actions.push(Action::Reveal(sel.clone()));
            }
            if ui
                .button(icon::with(icon::COPY, tr("Copier le chemin")))
                .clicked()
            {
                actions.push(Action::CopyPath(target.clone()));
            }
            let pin = if opts.favorite {
                icon::with(icon::STAR_FILL, tr("Retirer des favoris"))
            } else {
                icon::with(icon::STAR, tr("Ajouter aux favoris"))
            };
            if ui.button(pin).clicked() {
                actions.push(Action::ToggleFavorite(sel.clone()));
            }
            if ui
                .button(icon::with(icon::TRASH, tr("Supprimer")))
                .clicked()
            {
                actions.push(Action::AskDelete(sel.clone()));
            }
        });

        ui.add_space(8.0);
        egui::Frame::new()
            .fill(tok.card)
            .stroke(egui::Stroke::new(1.0, tok.border))
            .corner_radius(8.0)
            .inner_margin(10.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::Grid::new("details")
                    .num_columns(2)
                    .spacing([8.0, 5.0])
                    .show(ui, |ui| {
                        ui.weak(tr("Chemin"));
                        ui.add(egui::Label::new(config::entry_path(el)).wrap());
                        ui.end_row();
                        ui.weak(tr("Cible"));
                        ui.add(egui::Label::new(&target).wrap());
                        ui.end_row();
                        if !opts.parameters.is_empty() {
                            ui.weak(tr("Paramètres"));
                            ui.add(
                                egui::Label::new(egui::RichText::new(&opts.parameters).monospace())
                                    .wrap(),
                            );
                            ui.end_row();
                        }
                        if let Some(dir) = cfg.working_dir(el) {
                            ui.weak(tr("Dossier"));
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
                            ui.weak(tr("Raccourci clavier"));
                            ui.label(&opts.hotkey);
                            ui.end_row();
                        }
                        if !flags.is_empty() {
                            ui.weak(tr("Options"));
                            ui.label(flags.join(", "));
                            ui.end_row();
                        }
                        ui.weak(tr("État"));
                        match exists_state {
                            Some(true) => ui.colored_label(tok.success, tr("présent")),
                            Some(false) => ui.colored_label(tok.warning, tr("introuvable")),
                            None => ui.weak(tr("vérification…")),
                        };
                        ui.end_row();
                        ui.weak(tr("Lancements"));
                        ui.label(config::runcount(el).to_string());
                        ui.end_row();
                        ui.weak(tr("Dernier"));
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
                            ui.weak(tr("Utilisation"));
                            ui.label(crate::usage::format_duration(used));
                            ui.end_row();
                        }
                    });
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
pub(super) fn plaunch_hits(index: &CachedIndex, query: &str) -> Vec<config::FlatEntry> {
    let mut hits: Vec<(i32, &config::FlatEntry)> = index
        .entries
        .iter()
        .filter_map(|c| {
            fuzzy::rank_folded(query, &c.folded_name, &c.folded_target, c.entry.runcount)
                .map(|s| (s, &c.entry))
        })
        .collect();
    hits.sort_by_key(|h| std::cmp::Reverse(h.0));
    hits.into_iter()
        .take(MAX_HITS)
        .map(|(_, e)| e.clone())
        .collect()
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
        None => opts.color_plaunch,
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
