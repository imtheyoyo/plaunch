//! Protocole MCP (JSON-RPC 2.0) : initialisation, liste et appel des outils.

use std::io::{BufReader, Read, Seek, SeekFrom};
use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;

use serde_json::{json, Value};

use crate::{Job, Options, Reply, Tool, CALL_TIMEOUT};

const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

pub struct Server {
    options: Options,
    tools: Vec<Tool>,
    jobs: Mutex<Sender<Job>>,
    ctx: egui::Context,
}

impl Server {
    pub fn new(
        options: Options,
        app_tools: Vec<Tool>,
        jobs: Sender<Job>,
        ctx: egui::Context,
    ) -> Self {
        let mut tools = builtin_tools();
        for t in app_tools {
            if !tools.iter().any(|b| b.name == t.name) {
                tools.push(t);
            }
        }
        Server {
            options,
            tools,
            jobs: Mutex::new(jobs),
            ctx,
        }
    }

    pub fn options(&self) -> &Options {
        &self.options
    }

    /// Traite un message (ou un lot). `None` : rien à renvoyer (notifications).
    pub fn handle(&self, msg: Value) -> Option<Value> {
        match msg {
            Value::Array(batch) => {
                let out: Vec<Value> = batch
                    .into_iter()
                    .filter_map(|m| self.handle_one(m))
                    .collect();
                (!out.is_empty()).then_some(Value::Array(out))
            }
            m => self.handle_one(m),
        }
    }

    fn handle_one(&self, msg: Value) -> Option<Value> {
        let id = msg.get("id").cloned();
        let method = msg
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        // Notification (pas d'id) ou réponse du client : rien à renvoyer.
        let id = id?;
        let result = match method {
            "initialize" => Ok(self.initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({
                "tools": self.tools.iter().map(|t| json!({
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": t.input_schema,
                })).collect::<Vec<_>>()
            })),
            "tools/call" => Ok(self.call(&params)),
            "resources/list" => Ok(json!({ "resources": [] })),
            "prompts/list" => Ok(json!({ "prompts": [] })),
            _ => Err((-32601, format!("méthode inconnue : {method}"))),
        };
        Some(match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err((code, message)) => json!({
                "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message }
            }),
        })
    }

    fn initialize(&self, params: &Value) -> Value {
        let asked = params.get("protocolVersion").and_then(Value::as_str);
        let version = asked
            .filter(|v| PROTOCOL_VERSIONS.contains(v))
            .unwrap_or(PROTOCOL_VERSIONS[0]);
        let mut instructions = String::from(
            "Pilotage de l'application pour les tests. Méthode conseillée : screenshot pour voir \
             l'écran, ui_tree pour lister les widgets, click_widget (par libellé) ou click (x, y en \
             points), type_text après avoir cliqué dans un champ, press_key pour les raccourcis, \
             puis screenshot pour vérifier. get_logs donne le journal de l'application.",
        );
        if !self.options.instructions.is_empty() {
            instructions.push('\n');
            instructions.push_str(&self.options.instructions);
        }
        json!({
            "protocolVersion": version,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": self.options.name, "version": self.options.version },
            "instructions": instructions,
        })
    }

    fn call(&self, params: &Value) -> Value {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if !self.tools.iter().any(|t| t.name == name) {
            return tool_result(Reply::Error(format!("outil inconnu : {name}")));
        }
        let reply = match name.as_str() {
            // Traités ici, sans passer par l'interface.
            "wait" => {
                let ms = args
                    .get("ms")
                    .and_then(Value::as_u64)
                    .unwrap_or(500)
                    .min(20_000);
                std::thread::sleep(std::time::Duration::from_millis(ms));
                Reply::Text(format!("attendu {ms} ms"))
            }
            "get_logs" => self.logs(&args),
            _ => self.forward(name, args),
        };
        tool_result(reply)
    }

    /// Transmet l'appel au fil de l'interface et attend la réponse.
    fn forward(&self, name: String, args: Value) -> Reply {
        let (tx, rx) = mpsc::channel();
        let sent = self
            .jobs
            .lock()
            .map(|jobs| {
                jobs.send(Job {
                    name,
                    args,
                    reply: tx,
                })
                .is_ok()
            })
            .unwrap_or(false);
        if !sent {
            return Reply::Error("l'interface de l'application est fermée".into());
        }
        self.ctx.request_repaint();
        match rx.recv_timeout(CALL_TIMEOUT) {
            Ok(r) => r,
            Err(mpsc::RecvTimeoutError::Timeout) => Reply::Error(
                "l'application n'a pas répondu à temps (fenêtre masquée ou réduite, ou interface bloquée ?)"
                    .into(),
            ),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Reply::Error("l'application a abandonné la demande".into())
            }
        }
    }

    fn logs(&self, args: &Value) -> Reply {
        let Some(path) = &self.options.log_file else {
            return Reply::Error("cette application n'a pas déclaré de journal".into());
        };
        let lines = args
            .get("lines")
            .and_then(Value::as_u64)
            .unwrap_or(80)
            .clamp(1, 2000) as usize;
        let filter = args
            .get("filter")
            .and_then(Value::as_str)
            .map(str::to_lowercase)
            .unwrap_or_default();
        let mut f = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(e) => return Reply::Error(format!("{} : {e}", path.display())),
        };
        // Seule la fin du fichier est lue (journaux volumineux).
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        let _ = f.seek(SeekFrom::Start(len.saturating_sub(2_000_000)));
        let mut bytes = Vec::new();
        let _ = BufReader::new(f).take(2_000_000).read_to_end(&mut bytes);
        let text = String::from_utf8_lossy(&bytes);
        let all: Vec<&str> = text
            .lines()
            .filter(|l| filter.is_empty() || l.to_lowercase().contains(&filter))
            .collect();
        let start = all.len().saturating_sub(lines);
        Reply::Text(format!(
            "{} — {} dernière(s) ligne(s) :\n{}",
            path.display(),
            all.len() - start,
            all[start..].join("\n")
        ))
    }
}

