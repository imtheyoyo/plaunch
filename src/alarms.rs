//! Déclenchement des alarmes des notes, y compris fenêtre masquée.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use crate::i18n::{tr, trf};
use crate::notes::{Note, INPUT_FORMAT};
use crate::state::Shared;

/// Vérifie les alarmes au démarrage (alarmes manquées) puis toutes les 15 s.
pub fn spawn(shared: Arc<Shared>) {
    std::thread::spawn(move || loop {
        check(&shared);
        for _ in 0..15 {
            std::thread::sleep(Duration::from_secs(1));
            if shared.quitting.load(Ordering::SeqCst) {
                return;
            }
        }
    });
}

fn check(shared: &Shared) {
    let now = chrono::Local::now().naive_local();
    let fired = {
        let mut guard = shared.notes.lock().unwrap();
        let Some(notes) = guard.as_mut() else { return };
        let fired = notes.take_due(now);
        if !fired.is_empty() {
            if let Err(e) = notes.save() {
                shared.set_status(e);
            }
        }
        // État des alarmes après reprogrammation, pour le message.
        let next: Vec<Option<String>> = fired
            .iter()
            .map(|f| {
                notes
                    .items
                    .iter()
                    .find(|n| n.title == f.title && n.text == f.text)
                    .and_then(|n| n.alarm)
                    .map(|a| a.format(INPUT_FORMAT).to_string())
            })
            .collect();
        fired.into_iter().zip(next).collect::<Vec<_>>()
    };
    for (note, next) in fired {
        let title = display_title(&note);
        shared.set_status(trf("Rappel : {}", &[&title]));
        notify(&note, next);
    }
}

fn display_title(note: &Note) -> String {
    let t = note.title.trim();
    if !t.is_empty() {
        return t.to_string();
    }
    note.text
        .lines()
        .next()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .unwrap_or(tr("Note"))
        .to_string()
}

fn notify(note: &Note, next: Option<String>) {
    let title = display_title(note);
    let mut body = title.clone();
    if !note.text.trim().is_empty() && note.text.trim() != title {
        body.push_str("\n\n");
        body.push_str(note.text.trim());
    }
    if let Some(n) = next {
        body.push_str(&format!("\n\n{}", trf("Prochain rappel : {}", &[&n])));
    }
    // Une boîte par rappel, dans son propre thread : elle ne bloque rien.
    std::thread::spawn(move || imp::message_box(tr("PStart-rs · Rappel"), &body));
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MB_TOPMOST,
    };

    pub fn message_box(caption: &str, text: &str) {
        let caption: Vec<u16> = caption.encode_utf16().chain(std::iter::once(0)).collect();
        let text: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY : chaînes terminées par un zéro ; fenêtre parente nulle.
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                caption.as_ptr(),
                MB_OK | MB_ICONINFORMATION | MB_TOPMOST | MB_SETFOREGROUND,
            );
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn message_box(caption: &str, text: &str) {
        eprintln!("{caption} : {text}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles() {
        let mut n = Note::new([0; 3]);
        assert_eq!(display_title(&n), "Note");
        n.text = "\n".into();
        assert_eq!(display_title(&n), "Note");
        n.text = "Appeler le garage\nPour la Golf".into();
        assert_eq!(display_title(&n), "Appeler le garage");
        n.title = "Garage".into();
        assert_eq!(display_title(&n), "Garage");
    }
}
