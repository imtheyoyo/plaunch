//! Arbre des widgets, lu dans l'état AccessKit de l'image en cours.
//!
//! La capture se fait à la fin de `ui()`, quand toute l'interface de l'image a
//! été déclarée (fenêtres, menus et bulles compris).

use std::collections::HashMap;

use egui::accesskit::{self, NodeId, Role, Toggled};
use egui::{Pos2, Rect};
use serde_json::Value;

pub struct Node {
    pub id: u64,
    pub depth: usize,
    pub role: Role,
    pub label: String,
    pub value: String,
    pub rect: Option<Rect>,
    pub disabled: bool,
    pub toggled: Option<Toggled>,
    pub selected: bool,
    pub clickable: bool,
    pub focused: bool,
}

impl Node {
    pub fn center(&self) -> Pos2 {
        self.rect.map(|r| r.center()).unwrap_or_default()
    }

    pub fn describe(&self) -> String {
        let r = self.rect.unwrap_or(Rect::NOTHING);
        format!(
            "{:?} « {} » en ({:.0}, {:.0})",
            self.role,
            short(if self.label.is_empty() {
                &self.value
            } else {
                &self.label
            }),
            r.center().x,
            r.center().y
        )
    }

    fn line(&self) -> String {
        let mut s = format!("{}{:?}", "  ".repeat(self.depth.min(12)), self.role);
        if !self.label.is_empty() {
            s += &format!(" \"{}\"", short(&self.label));
        }
        if !self.value.is_empty() {
            s += &format!(" = \"{}\"", short(&self.value));
        }
        if let Some(r) = self.rect {
            s += &format!(
                " @({:.0},{:.0} {:.0}×{:.0})",
                r.min.x,
                r.min.y,
                r.width(),
                r.height()
            );
        }
        let mut flags = Vec::new();
        if self.clickable {
            flags.push("cliquable");
        }
        if self.disabled {
            flags.push("désactivé");
        }
        match self.toggled {
            Some(Toggled::True) => flags.push("coché"),
            Some(Toggled::False) => flags.push("décoché"),
            Some(Toggled::Mixed) => flags.push("mixte"),
            None => {}
        }
        if self.selected {
            flags.push("sélectionné");
        }
        if self.focused {
            flags.push("focus");
        }
        if !flags.is_empty() {
            s += &format!(" [{}]", flags.join(", "));
        }
        s + &format!(" id={:016x}", self.id)
    }

    fn matches_text(&self, needle: &str) -> bool {
        self.label.to_lowercase().contains(needle) || self.value.to_lowercase().contains(needle)
    }
}

fn short(s: &str) -> String {
    let s = s.replace('\n', "⏎");
    if s.chars().count() > 80 {
        s.chars().take(79).collect::<String>() + "…"
    } else {
        s
    }
}

pub struct Snapshot {
    pub nodes: Vec<Node>,
    pub available: bool,
}

impl Snapshot {
    pub fn capture(ctx: &egui::Context) -> Self {
        let focused = ctx.memory(|m| m.focused()).map(|id| id.value());
        ctx.viewport(|vp| {
            let Some(state) = vp.this_pass.accesskit_state.as_ref() else {
                return Snapshot {
                    nodes: Vec::new(),
                    available: false,
                };
            };
            let map: HashMap<NodeId, &accesskit::Node> = state
                .nodes
                .iter()
                .map(|(id, n)| (id.accesskit_id(), n))
                .collect();
            let mut nodes = Vec::new();
            let root = egui::accesskit_root_id().accesskit_id();
            // Parcours en profondeur, dans l'ordre de déclaration.
            let mut stack = vec![(root, 0usize)];
            let mut seen = std::collections::HashSet::new();
            while let Some((id, depth)) = stack.pop() {
                if !seen.insert(id) {
                    continue;
                }
                let Some(n) = map.get(&id) else { continue };
                if id != root {
                    nodes.push(make_node(id, depth, n, &map, focused));
                }
                for child in n.children().iter().rev() {
                    stack.push((*child, if id == root { 0 } else { depth + 1 }));
                }
            }
            Snapshot {
                nodes,
                available: true,
            }
        })
    }