fn tool_result(reply: Reply) -> Value {
    match reply {
        Reply::Text(t) => json!({ "content": [{ "type": "text", "text": t }], "isError": false }),
        Reply::Json(v) => json!({
            "content": [{ "type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default() }],
            "structuredContent": if v.is_object() { v.clone() } else { json!({ "value": v }) },
            "isError": false
        }),
        Reply::Image { png_base64, text } => json!({
            "content": [
                { "type": "image", "data": png_base64, "mimeType": "image/png" },
                { "type": "text", "text": text }
            ],
            "isError": false
        }),
        Reply::Error(e) => json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
    }
}

fn schema(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required })
}

fn builtin_tools() -> Vec<Tool> {
    let xy = || {
        json!({
            "x": { "type": "number", "description": "abscisse en points (1 px de capture = 1 point)" },
            "y": { "type": "number", "description": "ordonnée en points" },
        })
    };
    let click_opts = json!({
        "button": { "type": "string", "enum": ["left", "right", "middle"], "description": "bouton (left par défaut)" },
        "double": { "type": "boolean", "description": "double clic" },
        "modifiers": { "type": "array", "items": { "type": "string", "enum": ["ctrl", "shift", "alt", "cmd"] } },
    });
    let merge = |mut a: Value, b: &Value| {
        if let (Some(a), Some(b)) = (a.as_object_mut(), b.as_object()) {
            for (k, v) in b {
                a.insert(k.clone(), v.clone());
            }
        }
        a
    };
    let find = json!({
        "label": { "type": "string", "description": "texte du widget (exact de préférence, sinon contenu ; insensible à la casse)" },
        "role": { "type": "string", "description": "rôle AccessKit facultatif : Button, CheckBox, TextInput, Label…" },
        "index": { "type": "integer", "description": "rang parmi les widgets trouvés (0 par défaut)" },
        "id": { "type": "string", "description": "id affiché par ui_tree (à la place de label)" },
    });
    vec![
        Tool::new(
            "screenshot",
            "Capture la fenêtre de l'application (PNG). Par défaut, 1 pixel = 1 point : les coordonnées \
             lues sur l'image servent directement à click. Zone facultative (x, y, width, height en points).",
            schema(
                merge(
                    xy(),
                    &json!({
                        "width": { "type": "number" }, "height": { "type": "number" },
                        "native": { "type": "boolean", "description": "résolution physique d'origine" },
                        "max_width": { "type": "integer", "description": "largeur maximale en pixels (1600 par défaut)" },
                    }),
                ),
                &[],
            ),
        ),
        Tool::new(
            "ui_tree",
            "Liste les widgets visibles (rôle, libellé, valeur, position en points, état). \
             `filter` restreint par texte ou rôle ; `all` inclut les conteneurs sans texte.",
            schema(
                json!({
                    "filter": { "type": "string" },
                    "all": { "type": "boolean" },
                }),
                &[],
            ),
        ),
        Tool::new(
            "click_widget",
            "Clique au centre d'un widget trouvé par son libellé (ou son id de ui_tree).",
            schema(merge(find.clone(), &click_opts), &[]),
        ),
        Tool::new(
            "wait_for",
            "Attend qu'un widget apparaisse (ou disparaisse avec gone=true), par libellé ou id.",
            schema(
                merge(
                    find,
                    &json!({
                        "gone": { "type": "boolean" },
                        "timeout_ms": { "type": "integer", "description": "5000 par défaut, 25000 au plus" },
                    }),
                ),
                &[],
            ),
        ),
        Tool::new(
            "click",
            "Clic à une position (en points).",
            schema(merge(xy(), &click_opts), &["x", "y"]),
        ),
        Tool::new(
            "hover",
            "Déplace le pointeur (survol, bulles d'aide avec hold_ms ≈ 800).",
            schema(merge(xy(), &json!({ "hold_ms": { "type": "integer" } })), &["x", "y"]),
        ),
        Tool::new(
            "drag",
            "Glisser-déposer de (x, y) à (to_x, to_y).",
            schema(
                merge(
                    xy(),
                    &json!({
                        "to_x": { "type": "number" }, "to_y": { "type": "number" },
                        "steps": { "type": "integer" },
                        "button": { "type": "string", "enum": ["left", "right", "middle"] },
                        "modifiers": { "type": "array", "items": { "type": "string" } },
                    }),
                ),
                &["x", "y", "to_x", "to_y"],
            ),
        ),
        Tool::new(
            "scroll",
            "Molette au-dessus de (x, y). dy négatif : descendre ; dx : horizontal (en points).",
            schema(
                merge(
                    xy(),
                    &json!({
                        "dx": { "type": "number" }, "dy": { "type": "number" },
                        "modifiers": { "type": "array", "items": { "type": "string" } },
                    }),
                ),
                &["x", "y"],
            ),
        ),
        Tool::new(
            "type_text",
            "Saisit du texte dans le champ qui a le focus (cliquer d'abord dans le champ). \
             Les retours à la ligne envoient Entrée.",
            schema(json!({ "text": { "type": "string" } }), &["text"]),
        ),
        Tool::new(
            "press_key",
            "Appuie sur une touche ou un raccourci : « Enter », « Escape », « Tab », « F5 », \
             « Ctrl+S », « Shift+ArrowDown »…",
            schema(
                json!({
                    "key": { "type": "string" },
                    "times": { "type": "integer", "description": "répétitions (1 par défaut)" },
                    "modifiers": { "type": "array", "items": { "type": "string" } },
                }),
                &["key"],
            ),
        ),
        Tool::new(
            "wait",
            "Attend un délai (ms, 500 par défaut, 20000 au plus), par exemple après une action lente.",
            schema(json!({ "ms": { "type": "integer" } }), &[]),
        ),
        Tool::new(
            "get_logs",
            "Dernières lignes du journal de l'application (erreurs, plantages).",
            schema(
                json!({
                    "lines": { "type": "integer", "description": "80 par défaut" },
                    "filter": { "type": "string", "description": "ne garder que les lignes contenant ce texte" },
                }),
                &[],
            ),
        ),
    ]
}
