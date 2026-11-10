//! Serveur HTTP minimal (transport MCP « Streamable HTTP » sans flux SSE) et
//! client utilisé par la passerelle stdio.

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::protocol::Server;

const MAX_BODY: usize = 16 * 1024 * 1024;

/// Lance l'écoute sur 127.0.0.1 ; renvoie l'adresse réelle.
pub fn serve(server: Server) -> io::Result<SocketAddr> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, server.options().port))?;
    let addr = listener.local_addr()?;
    let server = Arc::new(server);
    std::thread::Builder::new()
        .name("mcp-http".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let server = server.clone();
                let _ = std::thread::Builder::new()
                    .name("mcp-conn".into())
                    .spawn(move || {
                        let _ = handle(stream, &server);
                    });
            }
        })?;
    Ok(addr)
}

struct Request {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn read_request(stream: &mut BufReader<TcpStream>) -> io::Result<Option<Request>> {
    let mut line = String::new();
    if stream.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    let mut headers = HashMap::new();
    loop {
        let mut h = String::new();
        if stream.read_line(&mut h)? == 0 {
            break;
        }
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    let len: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if len > MAX_BODY {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "corps trop volumineux",
        ));
    }
    let mut body = vec![0; len];
    stream.read_exact(&mut body)?;
    Ok(Some(Request {
        method,
        path,
        headers,
        body,
    }))
}

fn respond(
    stream: &mut TcpStream,
    status: &str,
    ctype: &str,
    body: &[u8],
    keep: bool,
) -> io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\n\
         Allow: POST, OPTIONS\r\nConnection: {}\r\n\r\n",
        body.len(),
        if keep { "keep-alive" } else { "close" }
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

/// Origine acceptée : absente, ou provenant de la machine elle-même.
fn origin_ok(origin: Option<&String>) -> bool {
    let Some(o) = origin else { return true };
    let o = o.to_ascii_lowercase();
    o == "null"
        || [
            "http://127.0.0.1",
            "http://localhost",
            "https://127.0.0.1",
            "https://localhost",
            "http://[::1]",
        ]
        .iter()
        .any(|p| o == *p || o.starts_with(&format!("{p}:")))
}

fn handle(stream: TcpStream, server: &Server) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(120)))?;
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);
    while let Some(req) = read_request(&mut reader)? {
        let keep = !req
            .headers
            .get("connection")
            .is_some_and(|c| c.eq_ignore_ascii_case("close"));
        let json = "application/json";
        if !origin_ok(req.headers.get("origin")) {
            respond(
                &mut writer,
                "403 Forbidden",
                "text/plain",
                b"origine refusee",
                false,
            )?;
            return Ok(());
        }
        if let Some(token) = &server.options().token {
            let bearer = req.headers.get("authorization").and_then(|a| {
                a.strip_prefix("Bearer ")
                    .or_else(|| a.strip_prefix("bearer "))
            });
            let ok =
                bearer == Some(token.as_str()) || req.headers.get("x-mcp-token") == Some(token);
            if !ok {
                respond(
                    &mut writer,
                    "401 Unauthorized",
                    "text/plain",
                    b"jeton manquant ou invalide",
                    false,
                )?;
                return Ok(());
            }
        }
        let path = req.path.split('?').next().unwrap_or_default();
        if path != "/mcp" && path != "/" {
            respond(
                &mut writer,
                "404 Not Found",
                "text/plain",
                b"utiliser /mcp",
                keep,
            )?;
        } else {
            match req.method.as_str() {
                "POST" => match serde_json::from_slice::<Value>(&req.body) {
                    Ok(msg) => match server.handle(msg) {
                        Some(out) => {
                            let body = serde_json::to_vec(&out).unwrap_or_default();
                            respond(&mut writer, "200 OK", json, &body, keep)?;
                        }
                        None => respond(&mut writer, "202 Accepted", json, b"", keep)?,
                    },
                    Err(e) => {
                        let body = serde_json::json!({
                            "jsonrpc": "2.0", "id": null,
                            "error": { "code": -32700, "message": format!("JSON invalide : {e}") }
                        });
                        respond(
                            &mut writer,
                            "400 Bad Request",
                            json,
                            body.to_string().as_bytes(),
                            keep,
                        )?;
                    }
                },
                "OPTIONS" => respond(&mut writer, "204 No Content", "text/plain", b"", keep)?,
                "DELETE" => respond(&mut writer, "200 OK", "text/plain", b"", keep)?,
                // Pas de flux d'événements côté serveur.
                _ => respond(
                    &mut writer,
                    "405 Method Not Allowed",
                    "text/plain",
                    b"",
                    keep,
                )?,
            }
        }
        if !keep {
            break;
        }
    }
    Ok(())
}

/// Envoie un message JSON-RPC au serveur ; renvoie (code HTTP, corps).
pub fn post(addr: SocketAddr, token: Option<&str>, body: &str) -> io::Result<(u16, String)> {
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))?;
    stream.set_read_timeout(Some(Duration::from_secs(90)))?;
    let auth = token
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    let req = format!(
        "POST /mcp HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\n{auth}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(req.as_bytes())?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw)?;
    let text = String::from_utf8_lossy(&raw);
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    Ok((status, body.to_string()))
}

#[cfg(test)]
mod tests {
    use super::origin_ok;

    #[test]
    fn origins() {
        assert!(origin_ok(None));
        assert!(origin_ok(Some(&"http://localhost:3000".into())));
        assert!(origin_ok(Some(&"http://127.0.0.1".into())));
        assert!(!origin_ok(Some(&"http://localhost.evil.com".into())));
        assert!(!origin_ok(Some(&"https://example.org".into())));
    }
}