    pub fn render(&self, filter: &str, all: bool) -> String {
        if !self.available {
            return "Arbre indisponible : AccessKit vient d'être activé, réessayer.".into();
        }
        let needle = filter.to_lowercase();
        let lines: Vec<String> = self
            .nodes
            .iter()
            .filter(|n| all || n.clickable || !n.label.is_empty() || !n.value.is_empty())
            .filter(|n| {
                needle.is_empty()
                    || n.matches_text(&needle)
                    || format!("{:?}", n.role).to_lowercase().contains(&needle)
            })
            .map(Node::line)
            .collect();
        format!(
            "{} widget(s) — coordonnées en points (mêmes unités que click et screenshot).\n{}",
            lines.len(),
            lines.join("\n")
        )
    }

    /// Cherche un widget : `id`, ou `label` (+ `role`, `index`).
    pub fn find(&self, args: &Value) -> Result<&Node, String> {
        if !self.available {
            return Err("arbre des widgets indisponible, réessayer".into());
        }
        let visible = |n: &&Node| n.rect.is_some_and(|r| r.is_positive());
        if let Some(id) = args.get("id").and_then(Value::as_str) {
            let id = id.trim_start_matches("0x").to_lowercase();
            return self
                .nodes
                .iter()
                .filter(visible)
                .find(|n| format!("{:016x}", n.id).ends_with(&id))
                .ok_or_else(|| format!("aucun widget avec l'id {id}"));
        }
        let label = args
            .get("label")
            .and_then(Value::as_str)
            .ok_or("indiquer « label » (texte du widget) ou « id »")?
            .to_lowercase();
        let role = args
            .get("role")
            .and_then(Value::as_str)
            .map(str::to_lowercase);
        let index = args.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
        let role_ok = |n: &&Node| {
            role.as_ref()
                .is_none_or(|r| format!("{:?}", n.role).to_lowercase() == *r)
        };
        let mut hits: Vec<(&Node, bool)> = self
            .nodes
            .iter()
            .filter(visible)
            .filter(role_ok)
            .filter_map(|n| {
                let l = n.label.to_lowercase();
                if l == label || (l.is_empty() && n.value.to_lowercase() == label) {
                    Some((n, true))
                } else if n.matches_text(&label) {
                    Some((n, false))
                } else {
                    None
                }
            })
            .collect();
        // Correspondances exactes d'abord, puis widgets cliquables.
        hits.sort_by_key(|(n, exact)| (!exact, !n.clickable));
        if let Some((n, _)) = hits.get(index) {
            return Ok(n);
        }
        let mut msg = format!("aucun widget « {label} »");
        if index > 0 && !hits.is_empty() {
            msg += &format!(" à l'index {index} ({} trouvé(s))", hits.len());
        }
        let names: Vec<String> = self
            .nodes
            .iter()
            .filter(|n| n.clickable && !n.label.is_empty())
            .take(25)
            .map(|n| format!("« {} »", short(&n.label)))
            .collect();
        if !names.is_empty() {
            msg += &format!(". Widgets cliquables visibles : {}", names.join(", "));
        }
        Err(msg)
    }
}

fn make_node(
    id: NodeId,
    depth: usize,
    n: &accesskit::Node,
    map: &HashMap<NodeId, &accesskit::Node>,
    focused: Option<u64>,
) -> Node {
    let mut label = n.label().unwrap_or_default().to_string();
    if label.is_empty() {
        // Libellé porté par d'autres nœuds (texte d'un bouton, etc.).
        let parts: Vec<&str> = n
            .labelled_by()
            .iter()
            .filter_map(|l| map.get(l))
            .filter_map(|l| l.label().or(l.value()))
            .collect();
        label = parts.join(" ");
    }
    let id_value: u64 = id.0;
    Node {
        id: id_value,
        depth,
        role: n.role(),
        label,
        value: n.value().unwrap_or_default().to_string(),
        rect: n.bounds().map(|b| {
            Rect::from_min_max(
                Pos2::new(b.x0 as f32, b.y0 as f32),
                Pos2::new(b.x1 as f32, b.y1 as f32),
            )
        }),
        disabled: n.is_disabled(),
        toggled: n.toggled(),
        selected: n.is_selected().unwrap_or(false),
        clickable: n.supports_action(accesskit::Action::Click),
        focused: focused == Some(id_value),
    }
}
