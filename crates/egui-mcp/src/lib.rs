//! # egui-mcp
//!
//! Serveur MCP (Model Context Protocol) intégré à une application egui, pour
//! qu'une IA (Claude Code, Claude Desktop…) pilote l'application pendant les
//! tests de débogage : capture d'écran, arbre des widgets, clics, saisie,
//! journaux, plus les outils propres à l'application.
//!
//! ## Fonctionnement
//!
//! - Un fil d'exécution écoute en HTTP sur `127.0.0.1` (transport MCP
//!   « Streamable HTTP », réponses JSON simples).
//! - Les appels d'outils sont transmis au fil de l'interface par un canal ;
//!   l'interface est réveillée par `request_repaint`.
//! - Les événements (souris, clavier) sont injectés dans
//!   [`egui::RawInput`] depuis `eframe::App::raw_input_hook`, image par image,
//!   comme s'ils venaient de l'utilisateur.
//! - L'arbre des widgets vient d'AccessKit (activé par le serveur).
//!
//! ## Intégration
//!
//! ```ignore
//! // Création :
//! let bridge = egui_mcp::Bridge::start(&cc.egui_ctx, options, mes_outils())?;
//!
//! // Dans eframe::App :
//! fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
//!     if let Some(b) = &mut self.mcp { b.raw_input_hook(raw); }
//! }
//! fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
//!     for call in self.mcp.as_mut().map(|b| b.take_calls()).unwrap_or_default() {
//!         match call.name.as_str() {
//!             "get_state" => call.json(self.state_json()),
//!             _ => {} // un appel abandonné répond « outil non pris en charge »
//!         }
//!     }
//!     // … dessin de l'interface …
//!     if let Some(b) = &mut self.mcp { b.end_frame(ui.ctx()); }
//! }
//! ```
//!
//! ## Sécurité
//!
//! Écoute en boucle locale uniquement, jeton facultatif (`Authorization:
//! Bearer …`), en-tête `Origin` refusé s'il ne vient pas de la machine
//! (protection contre le « DNS rebinding »). À n'activer que pour les tests.

mod base64;
mod http;
mod image;
mod input;
mod protocol;
pub mod stdio;
mod tree;

use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub use serde_json::{json, Value};

/// Port par défaut si l'application n'en choisit pas.
pub const DEFAULT_PORT: u16 = 47800;

/// Délai maximal d'attente d'une réponse de l'interface.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Réglages du serveur.
#[derive(Clone, Debug)]
pub struct Options {
    /// Nom annoncé au client MCP.
    pub name: String,
    /// Version annoncée au client MCP.
    pub version: String,
    /// Port d'écoute sur 127.0.0.1 (0 : port libre au hasard).
    pub port: u16,
    /// Jeton exigé dans `Authorization: Bearer …` (ou `X-MCP-Token`).
    pub token: Option<String>,
    /// Journal de l'application, lu par l'outil `get_logs`.
    pub log_file: Option<PathBuf>,
    /// Consignes envoyées au client à l'initialisation.
    pub instructions: String,
}

impl Options {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Options {
            name: name.into(),
            version: version.into(),
            port: DEFAULT_PORT,
            token: None,
            log_file: None,
            instructions: String::new(),
        }
    }
}

/// Outil propre à l'application, déclaré au client MCP.
#[derive(Clone, Debug)]
pub struct Tool {
    pub name: String,
    pub description: String,
    /// Schéma JSON des arguments (objet).
    pub input_schema: Value,
}

impl Tool {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
    ) -> Self {
        Tool {
            name: name.into(),
            description: description.into(),
            input_schema,
        }
    }

    /// Outil sans argument.
    pub fn simple(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self::new(
            name,
            description,
            json!({ "type": "object", "properties": {} }),
        )
    }
}

/// Résultat d'un outil.
#[derive(Debug, Clone)]
pub enum Reply {
    Text(String),
    Json(Value),
    /// Image PNG encodée en base64, avec un texte d'accompagnement.
    Image {
        png_base64: String,
        text: String,
    },
    Error(String),
}

/// Demande transmise par le serveur au fil de l'interface.
pub(crate) struct Job {
    pub name: String,
    pub args: Value,
    pub reply: Sender<Reply>,
}

/// Appel d'un outil propre à l'application, à traiter dans `ui()`.
///
/// Répondre avec [`Call::ok`], [`Call::json`] ou [`Call::error`]. Un appel
/// abandonné sans réponse renvoie « outil non pris en charge ».
pub struct Call {
    pub name: String,
    pub args: Value,
    reply: Option<Sender<Reply>>,
}

