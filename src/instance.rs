//! Instance unique : un second lancement réveille la fenêtre existante.

/// Renvoie `true` si cette instance est la première. Le verrou est gardé
/// jusqu'à la fin du processus.
#[cfg(windows)]
pub fn acquire() -> bool {
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;

    let name: Vec<u16> = "Local\\PStartRsSingleInstance"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY : nom terminé par un zéro, attributs de sécurité par défaut.
    // Le handle n'est volontairement jamais fermé : Windows le libère à la
    // fin du processus.
    unsafe {
        let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        // En cas d'échec de création, on ne bloque pas le démarrage.
        handle.is_null() || GetLastError() != ERROR_ALREADY_EXISTS
    }
}

#[cfg(not(windows))]
pub fn acquire() -> bool {
    true
}
