//! Pilotage par une IA (serveur MCP), pour les tests de débogage.
//!
//! Compilé seulement avec la fonctionnalité `mcp` (`plaunch-mcp.exe`) et
//! activé au lancement par `--mcp` (voir docs/MCP.md). Les outils génériques
//! (capture, arbre des widgets, clics, clavier, journal) viennent du crate
//! `egui-mcp` ; ce module ajoute ceux propres à PLaunch.

use std::sync::atomic::Ordering;

use eframe::egui;
use egui_mcp::{json, Bridge, Call, Options, Tool, Value};

use super::{tree::group_key_at, Action, Dialog, PLaunchApp, View};
use crate::config::{self, Config, ItemPath};

/// Démarre le serveur si `--mcp` figure parmi les arguments.
pub(super) fn start(ctx: &egui::Context) -> Option<Bridge> {
    // Port par défaut : 47800 (ComicRust utilise 47801).
    let (port, token) = egui_mcp::parse_args(std::env::args().skip(1))?;
    let mut options = Options::new("PLaunch", env!("CARGO_PKG_VERSION"));
    options.port = port;
    options.token = token;
    options.log_file = Some(crate::log::path());
    options.instructions = "PLaunch est un lanceur portable : arborescence de raccourcis \
        (groupes, entrées, séparateurs) lue dans PLaunch.xml. Les éléments se désignent par leur \
        chemin d'indices (« 0/2/1 », donné par list_items) ou par leur nom (« Groupe › Nom », ou \
        le nom seul s'il est unique). launch_item lance réellement le programme."
        .into();
    match Bridge::start(ctx, options, tools()) {
        Ok(b) => {
            crate::log::info(&format!("serveur MCP actif sur {}", b.url()));
            Some(b)
        }
        Err(e) => {
            crate::log::error(&format!("serveur MCP : port {port} indisponible ({e})"));
            None
        }
    }
}

fn item_arg() -> Value {
    json!({
        "item": { "type": "string", "description": "chemin d'indices « 0/2/1 » ou nom « Groupe › Nom »" }
    })
}

fn schema(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required })
}

fn tools() -> Vec<Tool> {
    vec![
        Tool::simple(
            "get_state",
            "État de PLaunch : fichier, vue, recherche, sélection, dialogue ouvert, statut, erreurs.",
        ),
        Tool::new(
            "list_items",
            "Liste l'arborescence (groupes, raccourcis, séparateurs) avec chemin, cible et existence.",
            schema(json!({ "filter": { "type": "string" } }), &[]),
        ),
        Tool::new(
            "select_item",
            "Sélectionne un élément (ouvre ses groupes parents et fait défiler jusqu'à lui).",
            schema(item_arg(), &["item"]),
        ),
        Tool::new(
            "launch_item",
            "Lance un raccourci (ou tous ceux d'un groupe, avec confirmation).",
            schema(item_arg(), &["item"]),
        ),
        Tool::new(
            "edit_item",
            "Ouvre le dialogue de modification d'un élément.",
            schema(item_arg(), &["item"]),
        ),
        Tool::new(
            "toggle_group",
            "Ouvre ou ferme un groupe de l'arborescence (open : état voulu, sinon bascule).",
            schema(
                {
                    let mut p = item_arg();
                    p["open"] = json!({ "type": "boolean" });
                    p
                },
                &["item"],
            ),
        ),
        Tool::new(
            "set_search",
            "Remplace le texte de la recherche (vide : efface).",
            schema(json!({ "text": { "type": "string" } }), &["text"]),
        ),
        Tool::new(
            "set_view",
            "Change de vue.",
            schema(
                json!({ "view": { "type": "string", "enum": ["tree", "most_used", "recent", "notes", "stats"] } }),
                &["view"],
            ),
        ),
        Tool::simple("new_entry", "Ouvre le dialogue « Nouveau raccourci »."),
        Tool::simple("new_group", "Ouvre le dialogue « Nouveau groupe »."),
        Tool::simple("open_settings", "Ouvre les réglages."),
        Tool::simple("close_dialog", "Ferme le dialogue ouvert sans enregistrer."),
        Tool::simple("undo", "Annule la dernière modification de l'arborescence."),
        Tool::simple("redo", "Rétablit la modification annulée."),
        Tool::simple("reload", "Recharge PLaunch.xml depuis le disque."),
        Tool::simple("toggle_compact", "Bascule le mode compact."),
        Tool::simple(
            "show_window",
            "Affiche et active la fenêtre (nécessaire si elle est masquée dans la zone de notification).",
        ),
    ]
}