impl Call {
    fn new(job: Job) -> Self {
        Call {
            name: job.name,
            args: job.args,
            reply: Some(job.reply),
        }
    }

    /// Argument texte.
    pub fn str(&self, key: &str) -> Option<&str> {
        self.args.get(key).and_then(Value::as_str)
    }

    /// Argument booléen.
    pub fn bool(&self, key: &str) -> Option<bool> {
        self.args.get(key).and_then(Value::as_bool)
    }

    /// Argument entier positif.
    pub fn u64(&self, key: &str) -> Option<u64> {
        self.args.get(key).and_then(Value::as_u64)
    }

    pub fn reply(mut self, reply: Reply) {
        if let Some(tx) = self.reply.take() {
            let _ = tx.send(reply);
        }
    }

    pub fn ok(self, text: impl Into<String>) {
        self.reply(Reply::Text(text.into()));
    }

    pub fn json(self, value: Value) {
        self.reply(Reply::Json(value));
    }

    pub fn error(self, message: impl Into<String>) {
        self.reply(Reply::Error(message.into()));
    }
}

impl Drop for Call {
    fn drop(&mut self) {
        if let Some(tx) = self.reply.take() {
            let _ = tx.send(Reply::Error(format!(
                "outil « {} » non pris en charge par l'application",
                self.name
            )));
        }
    }
}

/// Demande qui a besoin de l'arbre des widgets de l'image suivante.
struct TreeJob {
    job: Job,
    /// Pour `wait_for` : échéance.
    deadline: Option<Instant>,
}

/// Lien entre le serveur MCP et l'interface. À garder dans l'application.
pub struct Bridge {
    ctx: egui::Context,
    addr: SocketAddr,
    jobs: Receiver<Job>,
    calls: Vec<Call>,
    tree_jobs: Vec<TreeJob>,
    input: input::Driver,
    /// Dernier arbre capturé (pour les clics par libellé).
    last_tree: Option<Arc<tree::Snapshot>>,
    /// Captures demandées pas encore reçues : avec wgpu, l'image n'est relue
    /// qu'aux images suivantes, qu'il faut donc continuer à dessiner.
    shots_pending: Arc<AtomicUsize>,
}

