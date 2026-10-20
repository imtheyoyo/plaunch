//! Surveillance des programmes ouverts : alimente l'indicateur « en cours
//! d'exécution » et le temps d'utilisation.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::config::{self, Config};
use crate::eject;
use crate::state::Shared;
use crate::usage;

const TICK: Duration = Duration::from_secs(3);
const SAVE_EVERY: Duration = Duration::from_secs(60);

/// Exécutables ouverts : clé de chemin → processus.
pub fn running_map(procs: &[eject::Proc]) -> HashMap<String, Vec<u32>> {
    let mut map: HashMap<String, Vec<u32>> = HashMap::new();
    for p in procs {
        map.entry(eject::path_key(&p.path)).or_default().push(p.pid);
    }
    map
}

/// Ajoute `secs` au temps des raccourcis dont le programme est ouvert.
/// Renvoie le nombre de raccourcis concernés.
pub fn account(
    cfg: &Config,
    running: &HashMap<String, Vec<u32>>,
    usage: &mut HashMap<String, u64>,
    secs: u64,
) -> usize {
    let mut n = 0;
    for e in cfg.flatten() {
        if !running.contains_key(&eject::path_key(&e.target)) {
            continue;
        }
        let Some(el) = cfg.item(&e.path) else {
            continue;
        };
        *usage.entry(usage::key(config::entry_path(el))).or_insert(0) += secs;
        n += 1;
    }
    n
}

/// Enregistre le temps d'utilisation s'il a changé.
pub fn save_usage(shared: &Shared) {
    if !shared.usage_dirty.swap(false, Ordering::SeqCst) {
        return;
    }
    let file = shared
        .config
        .lock()
        .unwrap()
        .as_ref()
        .map(|c| usage::path_for(&c.file));
    if let Some(file) = file {
        let map = shared.usage.lock().unwrap().clone();
        if let Err(e) = usage::save(&file, &map) {
            crate::log::error(&e);
        }
    }
}

pub fn spawn(shared: Arc<Shared>) {
    std::thread::spawn(move || {
        let mut last = Instant::now();
        let mut last_save = Instant::now();
        loop {
            std::thread::sleep(TICK);
            if shared.quitting.load(Ordering::SeqCst) {
                save_usage(&shared);
                return;
            }
            // Menu de notification : suit le thème de Windows en mode « Système ».
            crate::menutheme::follow_system();
            let running = running_map(&eject::all_processes());
            let elapsed = last.elapsed().as_secs().min(30);
            last = Instant::now();
            let changed = {
                let mut cur = shared.running.lock().unwrap();
                let changed =
                    cur.len() != running.len() || cur.keys().any(|k| !running.contains_key(k));
                *cur = running.clone();
                changed
            };
            if shared.track_usage.load(Ordering::SeqCst) {
                let guard = shared.config.lock().unwrap();
                if let Some(cfg) = guard.as_ref() {
                    let mut usage = shared.usage.lock().unwrap();
                    if account(cfg, &running, &mut usage, elapsed) > 0 {
                        shared.usage_dirty.store(true, Ordering::SeqCst);
                    }
                }
            }
            if last_save.elapsed() >= SAVE_EVERY {
                last_save = Instant::now();
                save_usage(&shared);
            }
            if changed {
                shared.repaint();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accounting() {
        let cfg = Config::from_str(
            include_str!("../tests/fixtures/PLaunch.sample.xml"),
            "E:\\Portable",
        )
        .unwrap();
        let procs = vec![
            eject::Proc {
                pid: 7,
                path: "e:\\portable\\Video\\VLC\\vlc.exe".into(),
            },
            eject::Proc {
                pid: 8,
                path: "C:\\Windows\\explorer.exe".into(),
            },
        ];
        let running = running_map(&procs);
        let mut usage = HashMap::new();
        assert_eq!(account(&cfg, &running, &mut usage, 3), 1);
        assert_eq!(account(&cfg, &running, &mut usage, 3), 1);
        assert_eq!(usage[&usage::key("Video\\VLC\\vlc.exe")], 6);
    }
}