impl PLaunchApp {
    /// Traite les appels reçus (avant le dessin, configuration déverrouillée).
    pub(super) fn mcp_calls(&mut self) {
        let calls = match &mut self.mcp {
            Some(b) => b.take_calls(),
            None => return,
        };
        for call in calls {
            self.mcp_call(call);
        }
    }

    /// Fin d'image : arbre des widgets et injection des événements suivants.
    pub(super) fn mcp_end_frame(&mut self, ctx: &egui::Context) {
        if let Some(b) = &mut self.mcp {
            b.end_frame(ctx);
        }
    }

    fn mcp_call(&mut self, call: Call) {
        match call.name.as_str() {
            "get_state" => {
                let state = self.mcp_state();
                call.json(state)
            }
            "list_items" => {
                let filter = call.str("filter").unwrap_or("").to_lowercase();
                let guard = self.shared.config.lock().unwrap();
                let Some(cfg) = guard.as_ref() else {
                    return call.error("aucun fichier PLaunch.xml chargé");
                };
                let items: Vec<Value> = walk(cfg)
                    .into_iter()
                    .filter(|i| filter.is_empty() || i.label.to_lowercase().contains(&filter))
                    .map(|i| {
                        let mut v = json!({
                            "path": path_str(&i.path),
                            "kind": i.kind,
                            "name": i.label,
                        });
                        if let Some(t) = &i.target {
                            v["target"] = json!(t);
                            if let Some(e) = self.exists.get(t) {
                                v["exists"] = json!(e);
                            }
                        }
                        v
                    })
                    .collect();
                drop(guard);
                call.json(json!({ "count": items.len(), "items": items }))
            }
            "select_item" => self.with_item(call, |app, path| {
                app.open_parents(&path);
                app.apply(Action::Select(path));
                app.scroll_to_selected = true;
                Ok("élément sélectionné".into())
            }),
            "launch_item" => self.with_item(call, |app, path| {
                let is_group = app.is_group(&path);
                app.apply(if is_group {
                    Action::LaunchGroup {
                        path,
                        confirmed: false,
                    }
                } else {
                    Action::Launch(path)
                });
                Ok(if is_group {
                    "confirmation de lancement du groupe affichée".into()
                } else {
                    "lancement demandé (voir get_state.status)".into()
                })
            }),
            "edit_item" => self.with_item(call, |app, path| {
                app.apply(Action::Edit(path));
                Ok("dialogue de modification ouvert".into())
            }),
            "toggle_group" => {
                let want = call.bool("open");
                self.with_item(call, move |app, path| {
                    let key = {
                        let guard = app.shared.config.lock().unwrap();
                        guard.as_ref().and_then(|c| group_key_at(c, &path))
                    };
                    let Some(key) = key else {
                        return Err("cet élément n'est pas un groupe".into());
                    };
                    let open = app.open_groups.contains(&key);
                    if want.is_none_or(|w| w != open) {
                        app.apply(Action::ToggleGroup(key));
                    }
                    Ok(if want.unwrap_or(!open) {
                        "groupe ouvert"
                    } else {
                        "groupe fermé"
                    }
                    .into())
                })
            }
            "set_search" => {
                self.search = call.str("text").unwrap_or("").to_string();
                self.focus_search = true;
                call.ok(format!("recherche : « {} »", self.search))
            }
            "set_view" => {
                let view = match call.str("view").unwrap_or("") {
                    "tree" => View::Tree,
                    "most_used" => View::MostUsed,
                    "recent" => View::Recent,
                    "notes" => View::Notes,
                    "stats" => View::Stats,
                    v => return call.error(format!("vue inconnue : {v}")),
                };
                self.view = view;
                call.ok(format!("vue : {}", view.label()))
            }
            "new_entry" => self.simple(call, Action::NewEntry(None)),
            "new_group" => self.simple(call, Action::NewGroup(None)),
            "open_settings" => self.simple(call, Action::OpenSettings),
            "close_dialog" => self.simple(call, Action::CloseDialog),
            "undo" => self.simple(call, Action::Undo),
            "redo" => self.simple(call, Action::Redo),
            "reload" => self.simple(call, Action::Reload),
            "toggle_compact" => self.simple(call, Action::ToggleCompact),
            "show_window" => {
                crate::window::show();
                call.ok("fenêtre affichée")
            }
            _ => drop(call),
        }
    }

