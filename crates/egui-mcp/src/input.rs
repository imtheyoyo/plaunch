//! Injection d'événements souris et clavier, image par image.
//!
//! Chaque action est une suite d'étapes : un lot d'événements par image,
//! séparés par une image vide pour qu'egui les traite comme une vraie
//! séquence (appui puis relâchement, etc.). La réponse est envoyée deux
//! images après la dernière étape, quand l'interface a été redessinée.

use std::collections::VecDeque;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, Vec2};
use serde_json::Value;

use crate::Reply;

enum Step {
    Events(Vec<Event>),
    Frames(u32),
    Millis(u64),
}

pub struct Op {
    steps: VecDeque<Step>,
    modifiers: Modifiers,
    reply: Option<Sender<Reply>>,
    description: String,
}

impl Op {
    fn new(description: impl Into<String>, modifiers: Modifiers) -> Self {
        Op {
            steps: VecDeque::new(),
            modifiers,
            reply: None,
            description: description.into(),
        }
    }

    fn events(mut self, ev: Vec<Event>) -> Self {
        self.steps.push_back(Step::Events(ev));
        self
    }

    fn millis(mut self, ms: u64) -> Self {
        if ms > 0 {
            self.steps.push_back(Step::Millis(ms.min(10_000)));
        }
        self
    }

    pub fn with_reply(mut self, tx: Sender<Reply>) -> Self {
        // Laisse l'interface se redessiner avant de répondre.
        self.steps.push_back(Step::Frames(2));
        self.reply = Some(tx);
        self
    }

    pub fn describe(mut self, text: String) -> Self {
        self.description = text;
        self
    }
}

#[derive(Default)]
pub struct Driver {
    queue: VecDeque<Op>,
    frames: u32,
    until: Option<Instant>,
}

impl Driver {
    pub fn push(&mut self, op: Op) {
        self.queue.push_back(op);
    }

    pub fn busy(&self) -> bool {
        !self.queue.is_empty()
    }

    pub fn inject(&mut self, raw: &mut RawInput) {
        loop {
            let Some(op) = self.queue.front_mut() else {
                return;
            };
            raw.focused = true;
            if self.frames > 0 {
                self.frames -= 1;
                return;
            }
            if let Some(t) = self.until {
                if Instant::now() < t {
                    return;
                }
                self.until = None;
            }
            match op.steps.pop_front() {
                Some(Step::Events(ev)) => {
                    if op.modifiers != Modifiers::NONE {
                        raw.events.push(Event::ModifiersChanged(op.modifiers));
                    }
                    raw.events.extend(ev);
                    self.frames = 1;
                    return;
                }
                Some(Step::Frames(n)) => self.frames = n,
                Some(Step::Millis(ms)) => {
                    self.until = Some(Instant::now() + Duration::from_millis(ms))
                }
                None => {
                    if let Some(op) = self.queue.pop_front() {
                        if op.modifiers != Modifiers::NONE {
                            raw.events.push(Event::ModifiersChanged(Modifiers::NONE));
                        }
                        if let Some(tx) = op.reply {
                            let _ = tx.send(Reply::Text(format!("ok : {}", op.description)));
                        }
                    }
                }
            }
        }
    }
}

pub fn is_input_tool(name: &str) -> bool {
    matches!(
        name,
        "click" | "hover" | "drag" | "scroll" | "type_text" | "press_key"
    )
}

