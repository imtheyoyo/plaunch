//! Passerelle stdio → HTTP.
//!
//! Certains clients MCP (Claude Desktop notamment) lancent les serveurs comme
//! des programmes qui dialoguent sur l'entrée/sortie standard. L'application
//! peut alors être lancée avec `--mcp-stdio` : ce mode relaie chaque ligne
//! JSON-RPC vers le serveur HTTP de l'instance en cours, et la démarre si
//! besoin (avec `--mcp`).

use std::io::{self, BufRead, Write};
use std::net::{Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::http;

/// Relaie l'entrée standard vers `127.0.0.1:port` jusqu'à sa fermeture.
///
/// `launch` démarre l'application (une seule fois, à la première connexion
/// refusée).
pub fn run_bridge(
    port: u16,
    token: Option<&str>,
    mut launch: impl FnMut() -> io::Result<()>,
) -> io::Result<()> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut launched = false;
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let mut result = http::post(addr, token, &line);
        if result.is_err() && !launched {
            launched = true;
            match launch() {
                Ok(()) => {
                    // L'application met quelques secondes à ouvrir son port.
                    let start = Instant::now();
                    while start.elapsed() < Duration::from_secs(30) {
                        std::thread::sleep(Duration::from_millis(300));
                        result = http::post(addr, token, &line);
                        if result.is_ok() {
                            break;
                        }
                    }
                }
                Err(e) => result = Err(e),
            }
        }
        let id = serde_json::from_str::<Value>(&line)
            .ok()
            .and_then(|v| v.get("id").cloned());
        let out = match result {
            Ok((200, body)) if !body.trim().is_empty() => Some(body),
            Ok((200 | 202, _)) => None,
            Ok((code, body)) => id.map(|id| error(id, &format!("HTTP {code} : {body}"))),
            Err(e) => id.map(|id| {
                error(
                    id,
                    &format!(
                        "application injoignable sur {addr} ({e}) : est-elle lancée avec --mcp ?"
                    ),
                )
            }),
        };
        if let Some(out) = out {
            writeln!(stdout, "{}", out.trim())?;
            stdout.flush()?;
        }
    }
    Ok(())
}

fn error(id: Value, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32000, "message": message } })
        .to_string()
}
