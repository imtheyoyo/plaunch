//! Déclenchement des alarmes des notes, y compris fenêtre masquée.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use crate::i18n::{tr, trf};
use crate::notes::{Note, INPUT_FORMAT};
use crate::state::Shared;

/// Forme des rappels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReminderStyle {
    /// Notification Windows (reste affichée jusqu'à fermeture).
    #[default]
    Toast,
    /// Boîte de message au premier plan.
    Dialog,
    Both,
}

impl ReminderStyle {
    pub const ALL: [ReminderStyle; 3] = [
        ReminderStyle::Toast,
        ReminderStyle::Dialog,
        ReminderStyle::Both,
    ];

    pub fn key(self) -> &'static str {
        match self {
            ReminderStyle::Toast => "toast",
            ReminderStyle::Dialog => "dialog",
            ReminderStyle::Both => "both",
        }
    }

    pub fn from_key(s: &str) -> ReminderStyle {
        match s.trim().to_ascii_lowercase().as_str() {
            "dialog" | "boite" | "messagebox" => ReminderStyle::Dialog,
            "both" | "les deux" => ReminderStyle::Both,
            _ => ReminderStyle::Toast,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ReminderStyle::Toast => tr("Notification Windows"),
            ReminderStyle::Dialog => tr("Boîte de message"),
            ReminderStyle::Both => tr("Les deux"),
        }
    }
}

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
                shared.error(e);
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
        let style = *shared.reminders.lock().unwrap();
        notify(&note, next, style);
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

fn notify(note: &Note, next: Option<String>, style: ReminderStyle) {
    let title = display_title(note);
    let text = note.text.trim();
    let text = if text == title { "" } else { text };
    let next = next.map(|n| trf("Prochain rappel : {}", &[&n]));
    let mut toast_ok = false;
    if matches!(style, ReminderStyle::Toast | ReminderStyle::Both) {
        toast_ok = imp::toast(&title, text, next.as_deref()).is_ok();
    }
    if style == ReminderStyle::Toast && toast_ok {
        return;
    }
    // Boîte de message (choisie, ou notification impossible).
    let mut body = title.clone();
    if !text.is_empty() {
        body.push_str("\n\n");
        body.push_str(text);
    }
    if let Some(n) = next {
        body.push_str(&format!("\n\n{n}"));
    }
    // Une boîte par rappel, dans son propre thread : elle ne bloque rien.
    std::thread::spawn(move || imp::message_box(tr("PLaunch · Rappel"), &body));
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MB_TOPMOST,
    };

    /// Notification Windows au format « rappel » : elle reste affichée
    /// jusqu'à ce que l'utilisateur la ferme. L'identifiant d'application de
    /// PowerShell évite toute installation (raccourci, registre).
    pub fn toast(title: &str, text: &str, next: Option<&str>) -> Result<(), String> {
        use tauri_winrt_notification::{Duration, Scenario, Sound, Toast};
        let mut t = Toast::new(Toast::POWERSHELL_APP_ID)
            .title(title)
            .duration(Duration::Long)
            .scenario(Scenario::Reminder)
            .sound(Some(Sound::Reminder));
        if !text.is_empty() {
            t = t.text1(text);
        }
        if let Some(n) = next {
            t = t.text2(n);
        }
        t.show().map_err(|e| e.to_string())
    }

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
    pub fn toast(title: &str, text: &str, _next: Option<&str>) -> Result<(), String> {
        eprintln!("{title} : {text}");
        Ok(())
    }

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