pub fn build(name: &str, args: &Value) -> Result<Op, String> {
    match name {
        "click" => click_at(pos(args, "x", "y")?, args),
        "hover" => {
            let p = pos(args, "x", "y")?;
            let hold = args.get("hold_ms").and_then(Value::as_u64).unwrap_or(0);
            Ok(Op::new(
                format!("survol de ({:.0}, {:.0})", p.x, p.y),
                Modifiers::NONE,
            )
            .events(vec![Event::PointerMoved(p)])
            .millis(hold))
        }
        "drag" => {
            let from = pos(args, "x", "y")?;
            let to = pos(args, "to_x", "to_y")?;
            let mods = modifiers(args)?;
            let button = button(args)?;
            let n = args
                .get("steps")
                .and_then(Value::as_u64)
                .unwrap_or(8)
                .clamp(1, 60) as usize;
            let mut op = Op::new(
                format!(
                    "glisser ({:.0}, {:.0}) → ({:.0}, {:.0})",
                    from.x, from.y, to.x, to.y
                ),
                mods,
            )
            .events(vec![Event::PointerMoved(from)])
            .events(vec![press(from, button, true, mods)]);
            for i in 1..=n {
                let p = from + (to - from) * (i as f32 / n as f32);
                op = op.events(vec![Event::PointerMoved(p)]);
            }
            Ok(op.events(vec![press(to, button, false, mods)]))
        }
        "scroll" => {
            let p = pos(args, "x", "y")?;
            let num = |k: &str| args.get(k).and_then(Value::as_f64).unwrap_or(0.0) as f32;
            let delta = Vec2::new(num("dx"), num("dy"));
            if delta == Vec2::ZERO {
                return Err("indiquer dx et/ou dy (en points)".into());
            }
            let mods = modifiers(args)?;
            Ok(
                Op::new(format!("défilement de ({}, {})", delta.x, delta.y), mods)
                    .events(vec![Event::PointerMoved(p)])
                    .events(vec![Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta,
                        phase: egui::TouchPhase::Move,
                        source: egui::MouseWheelSource::Wheel,
                        modifiers: mods,
                    }]),
            )
        }
        "type_text" => {
            let text = args
                .get("text")
                .and_then(Value::as_str)
                .ok_or("argument « text » manquant")?;
            let mut op = Op::new(
                format!("saisie de {} caractère(s)", text.chars().count()),
                Modifiers::NONE,
            );
            for (i, line) in text.split('\n').enumerate() {
                if i > 0 {
                    op = key_steps(op, Key::Enter, Modifiers::NONE);
                }
                if !line.is_empty() {
                    op = op.events(vec![Event::Text(line.to_string())]);
                }
            }
            Ok(op)
        }
        "press_key" => {
            let spec = args
                .get("key")
                .and_then(Value::as_str)
                .ok_or("argument « key » manquant (ex. « Enter », « Ctrl+S », « F5 »)")?;
            let (key, mut mods) = parse_key(spec)?;
            let extra = modifiers(args)?;
            mods |= extra;
            let times = args
                .get("times")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .clamp(1, 50);
            let mut op = Op::new(format!("touche {spec}"), mods);
            for _ in 0..times {
                op = match (mods.command, key) {
                    // egui attend des événements dédiés pour le presse-papiers.
                    (true, Key::C) if !mods.shift && !mods.alt => op.events(vec![Event::Copy]),
                    (true, Key::X) if !mods.shift && !mods.alt => op.events(vec![Event::Cut]),
                    _ => key_steps(op, key, mods),
                };
            }
            Ok(op)
        }
        _ => Err(format!("outil inconnu : {name}")),
    }
}

/// Clic (simple, double, droit…) à une position en points.
pub fn click_at(p: Pos2, args: &Value) -> Result<Op, String> {
    let mods = modifiers(args)?;
    let button = button(args)?;
    let double = args.get("double").and_then(Value::as_bool).unwrap_or(false);
    let label = if double { "double clic" } else { "clic" };
    let mut op = Op::new(format!("{label} en ({:.0}, {:.0})", p.x, p.y), mods)
        .events(vec![Event::PointerMoved(p)]);
    for _ in 0..if double { 2 } else { 1 } {
        op = op
            .events(vec![press(p, button, true, mods)])
            .events(vec![press(p, button, false, mods)]);
    }
    Ok(op)
}

fn key_steps(op: Op, key: Key, mods: Modifiers) -> Op {
    let ev = |pressed| Event::Key {
        key,
        physical_key: Some(key),
        pressed,
        repeat: false,
        modifiers: mods,
    };
    op.events(vec![ev(true)]).events(vec![ev(false)])
}

fn press(pos: Pos2, button: PointerButton, pressed: bool, modifiers: Modifiers) -> Event {
    Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers,
    }
}

fn pos(args: &Value, kx: &str, ky: &str) -> Result<Pos2, String> {
    let x = args.get(kx).and_then(Value::as_f64);
    let y = args.get(ky).and_then(Value::as_f64);
    match (x, y) {
        (Some(x), Some(y)) => Ok(Pos2::new(x as f32, y as f32)),
        _ => Err(format!("arguments « {kx} » et « {ky} » requis (en points)")),
    }
}

