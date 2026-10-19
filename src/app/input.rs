//! Clavier, navigation et fermeture de la fenêtre.

use super::*;

impl PStartApp {
    pub(super) fn handle_input(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
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
        if self.multi.len() > 1 {
            if delete && !typing {
                actions.push(Action::AskDeleteMany(self.multi.clone()));
            }
        } else if let Some(sel) = self.selected.clone() {
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
    pub(super) fn navigate(
        &mut self,
        ctx: &egui::Context,
        search_focused: bool,
        actions: &mut Vec<Action>,
    ) {
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
    pub(super) fn enter_pressed(&self, actions: &mut Vec<Action>) {
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

    pub(super) fn handle_close(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }
        self.flush_notes();
        crate::monitor::save_usage(&self.shared);
        let quitting = self.shared.quitting.load(Ordering::SeqCst);
        if !quitting && self.settings.close_to_tray && self.tray.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            window::hide();
        }
    }
}
