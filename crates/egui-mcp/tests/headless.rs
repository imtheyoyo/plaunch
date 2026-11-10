//! Test de bout en bout sans fenêtre : un contexte egui tourne dans un fil,
//! le test dialogue avec le serveur MCP en HTTP comme le ferait une IA.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use egui_mcp::{json, Bridge, Options, Tool, Value};

fn post(addr: SocketAddr, token: &str, body: &Value) -> (u16, Value) {
    let mut s = TcpStream::connect(addr).unwrap();
    let body = body.to_string();
    write!(
        s,
        "POST /mcp HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut raw = String::new();
    s.read_to_string(&mut raw).unwrap();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap();
    let code = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    (code, serde_json::from_str(body).unwrap_or(Value::Null))
}

fn call(addr: SocketAddr, name: &str, args: Value) -> Value {
    let (code, v) = post(
        addr,
        "secret",
        &json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}),
    );
    assert_eq!(code, 200);
    v["result"].clone()
}

fn text(result: &Value) -> String {
    result["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn drive_headless_ui() {
    let clicks = Arc::new(AtomicU32::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let (addr_tx, addr_rx) = mpsc::channel();
    let typed = Arc::new(std::sync::Mutex::new(String::new()));

    let ui_thread = {
        let (clicks, stop, typed) = (clicks.clone(), stop.clone(), typed.clone());
        std::thread::spawn(move || {
            let ctx = egui::Context::default();
            let mut opts = Options::new("test", "0.0.1");
            opts.port = 0;
            opts.token = Some("secret".into());
            let tools = vec![Tool::simple("get_state", "état")];
            let mut bridge = Bridge::start(&ctx, opts, tools).unwrap();
            addr_tx.send(bridge.address()).unwrap();
            let mut text = String::new();
            let mut checked = false;
            while !stop.load(Ordering::SeqCst) {
                let mut raw = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400.0, 300.0),
                    )),
                    ..Default::default()
                };
                bridge.raw_input_hook(&mut raw);
                for c in bridge.take_calls() {
                    match c.name.as_str() {
                        "get_state" => c.json(json!({ "clicks": clicks.load(Ordering::SeqCst) })),
                        _ => drop(c),
                    }
                }
                let mut out = ctx.run_ui(raw, |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        if ui.button("Valider").clicked() {
                            clicks.fetch_add(1, Ordering::SeqCst);
                        }
                        ui.checkbox(&mut checked, "Option");
                        ui.text_edit_singleline(&mut text);
                    });
                    bridge.end_frame(ui.ctx());
                });
                out.textures_delta.clear();
                *typed.lock().unwrap() = text.clone();
                std::thread::sleep(Duration::from_millis(5));
            }
        })
    };
    let addr = addr_rx.recv().unwrap();

    // Jeton exigé.
    let (code, _) = post(
        addr,
        "faux",
        &json!({"jsonrpc":"2.0","id":1,"method":"ping"}),
    );
    assert_eq!(code, 401);

    // Initialisation et liste des outils.
    let (_, init) = post(
        addr,
        "secret",
        &json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}),
    );
    assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
    let (code, _) = post(
        addr,
        "secret",
        &json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
    assert_eq!(code, 202);
    let (_, list) = post(
        addr,
        "secret",
        &json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    );
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"screenshot") && names.contains(&"get_state"),
        "{names:?}"
    );

    // Arbre des widgets.
    let tree = text(&call(addr, "ui_tree", json!({})));
    assert!(tree.contains("\"Valider\""), "{tree}");
    assert!(tree.contains("CheckBox"), "{tree}");

    // Clic par libellé, puis vérification par l'outil de l'application.
    let r = call(addr, "click_widget", json!({"label": "valider"}));
    assert_eq!(r["isError"], false, "{r}");
    let state = call(addr, "get_state", json!({}));
    assert_eq!(state["structuredContent"]["clicks"], 1, "{state}");

    // Case à cocher.
    call(addr, "click_widget", json!({"label": "Option"}));
    let tree = text(&call(addr, "ui_tree", json!({"filter": "option"})));
    assert!(
        tree.contains("coché") && !tree.contains("décoché"),
        "{tree}"
    );

    // Saisie dans le champ texte.
    call(
        addr,
        "click_widget",
        json!({"role": "TextInput", "label": ""}),
    );
    call(addr, "type_text", json!({"text": "bonjour"}));
    call(addr, "press_key", json!({"key": "Backspace", "times": 2}));
    assert_eq!(typed.lock().unwrap().as_str(), "bonjo");

    // Attente d'un widget absent.
    let r = call(
        addr,
        "wait_for",
        json!({"label": "Inexistant", "timeout_ms": 200}),
    );
    assert_eq!(r["isError"], true);

    // Outil inconnu de l'application.
    let r = call(addr, "nexiste_pas", json!({}));
    assert_eq!(r["isError"], true);

    stop.store(true, Ordering::SeqCst);
    ui_thread.join().unwrap();
}