impl Bridge {
    /// Démarre le serveur HTTP et active AccessKit.
    pub fn start(ctx: &egui::Context, options: Options, tools: Vec<Tool>) -> io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let addr = http::serve(protocol::Server::new(options, tools, tx, ctx.clone()))?;
        ctx.enable_accesskit();
        Ok(Bridge {
            ctx: ctx.clone(),
            addr,
            jobs: rx,
            calls: Vec::new(),
            tree_jobs: Vec::new(),
            input: input::Driver::default(),
            last_tree: None,
            shots_pending: Arc::default(),
        })
    }

    /// Adresse d'écoute réelle.
    pub fn address(&self) -> SocketAddr {
        self.addr
    }

    /// Adresse à donner au client MCP.
    pub fn url(&self) -> String {
        format!("http://{}/mcp", self.addr)
    }

    /// À appeler depuis `eframe::App::raw_input_hook` : reçoit les demandes
    /// et injecte les événements simulés.
    pub fn raw_input_hook(&mut self, raw: &mut egui::RawInput) {
        while let Ok(job) = self.jobs.try_recv() {
            self.dispatch(job);
        }
        self.input.inject(raw);
    }

    /// Appels des outils propres à l'application, reçus depuis la dernière image.
    pub fn take_calls(&mut self) -> Vec<Call> {
        std::mem::take(&mut self.calls)
    }

    /// À appeler à la fin de `ui()`, une fois toute l'interface dessinée.
    pub fn end_frame(&mut self, ctx: &egui::Context) {
        // AccessKit peut avoir été coupé (lecteur d'écran fermé) : on le garde actif.
        ctx.enable_accesskit();
        if !self.tree_jobs.is_empty() {
            let snap = Arc::new(tree::Snapshot::capture(ctx));
            self.last_tree = Some(snap.clone());
            let jobs = std::mem::take(&mut self.tree_jobs);
            for tj in jobs {
                if let Some(keep) = self.handle_tree_job(tj, &snap) {
                    self.tree_jobs.push(keep);
                }
            }
        }
        if self.input.busy() || self.shots_pending.load(Ordering::SeqCst) > 0 {
            ctx.request_repaint();
        } else if !self.tree_jobs.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    fn dispatch(&mut self, job: Job) {
        match job.name.as_str() {
            "screenshot" => self.screenshot(job),
            "ui_tree" | "click_widget" => self.tree_jobs.push(TreeJob {
                job,
                deadline: None,
            }),
            "wait_for" => {
                let ms = job
                    .args
                    .get("timeout_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or(5000);
                let deadline = Some(Instant::now() + Duration::from_millis(ms.min(25_000)));
                self.tree_jobs.push(TreeJob { job, deadline });
            }
            name if input::is_input_tool(name) => match input::build(name, &job.args) {
                Ok(op) => self.input.push(op.with_reply(job.reply)),
                Err(e) => {
                    let _ = job.reply.send(Reply::Error(e));
                }
            },
            _ => self.calls.push(Call::new(job)),
        }
        self.ctx.request_repaint();
    }

    fn screenshot(&mut self, job: Job) {
        let ppp = self.ctx.pixels_per_point();
        let opts = image::ShotOptions::from_args(&job.args, ppp);
        let reply = job.reply;
        let pending = self.shots_pending.clone();
        pending.fetch_add(1, Ordering::SeqCst);
        self.ctx.request_screenshot(move |img| {
            pending.fetch_sub(1, Ordering::SeqCst);
            // Encodage hors du fil de l'interface.
            std::thread::spawn(move || {
                let _ = reply.send(image::encode(&img, &opts));
            });
        });
        self.ctx.request_repaint();
    }

    /// Traite une demande liée à l'arbre ; la renvoie si elle doit attendre.
    fn handle_tree_job(&mut self, tj: TreeJob, snap: &tree::Snapshot) -> Option<TreeJob> {
        let args = &tj.job.args;
        match tj.job.name.as_str() {
            "ui_tree" => {
                let filter = args.get("filter").and_then(Value::as_str).unwrap_or("");
                let all = args.get("all").and_then(Value::as_bool).unwrap_or(false);
                let _ = tj.job.reply.send(Reply::Text(snap.render(filter, all)));
                None
            }
            "click_widget" => {
                match snap.find(args) {
                    Ok(node) => {
                        let pos = node.center();
                        let op = input::click_at(pos, args)
                            .map(|op| op.describe(format!("clic sur {}", node.describe())));
                        match op {
                            Ok(op) => self.input.push(op.with_reply(tj.job.reply)),
                            Err(e) => {
                                let _ = tj.job.reply.send(Reply::Error(e));
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tj.job.reply.send(Reply::Error(e));
                    }
                }
                None
            }
            "wait_for" => {
                let gone = args.get("gone").and_then(Value::as_bool).unwrap_or(false);
                let found = snap.find(args).is_ok();
                if found != gone {
                    let what = if gone { "disparu" } else { "présent" };
                    let _ = tj.job.reply.send(Reply::Text(format!("widget {what}")));
                    None
                } else if tj.deadline.is_some_and(|d| Instant::now() >= d) {
                    let _ = tj
                        .job
                        .reply
                        .send(Reply::Error("délai dépassé : condition non remplie".into()));
                    None
                } else {
                    Some(tj)
                }
            }
            _ => None,
        }
    }
}

/// Analyse les arguments de ligne de commande `--mcp`, `--mcp=PORT`,
/// `--mcp-token=JETON`. Renvoie `None` si `--mcp` est absent.
///
/// Le jeton peut aussi venir de la variable d'environnement `MCP_TOKEN`.
pub fn parse_args<I, S>(args: I) -> Option<(u16, Option<String>)>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut port = None;
    let mut token = std::env::var("MCP_TOKEN").ok().filter(|t| !t.is_empty());
    for a in args {
        let a = a.as_ref();
        if a == "--mcp" {
            port.get_or_insert(DEFAULT_PORT);
        } else if let Some(p) = a.strip_prefix("--mcp=") {
            port = Some(p.parse().unwrap_or(DEFAULT_PORT));
        } else if let Some(t) = a.strip_prefix("--mcp-token=") {
            token = Some(t.to_string()).filter(|t| !t.is_empty());
        }
    }
    port.map(|p| (p, token))
}

/// Vrai si l'argument concerne le serveur MCP (à ignorer par l'application).
pub fn is_mcp_arg(arg: &str) -> bool {
    arg == "--mcp"
        || arg == "--mcp-stdio"
        || arg.starts_with("--mcp=")
        || arg.starts_with("--mcp-token=")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args() {
        assert_eq!(parse_args(["a.xml"]), None);
        assert_eq!(parse_args(["--mcp"]).map(|p| p.0), Some(DEFAULT_PORT));
        let (port, token) = parse_args(["--mcp=1234", "--mcp-token=abc"]).unwrap();
        assert_eq!((port, token.as_deref()), (1234, Some("abc")));
        assert!(is_mcp_arg("--mcp-stdio"));
        assert!(!is_mcp_arg("PLaunch.xml"));
    }
}
