//! Fenêtres modales : formulaires, réglages, éjection, import…

use super::*;

impl PStartApp {
    pub(super) fn dialog_ui(
        &mut self,
        ctx: &egui::Context,
        cfg: Option<&Config>,
        actions: &mut Vec<Action>,
    ) {
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
                Dialog::ConfirmDeleteMany(paths) => {
                    ui.heading(tr("Supprimer"));
                    ui.add_space(8.0);
                    let entries: usize = cfg
                        .map(|c| {
                            paths
                                .iter()
                                .filter_map(|p| c.item(p))
                                .map(edit::count_entries)
                                .sum()
                        })
                        .unwrap_or(0);
                    ui.label(trf(
                        "Supprimer les {} éléments sélectionnés ({} raccourcis) ?",
                        &[&paths.len(), &entries],
                    ));
                    ui.weak(tr(
                        "Les programmes eux-mêmes ne sont pas supprimés (Ctrl+Z pour annuler).",
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
}

pub(super) fn form_ui(
    ui: &mut egui::Ui,
    form: &mut EditForm,
    base: &str,
    actions: &mut Vec<Action>,
) {
    ui.heading(form.title());
    ui.add_space(8.0);
    let mut submit = false;
    egui::Grid::new("form")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            if let FormKind::NewEntry { parent, index } | FormKind::NewGroup { parent, index } =
                &mut form.kind
            {
                ui.label(tr("Emplacement"));
                let current = form
                    .groups
                    .iter()
                    .find(|(p, _)| p == parent)
                    .map(|(_, l)| l.clone())
                    .unwrap_or_else(|| tr("(racine)").to_string());
                let place = if index.is_some() {
                    trf("{} (après la sélection)", &[&current])
                } else {
                    trf("{} (à la fin)", &[&current])
                };
                egui::ComboBox::from_id_salt("location")
                    .selected_text(place)
                    .width(300.0)
                    .show_ui(ui, |ui| {
                        for (p, label) in &form.groups {
                            if ui.selectable_label(p == parent, label).clicked() {
                                *parent = p.clone();
                                *index = None;
                            }
                        }
                    });
                ui.end_row();
            }
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
                ui.label(tr("Raccourci clavier")).on_hover_text(tr(
                    "Lance ce programme depuis n'importe où, même fenêtre masquée",
                ));
                widgets::hotkey_capture(ui, &mut o.hotkey, "entry");
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
pub(super) fn note_card(
    ui: &mut egui::Ui,
    i: usize,
    note: &mut Note,
    palette: &[[u8; 3]],
) -> (bool, bool) {
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
                if ui.small_button(tr("Supprimer")).clicked() {
                    delete = true;
                }
            });
            if widgets::alarm_editor(ui, &mut note.alarm, i) {
                note.sync_input();
                changed = true;
            }
            if note.alarm.is_some() {
                ui.horizontal(|ui| {
                    ui.label(tr("Répéter"));
                    let before = note.repeat;
                    egui::ComboBox::from_id_salt(("repeat", i))
                        .selected_text(note.repeat.label())
                        .show_ui(ui, |ui| {
                            for r in Repeat::ALL {
                                ui.selectable_value(&mut note.repeat, r, r.label());
                            }
                        });
                    changed |= note.repeat != before;
                });
            }
        });
    (changed, delete)
}

pub(super) fn lighten(c: egui::Color32) -> egui::Color32 {
    let f = |v: u8| v + (255 - v) / 2;
    egui::Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
}

pub(super) fn darken(c: egui::Color32) -> egui::Color32 {
    let f = |v: u8| (v as u16 * 85 / 100) as u8;
    egui::Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
}

/// Correction de la lettre de lecteur.
pub(super) fn drive_fix_ui(
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

pub(super) fn settings_ui(
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
            if widgets::hotkey_capture(ui, &mut hk, "global") {
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

            ui.label(tr("Programmes ouverts"));
            ui.vertical(|ui| {
                ui.checkbox(
                    &mut s.reuse_running,
                    tr("Ramener la fenêtre d'un programme déjà ouvert"),
                );
                ui.checkbox(&mut s.track_usage, tr("Mesurer le temps d'utilisation"));
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
                if let Some(prev) = update::previous_version() {
                    if ui
                        .button(trf("↩ Revenir à la version {}", &[&prev]))
                        .on_hover_text(tr(
                            "Remet l'exécutable précédent en place et redémarre (réversible)",
                        ))
                        .clicked()
                    {
                        actions.push(Action::Rollback);
                    }
                }
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

/// Recalcule l'aperçu de l'icône quand les réglages changent.
pub(super) fn refresh_preview(ctx: &egui::Context, form: &mut SettingsForm) {
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
pub(super) fn color_swatch(ui: &mut egui::Ui, color: IconColor, selected: bool) -> egui::Response {
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

/// Éjection de la clé : programmes ouverts, puis confirmation.
pub(super) fn eject_ui(
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
pub(super) fn import_ui(
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
