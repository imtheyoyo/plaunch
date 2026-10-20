//! Lignes de l'arborescence et des listes (glisser-déposer, menus).

use super::*;

/// Contexte commun au dessin des lignes.
pub(super) struct Rows<'a> {
    pub(super) selected: &'a Option<ItemPath>,
    pub(super) exists: &'a HashMap<String, bool>,
    pub(super) textures: &'a Textures,
    pub(super) single: bool,
    /// Lignes dessinées, dans l'ordre (navigation au clavier).
    pub(super) visible: &'a RefCell<Vec<ItemPath>>,
    pub(super) open: &'a HashSet<String>,
    pub(super) scroll_to: bool,
    /// Sélection multiple.
    pub(super) multi: &'a [ItemPath],
    /// Exécutables ouverts (clés `eject::path_key`).
    pub(super) running: &'a HashSet<String>,
}

/// Clic simple, Ctrl+clic ou Maj+clic sur une ligne.
fn click_action(ui: &egui::Ui, path: &[usize]) -> Action {
    let m = ui.input(|i| i.modifiers);
    if m.command {
        Action::SelectMulti {
            path: path.to_vec(),
            range: false,
        }
    } else if m.shift {
        Action::SelectMulti {
            path: path.to_vec(),
            range: true,
        }
    } else {
        Action::Select(path.to_vec())
    }
}

/// Clé d'un groupe : la suite des noms depuis la racine.
pub(super) fn group_key(names: &[String]) -> String {
    names.join("\u{1f}")
}

/// Clé du groupe situé à `path`.
pub(super) fn group_key_at(cfg: &Config, path: &[usize]) -> Option<String> {
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
    pub(super) fn tree(
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
                    let id = ui.make_persistent_id(("plaunch-group", path.clone()));
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

    pub(super) fn group_header(
        &self,
        ui: &egui::Ui,
        header: &egui::Response,
        path: &[usize],
        actions: &mut Vec<Action>,
    ) {
        header.dnd_set_drag_payload(DragItem(path.to_vec()));
        if header.clicked() {
            actions.push(click_action(ui, path));
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
    pub(super) fn drop_target(
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
    pub(super) fn end_drop_zone(
        &self,
        ui: &mut egui::Ui,
        root_len: usize,
        actions: &mut Vec<Action>,
    ) {
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

    pub(super) fn flat(
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

    pub(super) fn icon(&self, ui: &mut egui::Ui, target: &str) {
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
    pub(super) fn row(
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
        let is_selected =
            self.selected.as_deref() == Some(path) || self.multi.iter().any(|p| p == path);
        let running = self.running.contains(&crate::eject::path_key(target));
        let mut text = egui::RichText::new(name);
        if missing {
            text = text.color(ui.visuals().weak_text_color()).strikethrough();
        }
        ui.horizontal(|ui| {
            self.icon(ui, target);
            let mut resp = ui.selectable_label(is_selected, text);
            if running {
                ui.colored_label(egui::Color32::from_rgb(46, 170, 90), "●")
                    .on_hover_text(tr("En cours d'exécution"));
            }
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
            let plain = ui.input(|i| !i.modifiers.command && !i.modifiers.shift);
            if resp.double_clicked() || (self.single && resp.clicked() && plain) {
                actions.push(Action::Launch(path.to_vec()));
            } else if resp.clicked() {
                actions.push(click_action(ui, path));
            }
            resp.context_menu(|ui| {
                if ui.button(tr("Lancer")).clicked() {
                    actions.push(Action::Launch(path.to_vec()));
                    ui.close();
                }
                if running && ui.button(tr("Lancer une autre instance")).clicked() {
                    actions.push(Action::LaunchNew(path.to_vec()));
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
                if ui.button(tr("Dupliquer")).clicked() {
                    actions.push(Action::Duplicate(path.to_vec()));
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

    pub(super) fn start_item(
        &self,
        ui: &mut egui::Ui,
        item: &StartItem,
        actions: &mut Vec<Action>,
    ) {
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
                if ui.button(tr("Ajouter à PLaunch")).clicked() {
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
