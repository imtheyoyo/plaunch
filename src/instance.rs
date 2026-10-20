//! Instance unique : un second lancement réveille la fenêtre existante.

/// Renvoie `true` si cette instance est la première. Le verrou est gardé
/// jusqu'à la fin du processus.
#[cfg(windows)]
pub fn acquire() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;

    let name: Vec<u16> = "Local\\PLaunchSingleInstance"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY : nom terminé par un zéro, attributs de sécurité par défaut.
    // Le handle n'est volontairement jamais fermé : Windows le libère à la
    // fin du processus.
    unsafe {
        let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        // En cas d'échec de création, on ne bloque pas le démarrage.
        if handle.is_null() {
            return true;
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            // Ne pas garder le verrou de l'autre instance en vie (nouvel
            // essai après une mise à jour).
            CloseHandle(handle);
            return false;
        }
        true
    }
}

#[cfg(not(windows))]
pub fn acquire() -> bool {
    true
}