    fn simple(&mut self, call: Call, action: Action) {
        self.apply(action);
        call.ok("fait");
    }

    /// Résout l'argument `item`, puis exécute `f`.
    fn with_item(
        &mut self,
        call: Call,
        f: impl FnOnce(&mut Self, ItemPath) -> Result<String, String>,
    ) {
        let Some(item) = call.str("item").map(str::to_string) else {
            return call.error("argument « item » manquant");
        };
        let resolved = {
            let guard = self.shared.config.lock().unwrap();
            match guard.as_ref() {
                Some(cfg) => resolve(cfg, &item),
                None => Err("aucun fichier PLaunch.xml chargé".into()),
            }
        };
        match resolved.and_then(|p| f(self, p)) {
            Ok(msg) => call.ok(msg),
            Err(e) => call.error(e),
        }
    }

    fn is_group(&self, path: &[usize]) -> bool {
        let guard = self.shared.config.lock().unwrap();
        guard
            .as_ref()
            .and_then(|c| c.item(path))
            .is_some_and(|e| e.name == "files")
    }

    /// Ouvre les groupes qui contiennent `path`.
    fn open_parents(&mut self, path: &[usize]) {
        let guard = self.shared.config.lock().unwrap();
        if let Some(cfg) = guard.as_ref() {
            for n in 1..path.len() {
                if let Some(k) = group_key_at(cfg, &path[..n]) {
                    self.open_groups.insert(k);
                }
            }
        }
    }

    fn mcp_state(&self) -> Value {
        let guard = self.shared.config.lock().unwrap();
        let cfg = guard.as_ref();
        let label_of = |p: &ItemPath| cfg.and_then(|c| label_at(c, p));
        let dialog = self.dialog.as_ref().map(|d| match d {
            Dialog::Form(f) => f.title().to_string(),
            Dialog::Settings(_) => "Réglages".into(),
            Dialog::ConfirmDelete { .. } => "Confirmation de suppression".into(),
            Dialog::DriveFix { .. } => "Changement de lettre de lecteur".into(),
            Dialog::ConfirmCleanup(_) => "Nettoyage des raccourcis".into(),
            Dialog::ConfirmDeleteMany(_) => "Suppression multiple".into(),
            Dialog::NewProfile { .. } => "Nouveau profil".into(),
            Dialog::ConfirmRestore { .. } => "Restauration".into(),
            Dialog::ConfirmLaunchGroup { .. } => "Lancement d'un groupe".into(),
            Dialog::Eject { .. } => "Éjection".into(),
            Dialog::Import { .. } => "Import de programmes".into(),
        });
        let form_error = match &self.dialog {
            Some(Dialog::Form(f)) => f.error.clone(),
            _ => None,
        };
        json!({
            "version": env!("CARGO_PKG_VERSION"),
            "file": cfg.map(|c| c.file.display().to_string()),
            "load_error": self.load_error,
            "entries": cfg.map(|c| c.flatten().len()),
            "view": self.view.label(),
            "search": self.search,
            "selected": self.selected.as_ref().map(|p| json!({
                "path": path_str(p),
                "name": label_of(p),
            })),
            "multi_selection": self.multi.iter().map(Vec::as_slice).map(path_str).collect::<Vec<_>>(),
            "dialog": dialog,
            "dialog_error": form_error,
            "status": self.shared.status.lock().unwrap().clone(),
            "compact": self.settings.compact,
            "theme": format!("{:?}", self.settings.theme),
            "language": format!("{:?}", self.settings.language),
            "new_programs_found": self.found.len(),
            "drive_warnings": self.drive_warnings,
            "quitting": self.shared.quitting.load(Ordering::SeqCst),
        })
    }
}

