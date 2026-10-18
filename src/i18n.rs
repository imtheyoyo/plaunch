//! Traduction de l'interface.
//!
//! Le texte français sert de clé : `tr("Annuler")` renvoie « Cancel » en
//! anglais. Les chaînes à paramètres passent par `trf`, avec des `{}`
//! remplacés dans l'ordre (ou `{0}`, `{1}`… si une langue doit les inverser).
//! Un test vérifie que chaque chaîne passée à `tr`/`trf` dans `src/` possède
//! sa traduction.

use std::collections::HashMap;
use std::fmt::{Display, Write};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

/// Choix de l'utilisateur (`language=` dans pstart-rs.ini).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    Auto,
    French,
    English,
}

impl Language {
    pub const ALL: [Language; 3] = [Language::Auto, Language::French, Language::English];

    pub fn key(self) -> &'static str {
        match self {
            Language::Auto => "auto",
            Language::French => "fr",
            Language::English => "en",
        }
    }

    pub fn from_key(s: &str) -> Language {
        match s.trim().to_ascii_lowercase().as_str() {
            "fr" | "french" | "français" => Language::French,
            "en" | "english" | "anglais" => Language::English,
            _ => Language::Auto,
        }
    }

    /// Nom affiché : chaque langue dans sa propre langue.
    pub fn label(self) -> &'static str {
        match self {
            Language::Auto => tr("Automatique (système)"),
            Language::French => "Français",
            Language::English => "English",
        }
    }

    /// Langue effective (résout `Auto` avec la langue du système).
    pub fn resolve(self) -> Language {
        match self {
            Language::Auto => {
                if system_is_french() {
                    Language::French
                } else {
                    Language::English
                }
            }
            l => l,
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Change la langue de l'interface (prend effet à l'image suivante).
pub fn set_language(lang: Language) {
    let v = match lang.resolve() {
        Language::English => 1,
        _ => 0,
    };
    CURRENT.store(v, Ordering::SeqCst);
}

pub fn is_english() -> bool {
    CURRENT.load(Ordering::SeqCst) == 1
}

/// Traduit un texte fixe.
pub fn tr(fr: &'static str) -> &'static str {
    if !is_english() {
        return fr;
    }
    table().get(fr).copied().unwrap_or(fr)
}

/// Version anglaise d'un texte, quelle que soit la langue courante.
pub fn english(fr: &'static str) -> &'static str {
    table().get(fr).copied().unwrap_or(fr)
}

/// Traduit un texte à paramètres puis le remplit.
pub fn trf(fr: &'static str, args: &[&dyn Display]) -> String {
    fill(tr(fr), args)
}

fn fill(pattern: &str, args: &[&dyn Display]) -> String {
    let mut out = String::with_capacity(pattern.len() + 16);
    let mut next = 0;
    let mut rest = pattern;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) if after[..end].chars().all(|c| c.is_ascii_digit()) => {
                let idx = if end == 0 {
                    next += 1;
                    next - 1
                } else {
                    after[..end].parse().unwrap_or(usize::MAX)
                };
                if let Some(a) = args.get(idx) {
                    let _ = write!(out, "{a}");
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(windows)]
fn system_is_french() -> bool {
    // SAFETY : simple lecture de la langue d'interface de l'utilisateur.
    let id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
    id & 0x3FF == 0x0C // LANG_FRENCH
}

#[cfg(not(windows))]
fn system_is_french() -> bool {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.is_empty())
        .is_some_and(|v| v.to_ascii_lowercase().starts_with("fr"))
}

fn table() -> &'static HashMap<&'static str, &'static str> {
    static TABLE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    TABLE.get_or_init(|| EN.iter().copied().collect())
}

/// Français → anglais.
const EN: &[(&str, &str)] = &[
    // Fenêtre et barre du haut
    (
        "PStart-rs · Lanceur portable",
        "PStart-rs · Portable launcher",
    ),
    ("Rechercher… (Ctrl+F)", "Search… (Ctrl+F)"),
    ("Arborescence", "Tree"),
    ("Plus utilisés", "Most used"),
    ("Récents", "Recent"),
    ("Notes", "Notes"),
    ("Réglages", "Settings"),
    ("Recharger le fichier", "Reload file"),
    ("Nouveau raccourci…", "New shortcut…"),
    ("Nouveau groupe…", "New group…"),
    ("Ajouter des fichiers…", "Add files…"),
    (
        "Ajouter (vous pouvez aussi déposer des fichiers ici)",
        "Add (you can also drop files here)",
    ),
    // États
    ("{} raccourcis chargés", "{} shortcuts loaded"),
    ("Notes illisibles : {}", "Unreadable notes: {}"),
    ("Démarrage avec Windows : {}", "Start with Windows: {}"),
    (
        "Réglages enregistrés (raccourci global et zone de notification : au prochain démarrage)",
        "Settings saved (global hotkey and notification area: on next start)",
    ),
    ("Réglages enregistrés", "Settings saved"),
    ("Supprimé", "Deleted"),
    ("Raccourci ajouté", "Shortcut added"),
    ("{} raccourcis ajoutés", "{} shortcuts added"),
    ("Enregistré", "Saved"),
    ("Lancé : {}", "Launched: {}"),
    (
        "Lancé : {} (statistiques non enregistrées : {})",
        "Launched: {} (statistics not saved: {})",
    ),
    ("Échec : {}", "Failed: {}"),
    ("Modification impossible : {}", "Cannot modify: {}"),
    ("Icône : {}", "Icon: {}"),
    // Sélecteurs de fichiers
    ("Ajouter des raccourcis", "Add shortcuts"),
    (
        "Choisir le programme ou le document",
        "Choose the program or document",
    ),
    ("Choisir le dossier de travail", "Choose the working folder"),
    ("Choisir une icône", "Choose an icon"),
    ("Icônes et programmes", "Icons and programs"),
    ("Tous les fichiers", "All files"),
    // Formulaire de raccourci
    ("Nouveau raccourci", "New shortcut"),
    ("Modifier le raccourci", "Edit shortcut"),
    ("Nouveau groupe", "New group"),
    ("Renommer le groupe", "Rename group"),
    ("Le nom est obligatoire.", "A name is required."),
    ("Le chemin est obligatoire.", "A path is required."),
    ("Nom", "Name"),
    ("Chemin", "Path"),
    ("Dossier", "Folder"),
    ("Parcourir…", "Browse…"),
    ("(dossier du programme)", "(program folder)"),
    ("Choisir le dossier", "Choose the folder"),
    (
        "Les chemins sur le même lecteur sont enregistrés en relatif.",
        "Paths on the same drive are stored as relative.",
    ),
    ("Enregistrer", "Save"),
    ("Annuler", "Cancel"),
    // Notes
    ("Notes indisponibles.", "Notes unavailable."),
    ("＋ Nouvelle note", "＋ New note"),
    ("Enregistrées dans {}", "Saved in {}"),
    (
        "Aucune note. Les alarmes s'affichent même fenêtre masquée.",
        "No notes yet. Alarms show up even when the window is hidden.",
    ),
    ("Titre", "Title"),
    ("Texte de la note", "Note text"),
    ("Couleur", "Color"),
    ("Alarme", "Alarm"),
    ("AAAA/MM/JJ HH:MM", "YYYY/MM/DD HH:MM"),
    (
        "Date invalide (AAAA/MM/JJ HH:MM)",
        "Invalid date (YYYY/MM/DD HH:MM)",
    ),
    ("une fois", "once"),
    ("chaque jour", "every day"),
    ("chaque semaine", "every week"),
    ("Rappel : {}", "Reminder: {}"),
    ("Note", "Note"),
    ("Prochain rappel : {}", "Next reminder: {}"),
    ("PStart-rs · Rappel", "PStart-rs · Reminder"),
    // Recherche, accueil, détails
    ("Aucun résultat.", "No results."),
    (
        "Glissez un fichier PStart.xml sur cette fenêtre,",
        "Drop a PStart.xml file on this window,",
    ),
    (
        "ou placez pstart-rs.exe à côté de votre PStart.xml.",
        "or put pstart-rs.exe next to your PStart.xml.",
    ),
    ("Sélectionnez un raccourci.", "Select a shortcut."),
    ("{} raccourcis", "{} shortcuts"),
    ("＋ Raccourci", "＋ Shortcut"),
    ("＋ Groupe", "＋ Group"),
    ("Renommer", "Rename"),
    ("Supprimer", "Delete"),
    ("Cible", "Target"),
    ("État", "Status"),
    ("présent", "found"),
    ("introuvable", "missing"),
    ("vérification…", "checking…"),
    ("Lancements", "Launches"),
    ("Dernier", "Last"),
    ("jamais", "never"),
    ("▶ Lancer", "▶ Launch"),
    ("Modifier…", "Edit…"),
    ("Emplacement", "Location"),
    ("Raccourcis", "Shortcuts"),
    ("Entrée : lancer", "Enter: launch"),
    ("F2 : modifier, Suppr : supprimer", "F2: edit, Del: delete"),
    (
        "Ctrl+F : rechercher, Échap : effacer",
        "Ctrl+F: search, Esc: clear",
    ),
    ("Glisser-déposer : réorganiser", "Drag and drop: reorder"),
    ("{} : afficher / masquer", "{}: show / hide"),
    ("Raccourci global : désactivé", "Global hotkey: disabled"),
    ("Réglages : {}", "Settings: {}"),
    (
        "Supprimer le groupe « {} » et ses {} raccourcis ?",
        "Delete the group “{}” and its {} shortcuts?",
    ),
    (
        "Supprimer le raccourci « {} » ?",
        "Delete the shortcut “{}”?",
    ),
    (
        "Le programme lui-même n'est pas supprimé.",
        "The program itself is not deleted.",
    ),
    ("Nouveau raccourci ici…", "New shortcut here…"),
    ("Nouveau groupe ici…", "New group here…"),
    ("Renommer…", "Rename…"),
    ("Supprimer…", "Delete…"),
    ("Cible introuvable", "Target not found"),
    ("Lancer", "Launch"),
    ("Ouvrir l'emplacement", "Open location"),
    ("Ajouter à PStart", "Add to PStart"),
    ("Menu Démarrer (utilisateur)", "Start menu (user)"),
    ("Menu Démarrer (tous)", "Start menu (all users)"),
    // Réglages
    ("Apparence", "Appearance"),
    ("Langue", "Language"),
    ("Automatique (système)", "Automatic (system)"),
    ("Thème", "Theme"),
    ("Système", "System"),
    ("Clair", "Light"),
    ("Sombre", "Dark"),
    ("Taille du texte", "Text size"),
    ("Icône", "Icon"),
    ("Forme", "Shape"),
    ("Lecture", "Play"),
    ("Étoile", "Star"),
    ("Grille", "Grid"),
    ("Éclair", "Bolt"),
    ("Bleu", "Blue"),
    ("Vert", "Green"),
    ("Orange", "Orange"),
    ("Violet", "Purple"),
    ("Rouge", "Red"),
    ("Gris", "Gray"),
    ("Fichier…", "File…"),
    (
        "Icône d'un fichier .ico ou .exe",
        "Icon from an .ico or .exe file",
    ),
    ("Retirer", "Remove"),
    ("Raccourci global", "Global hotkey"),
    ("désactivé", "disabled"),
    (
        "Ex. Ctrl+Shift+Space, Ctrl+Alt+P",
        "E.g. Ctrl+Shift+Space, Ctrl+Alt+P",
    ),
    ("Zone de notification", "Notification area"),
    (
        "Icône dans la zone de notification",
        "Icon in the notification area",
    ),
    (
        "La croix masque la fenêtre",
        "The close button hides the window",
    ),
    ("Démarrer masqué", "Start hidden"),
    ("Coin actif", "Hot corner"),
    ("délai", "delay"),
    ("taille du coin", "corner size"),
    ("Démarrer avec Windows", "Start with Windows"),
    (
        "Le raccourci global et l'icône de notification s'activent au prochain démarrage.",
        "The global hotkey and the notification icon are switched on at next start.",
    ),
    ("Fichier : {}", "File: {}"),
    ("coin haut gauche", "top-left corner"),
    ("coin haut droit", "top-right corner"),
    ("coin bas gauche", "bottom-left corner"),
    ("coin bas droit", "bottom-right corner"),
    ("bord haut", "top edge"),
    ("bord bas", "bottom edge"),
    ("bord gauche", "left edge"),
    ("bord droit", "right edge"),
    // Zone de notification
    ("Afficher PStart-rs", "Show PStart-rs"),
    ("(vide)", "(empty)"),
    ("Notes…", "Notes…"),
    ("Quitter PStart-rs", "Quit PStart-rs"),
    (
        "Démarrage avec Windows activé",
        "Start with Windows enabled",
    ),
    (
        "Démarrage avec Windows désactivé",
        "Start with Windows disabled",
    ),
    ("icône de notification : {}", "notification icon: {}"),
    // v0.6
    ("(aucun)", "(none)"),
    (
        "Aucun : tous les raccourcis vérifiés sont valides.",
        "None: every checked shortcut is valid.",
    ),
    ("Aucun lancement enregistré.", "No launches recorded."),
    (
        "Ctrl+1 à 9 : lancer un favori",
        "Ctrl+1 to 9: launch a favorite",
    ),
    ("Dernier lancement", "Last launch"),
    ("Démarrages de PStart", "PStart starts"),
    (
        "Flèches : naviguer, ouvrir ou fermer un groupe",
        "Arrows: move, open or close a group",
    ),
    ("Groupes", "Groups"),
    ("Introuvables", "Missing"),
    ("Jamais lancés ({})", "Never launched ({})"),
    ("Lancements enregistrés", "Recorded launches"),
    (
        "Lancer au démarrage de PStart-rs",
        "Launch when PStart-rs starts",
    ),
    ("Les plus lancés", "Most launched"),
    ("Raccourcis introuvables", "Missing shortcuts"),
    (
        "Retire ces raccourcis de PStart.xml",
        "Removes these shortcuts from PStart.xml",
    ),
    ("Retirer des favoris", "Remove from favorites"),
    (
        "Supprimer les {} raccourcis dont la cible est introuvable ?",
        "Delete the {} shortcuts whose target is missing?",
    ),
    ("Tout supprimer…", "Delete all…"),
    ("ce mois-ci", "this month"),
    ("cette année", "this year"),
    ("cette semaine", "this week"),
    ("{} (vérification…)", "{} (checking…)"),
    ("★ Retirer des favoris", "★ Remove from favorites"),
    ("Ajouter aux favoris", "Add to favorites"),
    ("Ajouté aux favoris", "Added to favorites"),
    ("Ancienne lettre", "Old letter"),
    ("Boîte de message", "Message box"),
    ("Corriger", "Fix"),
    ("Corriger une lettre de lecteur…", "Fix a drive letter…"),
    ("Description", "Description"),
    ("Enregistrer la sauvegarde", "Save the backup"),
    ("Exporter…", "Export…"),
    ("Exécuter en tant qu'administrateur", "Run as administrator"),
    ("Fenêtre", "Window"),
    ("Fenêtre complète", "Full window"),
    ("Fichiers remplacés : {}", "Files replaced: {}"),
    ("GitHub inaccessible : {}", "GitHub unreachable: {}"),
    ("Ignorer", "Ignore"),
    ("Installer la version {}", "Install version {}"),
    ("Lecteur", "Drive"),
    (
        "Les chemins absolus qui commencent par l'ancienne lettre seront modifiés.",
        "Absolute paths starting with the old letter will be changed.",
    ),
    ("Les deux", "Both"),
    (
        "Les fichiers actuels sont conservés en « .avant-restauration ».",
        "Current files are kept as “.avant-restauration”.",
    ),
    ("Lettre de lecteur", "Drive letter"),
    ("Mises à jour", "Updates"),
    ("Mode compact", "Compact mode"),
    (
        "Mode compact (recherche et liste)",
        "Compact mode (search and list)",
    ),
    ("Nettoyage", "Clean-up"),
    ("Notification Windows", "Windows notification"),
    ("Nouveautés", "What's new"),
    ("Nouvelle lettre", "New letter"),
    (
        "Nouvelle version : ouvrir les réglages",
        "New version: open settings",
    ),
    (
        "Nouvelle version disponible : {}",
        "New version available: {}",
    ),
    ("Options", "Options"),
    ("Outils", "Tools"),
    (
        "PStart.xml, notes et réglages dans un fichier zip",
        "PStart.xml, notes and settings in a zip file",
    ),
    ("Paramètres", "Parameters"),
    (
        "Pensez à brancher les lecteurs réseau ou amovibles avant.",
        "Connect network or removable drives first.",
    ),
    ("Rappels", "Reminders"),
    ("Restaurer", "Restore"),
    ("Restaurer une sauvegarde", "Restore a backup"),
    ("Restaurer…", "Restore…"),
    ("Restauré : {}", "Restored: {}"),
    ("Retiré des favoris", "Removed from favorites"),
    ("Sauvegarde", "Backup"),
    (
        "Sauvegarde créée ({} fichiers) : {}",
        "Backup created ({} files): {}",
    ),
    ("Statistiques", "Statistics"),
    ("Sur tous les écrans", "On every screen"),
    (
        "Téléchargement de la version {}…",
        "Downloading version {}…",
    ),
    ("Version {}", "Version {}"),
    ("Vérifier au démarrage", "Check at startup"),
    ("Vérifier maintenant", "Check now"),
    ("administrateur", "administrator"),
    ("agrandie", "maximized"),
    ("au démarrage", "at startup"),
    (
        "le fichier téléchargé n'est pas un exécutable valide",
        "the downloaded file is not a valid executable",
    ),
    ("normale", "normal"),
    ("plus d'un an", "over a year ago"),
    ("remplacement impossible : {}", "cannot replace: {}"),
    ("restauration : {}", "restore: {}"),
    ("réduite", "minimized"),
    (
        "réponse inattendue de GitHub",
        "unexpected answer from GitHub",
    ),
    ("sauvegarde : {}", "backup: {}"),
    ("téléchargement impossible : {}", "download failed: {}"),
    ("téléchargement…", "downloading…"),
    ("{} absent de l'archive", "{} missing from the archive"),
    (
        "{} absent de la version publiée",
        "{} missing from the published release",
    ),
    ("{} chemins concernés", "{} paths affected"),
    (
        "{} chemins corrigés ({}: → {}:)",
        "{} paths fixed ({}: → {}:)",
    ),
    ("{} raccourcis supprimés", "{} shortcuts deleted"),
    ("Épingler dans les favoris", "Pin to favorites"),
    ("à jour", "up to date"),
    ("☆ Ajouter aux favoris", "☆ Add to favorites"),
    // v0.7
    ("(aucune)", "(none)"),
    ("Ajouter la sélection", "Add selection"),
    ("Annuler (Ctrl+Z)", "Undo (Ctrl+Z)"),
    ("Aucun nouveau programme trouvé", "No new program found"),
    (
        "Aucun programme n'est ouvert depuis la clé.",
        "No program is running from the drive.",
    ),
    (
        "Ce groupe ne contient aucun raccourci",
        "This group has no shortcuts",
    ),
    (
        "Ces programmes ne se sont pas fermés (enregistrement en attente ?) :",
        "These programs did not close (waiting to save?):",
    ),
    ("Chemins", "Paths"),
    ("Clé USB", "USB drive"),
    (
        "Ctrl+Z / Ctrl+Y : annuler / rétablir",
        "Ctrl+Z / Ctrl+Y: undo / redo",
    ),
    ("Dossier : {}", "Folder: {}"),
    (
        "Dossier où chercher des programmes",
        "Folder to search for programs",
    ),
    ("Détails dans {}", "Details in {}"),
    (
        "Fermer les programmes et éjecter",
        "Close programs and eject",
    ),
    ("Fermeture des programmes…", "Closing programs…"),
    ("Forcer la fermeture", "Force close"),
    ("Groupe :", "Group:"),
    ("Groupe pour les autres :", "Group for the others:"),
    ("Ignorer la sélection", "Ignore selection"),
    ("Journal", "Log"),
    (
        "Lance ce programme depuis n'importe où, même fenêtre masquée",
        "Launches this program from anywhere, even with the window hidden",
    ),
    ("Lancer le groupe", "Launch group"),
    (
        "Lancer les {} programmes du groupe « {} » ?",
        "Launch the {} programs of group “{}”?",
    ),
    (
        "Le lecteur de PStart-rs ne peut pas être éjecté",
        "PStart-rs's drive cannot be ejected",
    ),
    (
        "Les chemins absolus sur le lecteur de PStart.xml deviennent relatifs",
        "Absolute paths on PStart.xml's drive become relative",
    ),
    (
        "Les modifications non enregistrées seront perdues",
        "Unsaved changes will be lost",
    ),
    ("Modification annulée", "Change undone"),
    ("Modification rétablie", "Change redone"),
    (
        "Ne plus signaler ces programmes",
        "Stop reporting these programs",
    ),
    ("Nouveau lien web ici…", "New web link here…"),
    ("Nouveau lien web…", "New web link…"),
    ("Nouveaux programmes", "New programs"),
    (
        "Oublier les {} programmes ignorés",
        "Forget the {} ignored programs",
    ),
    ("Ouvrir le journal", "Open log"),
    (
        "PStart-rs a rencontré une erreur et va se fermer.",
        "PStart-rs ran into an error and will close.",
    ),
    (
        "PStart-rs se fermera, puis Windows éjectera la clé.",
        "PStart-rs will close, then Windows will eject the drive.",
    ),
    (
        "PStart.xml modifié par un autre programme : rechargé",
        "PStart.xml changed by another program: reloaded",
    ),
    ("Plus tard", "Later"),
    (
        "PortableApps.com, exécutables et raccourcis .lnk d'un dossier",
        "PortableApps.com, executables and .lnk shortcuts in a folder",
    ),
    ("Programmes", "Programs"),
    (
        "Programmes ouverts depuis la clé :",
        "Programs running from the drive:",
    ),
    ("Raccourci clavier", "Keyboard shortcut"),
    ("Recherche de programmes…", "Searching for programs…"),
    ("Rechercher des programmes…", "Search for programs…"),
    ("Rendre les chemins portables", "Make paths portable"),
    ("Rien à annuler", "Nothing to undo"),
    ("Rétablir (Ctrl+Y)", "Redo (Ctrl+Y)"),
    (
        "Signaler les nouveaux programmes au démarrage",
        "Report new programs at startup",
    ),
    ("Supprimer le séparateur", "Delete separator"),
    ("Séparateur", "Separator"),
    ("Séparateur ici", "Separator here"),
    ("Tout cocher", "Check all"),
    ("Tout décocher", "Uncheck all"),
    (
        "Un groupe par catégorie (ou dossier)",
        "One group per category (or folder)",
    ),
    (
        "Variables : %PSTART% (dossier de PStart.xml), %DRIVE% (son lecteur), %USERPROFILE%…",
        "Variables: %PSTART% (PStart.xml folder), %DRIVE% (its drive), %USERPROFILE%…",
    ),
    (
        "empreinte SHA-256 incorrecte : mise à jour refusée",
        "wrong SHA-256 checksum: update refused",
    ),
    (
        "empreinte publiée illisible",
        "published checksum unreadable",
    ),
    ("ex. Ctrl+Alt+V", "e.g. Ctrl+Alt+V"),
    (
        "pas d'empreinte publiée pour cette version : mise à jour manuelle",
        "no checksum published for this version: update manually",
    ),
    (
        "raccourci « {} » utilisé deux fois",
        "shortcut “{}” used twice",
    ),
    ("{} chemins rendus relatifs", "{} paths made relative"),
    ("{} nouveaux programmes détectés", "{} new programs found"),
    ("{} programmes lancés ({})", "{} programs launched ({})"),
    ("{} programmes trouvés", "{} programs found"),
    ("Éjecter", "Eject"),
    ("Éjecter la clé ({}:)", "Eject drive ({}:)"),
    ("Éjecter la clé ({}:)…", "Eject drive ({}:)…"),
    ("Éjecter la clé…", "Eject drive…"),
    ("▶ Tout lancer", "▶ Launch all"),
    ("＋ {} nouveaux programmes", "＋ {} new programs"),
    // Erreurs
    ("raccourci « {} » invalide : {}", "invalid hotkey “{}”: {}"),
    (
        "raccourci « {} » indisponible : {}",
        "hotkey “{}” unavailable: {}",
    ),
    ("mémoire insuffisante", "out of memory"),
    ("fichier introuvable", "file not found"),
    ("chemin introuvable", "path not found"),
    (
        "accès refusé (ou élévation annulée)",
        "access denied (or elevation cancelled)",
    ),
    ("exécutable invalide", "invalid executable"),
    ("violation de partage", "sharing violation"),
    (
        "association de fichier incomplète",
        "incomplete file association",
    ),
    (
        "aucune application associée à ce type de fichier",
        "no application is associated with this file type",
    ),
    ("DLL introuvable", "DLL not found"),
    ("échec du lancement (code {})", "launch failed (code {})"),
    (
        "registre inaccessible (code {})",
        "registry unavailable (code {})",
    ),
    (
        "écriture du registre impossible (code {})",
        "cannot write to the registry (code {})",
    ),
    (
        "disponible sous Windows uniquement",
        "only available on Windows",
    ),
    ("élément introuvable", "item not found"),
    (
        "impossible de déplacer un groupe dans lui-même",
        "cannot move a group into itself",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_placeholders() {
        assert_eq!(fill("{} sur {}", &[&1, &"deux"]), "1 sur deux");
        assert_eq!(fill("{1} puis {0}", &[&"a", &"b"]), "b puis a");
        assert_eq!(fill("{x} {}", &[&3]), "{x} 3");
        assert_eq!(fill("{} {}", &[&3]), "3 ");
    }

    #[test]
    fn languages() {
        for l in Language::ALL {
            assert_eq!(Language::from_key(l.key()), l);
        }
        assert_eq!(Language::from_key("EN"), Language::English);
        assert_eq!(Language::from_key("?"), Language::Auto);
        assert_ne!(Language::Auto.resolve(), Language::Auto);
    }

    #[test]
    fn no_duplicate_keys() {
        let mut seen = std::collections::HashSet::new();
        for (fr, _) in EN {
            assert!(seen.insert(fr), "clé en double : {fr}");
        }
    }

    #[test]
    fn placeholders_match() {
        let count = |s: &str| s.matches('{').count();
        for (fr, en) in EN {
            assert_eq!(count(fr), count(en), "paramètres différents : {fr}");
        }
    }

    /// Chaque chaîne passée à `tr(` ou `trf(` dans les sources doit être
    /// traduite.
    #[test]
    fn every_string_is_translated() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut missing = Vec::new();
        let mut found = 0;
        let mut files = Vec::new();
        let mut dirs = vec![std::path::PathBuf::from(dir)];
        while let Some(d) = dirs.pop() {
            for entry in std::fs::read_dir(d).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    files.push(path);
                }
            }
        }
        for path in files {
            let src = std::fs::read_to_string(&path).unwrap();
            for lit in literals(&src) {
                found += 1;
                if !table().contains_key(lit.as_str()) {
                    missing.push(format!("{}: {lit}", path.display()));
                }
            }
        }
        assert!(found > 100, "seulement {found} chaînes trouvées");
        assert!(
            missing.is_empty(),
            "traductions manquantes :\n{}",
            missing.join("\n")
        );
    }

    fn literals(src: &str) -> Vec<String> {
        let mut out = Vec::new();
        for name in ["tr", "trf"] {
            let needle = format!("{name}(");
            let mut from = 0;
            while let Some(pos) = src[from..].find(&needle) {
                let start = from + pos;
                from = start + needle.len();
                let before = src[..start].chars().next_back();
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == ':') {
                    // `attr(`, `fn tr(`… mais `i18n::tr(` reste accepté.
                    if !src[..start].ends_with("i18n::") {
                        continue;
                    }
                }
                if src[..start].ends_with("fn ") {
                    continue;
                }
                let rest = src[from..].trim_start();
                let Some(body) = rest.strip_prefix('"') else {
                    continue;
                };
                let mut lit = String::new();
                let mut chars = body.chars();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('n') => lit.push('\n'),
                            Some(o) => lit.push(o),
                            None => {}
                        },
                        c => lit.push(c),
                    }
                }
                out.push(lit);
            }
        }
        out
    }
}