fn button(args: &Value) -> Result<PointerButton, String> {
    match args.get("button").and_then(Value::as_str).unwrap_or("left") {
        "left" | "gauche" => Ok(PointerButton::Primary),
        "right" | "droit" => Ok(PointerButton::Secondary),
        "middle" | "milieu" => Ok(PointerButton::Middle),
        b => Err(format!("bouton inconnu : {b} (left, right, middle)")),
    }
}

fn modifier(name: &str, m: &mut Modifiers) -> bool {
    match name.to_lowercase().as_str() {
        "ctrl" | "control" | "ctl" => {
            m.ctrl = true;
            m.command = true;
        }
        "shift" | "maj" => m.shift = true,
        "alt" => m.alt = true,
        "cmd" | "command" | "meta" | "win" => {
            m.mac_cmd = true;
            m.command = true;
        }
        _ => return false,
    }
    true
}

fn modifiers(args: &Value) -> Result<Modifiers, String> {
    let mut m = Modifiers::NONE;
    if let Some(list) = args.get("modifiers").and_then(Value::as_array) {
        for v in list {
            let name = v.as_str().unwrap_or_default();
            if !modifier(name, &mut m) {
                return Err(format!(
                    "modificateur inconnu : {name} (ctrl, shift, alt, cmd)"
                ));
            }
        }
    }
    Ok(m)
}

/// « Ctrl+Shift+S » → (S, ctrl+shift).
fn parse_key(spec: &str) -> Result<(Key, Modifiers), String> {
    let mut mods = Modifiers::NONE;
    let parts: Vec<&str> = spec.split('+').map(str::trim).collect();
    let (last, prefix) = parts.split_last().ok_or("touche vide")?;
    for p in prefix {
        if !modifier(p, &mut mods) {
            return Err(format!("modificateur inconnu : {p}"));
        }
    }
    let name = match last.to_lowercase().as_str() {
        "esc" | "echap" | "échap" => "Escape".to_string(),
        "entree" | "entrée" | "return" => "Enter".to_string(),
        "del" | "suppr" => "Delete".to_string(),
        "up" | "haut" => "ArrowUp".to_string(),
        "down" | "bas" => "ArrowDown".to_string(),
        "left" | "gauche" => "ArrowLeft".to_string(),
        "right" | "droite" => "ArrowRight".to_string(),
        "backspace" | "retour" => "Backspace".to_string(),
        "pgup" | "pageup" => "PageUp".to_string(),
        "pgdn" | "pagedown" => "PageDown".to_string(),
        "space" | "espace" => "Space".to_string(),
        s if s.len() == 1 => s.to_ascii_uppercase(),
        _ => last.to_string(),
    };
    let key = Key::from_name(&name)
        .or_else(|| {
            // Noms insensibles à la casse : « enter », « f5 », « tab »…
            let mut c = name.chars();
            let cap: String = c
                .next()
                .map(|f| {
                    f.to_uppercase()
                        .chain(c.flat_map(char::to_lowercase))
                        .collect()
                })
                .unwrap_or_default();
            Key::from_name(&cap).or_else(|| Key::from_name(&name.to_ascii_uppercase()))
        })
        .ok_or_else(|| format!("touche inconnue : {last}"))?;
    Ok((key, mods))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        assert_eq!(parse_key("Enter").unwrap().0, Key::Enter);
        assert_eq!(parse_key("enter").unwrap().0, Key::Enter);
        assert_eq!(parse_key("f5").unwrap().0, Key::F5);
        assert_eq!(parse_key("Échap").unwrap().0, Key::Escape);
        let (k, m) = parse_key("Ctrl+Shift+s").unwrap();
        assert_eq!(k, Key::S);
        assert!(m.ctrl && m.shift && m.command);
        assert!(parse_key("Hyper+Q").is_err());
    }

    #[test]
    fn click_sequence() {
        let tx = std::sync::mpsc::channel().0;
        let mut d = Driver::default();
        d.push(
            click_at(Pos2::new(5.0, 6.0), &serde_json::json!({}))
                .unwrap()
                .with_reply(tx),
        );
        let mut seen = Vec::new();
        for _ in 0..20 {
            let mut raw = RawInput::default();
            d.inject(&mut raw);
            seen.extend(raw.events);
        }
        assert!(!d.busy());
        assert_eq!(seen.len(), 3);
        assert!(matches!(
            seen[1],
            Event::PointerButton { pressed: true, .. }
        ));
        assert!(matches!(
            seen[2],
            Event::PointerButton { pressed: false, .. }
        ));
    }
}