struct Item {
    path: ItemPath,
    kind: &'static str,
    /// « Groupe › Sous-groupe › Nom ».
    label: String,
    target: Option<String>,
}

/// Tous les éléments, dans l'ordre du fichier.
fn walk(cfg: &Config) -> Vec<Item> {
    fn rec(
        cfg: &Config,
        el: &crate::xml::Element,
        path: &mut ItemPath,
        names: &mut Vec<String>,
        out: &mut Vec<Item>,
    ) {
        for (i, child) in el.children.iter().enumerate() {
            path.push(i);
            let kind = match child.name.as_str() {
                "files" => "group",
                "file" => "entry",
                "separator" => "separator",
                _ => {
                    path.pop();
                    continue;
                }
            };
            let name = if kind == "separator" {
                "───".to_string()
            } else {
                config::display_name(child).to_string()
            };
            names.push(name);
            out.push(Item {
                path: path.clone(),
                kind,
                label: names.join(" › "),
                target: (kind == "entry").then(|| cfg.resolve(child)),
            });
            if kind == "group" {
                rec(cfg, child, path, names, out);
            }
            names.pop();
            path.pop();
        }
    }
    let mut out = Vec::new();
    if let Some(files) = cfg.files() {
        rec(cfg, files, &mut Vec::new(), &mut Vec::new(), &mut out);
    }
    out
}

fn path_str(p: &[usize]) -> String {
    p.iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join("/")
}

fn label_at(cfg: &Config, path: &[usize]) -> Option<String> {
    walk(cfg)
        .into_iter()
        .find(|i| i.path == path)
        .map(|i| i.label)
}

/// « 0/2/1 », « Groupe › Nom » ou nom seul (s'il est unique).
fn resolve(cfg: &Config, item: &str) -> Result<ItemPath, String> {
    let item = item.trim();
    if !item.is_empty() && item.chars().all(|c| c.is_ascii_digit() || c == '/') {
        let path: ItemPath = item
            .split('/')
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        return match cfg.item(&path) {
            Some(_) if !path.is_empty() => Ok(path),
            _ => Err(format!("aucun élément au chemin {item}")),
        };
    }
    let wanted = item.replace(" > ", " › ").to_lowercase();
    let all = walk(cfg);
    let exact: Vec<&Item> = all
        .iter()
        .filter(|i| i.label.to_lowercase() == wanted)
        .collect();
    if let [one] = exact.as_slice() {
        return Ok(one.path.clone());
    }
    let by_name: Vec<&Item> = all
        .iter()
        .filter(|i| {
            i.label
                .rsplit(" › ")
                .next()
                .is_some_and(|n| n.to_lowercase() == wanted)
        })
        .collect();
    match by_name.as_slice() {
        [one] => Ok(one.path.clone()),
        [] => Err(format!("aucun élément « {item} » (voir list_items)")),
        many => Err(format!(
            "« {item} » est ambigu : {}",
            many.iter()
                .map(|i| format!("{} ({})", i.label, path_str(&i.path)))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = "<start><files>\
        <files name=\"Outils\"><file name=\"Notepad\"><path>C:\\n.exe</path></file></files>\
        <separator/>\
        <file name=\"Firefox\"><path>C:\\f.exe</path></file>\
        <files name=\"Jeux\"><file name=\"Notepad\"><path>C:\\m.exe</path></file></files>\
        </files></start>";

    #[test]
    fn resolve_items() {
        let cfg = Config::from_str(XML, "C:\\").unwrap();
        assert_eq!(resolve(&cfg, "0/0").unwrap(), vec![0, 0]);
        assert_eq!(resolve(&cfg, "Firefox").unwrap(), vec![2]);
        assert_eq!(resolve(&cfg, "jeux > notepad").unwrap(), vec![3, 0]);
        assert!(resolve(&cfg, "Notepad").unwrap_err().contains("ambigu"));
        assert!(resolve(&cfg, "9/9").is_err());
        let items = walk(&cfg);
        assert_eq!(items.len(), 6);
        assert_eq!(items[1].label, "Outils › Notepad");
        assert_eq!(items[2].kind, "separator");
    }
}
