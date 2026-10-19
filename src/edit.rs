//! Modifications de l'arborescence : ajout, renommage, modification,
//! suppression et déplacement de raccourcis et de groupes.
//!
//! Les positions sont des [`ItemPath`] (indices depuis `<start><files>`).
//! Les éléments créés suivent l'ordre de balises de PStart.

use crate::config::{self, Config, EntryOptions, ItemPath};
use crate::i18n::tr;
use crate::xml::Element;

/// Élément à créer.
pub enum NewItem {
    Entry {
        name: String,
        path: String,
        directory: Option<String>,
        options: EntryOptions,
    },
    Group {
        name: String,
    },
    /// Trait de séparation (balise `<separator>` propre à PStart-rs).
    Separator,
}

impl NewItem {
    fn into_element(self) -> Element {
        match self {
            NewItem::Entry {
                name,
                path,
                directory,
                options,
            } => {
                let mut el = Element::new("file");
                el.container = true;
                el.attrs.push(("name".into(), name));
                el.children.push(Element::leaf("path", &path));
                if let Some(d) = directory.filter(|d| !d.trim().is_empty()) {
                    el.children.push(Element::leaf("directory", &d));
                }
                config::set_entry_options(&mut el, &options);
                el
            }
            NewItem::Group { name } => {
                let mut el = Element::new("files");
                el.container = true;
                el.attrs.push(("name".into(), name));
                el
            }
            NewItem::Separator => Element::new("separator"),
        }
    }
}

fn files_mut(cfg: &mut Config) -> &mut Element {
    if cfg.root.child("files").is_none() {
        let mut files = Element::new("files");
        files.container = true;
        cfg.root.children.insert(0, files);
    }
    cfg.root.child_mut("files").expect("<files> présent")
}

fn node_mut<'a>(root: &'a mut Element, path: &[usize]) -> Option<&'a mut Element> {
    let mut cur = root;
    for &i in path {
        cur = cur.children.get_mut(i)?;
    }
    Some(cur)
}

fn group_mut<'a>(cfg: &'a mut Config, path: &[usize]) -> Result<&'a mut Element, String> {
    let el = node_mut(files_mut(cfg), path).ok_or("groupe introuvable")?;
    if el.name != "files" {
        return Err("la destination n'est pas un groupe".into());
    }
    Ok(el)
}

/// Où insérer un nouvel élément selon la sélection : après un raccourci
/// sélectionné, à la fin d'un groupe sélectionné, sinon à la fin de la racine.
pub fn insertion_point(cfg: &Config, selected: Option<&[usize]>) -> (ItemPath, Option<usize>) {
    let Some(sel) = selected else {
        return (Vec::new(), None);
    };
    match cfg.item(sel) {
        Some(el) if el.name == "files" => (sel.to_vec(), None),
        Some(_) => {
            let (last, parent) = sel.split_last().expect("chemin non vide");
            (parent.to_vec(), Some(last + 1))
        }
        None => (Vec::new(), None),
    }
}

/// Insère un élément dans le groupe `parent`, à `index` (fin si `None`).
pub fn insert(
    cfg: &mut Config,
    parent: &[usize],
    index: Option<usize>,
    item: NewItem,
) -> Result<ItemPath, String> {
    let group = group_mut(cfg, parent)?;
    let idx = index
        .unwrap_or(group.children.len())
        .min(group.children.len());
    group.children.insert(idx, item.into_element());
    group.container = true;
    let mut path = parent.to_vec();
    path.push(idx);
    Ok(path)
}

/// Renomme un raccourci ou un groupe.
pub fn rename(cfg: &mut Config, path: &[usize], name: &str) -> Result<(), String> {
    let el = node_mut(files_mut(cfg), path).ok_or(tr("élément introuvable"))?;
    set_name(el, name);
    Ok(())
}

fn set_name(el: &mut Element, name: &str) {
    match el.attrs.iter_mut().find(|(k, _)| k == "name") {
        Some((_, v)) => *v = name.to_string(),
        None => el.attrs.insert(0, ("name".into(), name.to_string())),
    }
}

/// Modifie le nom, le chemin, le dossier de travail et les options d'un
/// raccourci. Les compteurs et les balises inconnues sont conservés.
pub fn update_entry(
    cfg: &mut Config,
    path: &[usize],
    name: &str,
    target: &str,
    directory: Option<&str>,
    options: &EntryOptions,
) -> Result<(), String> {
    let el = node_mut(files_mut(cfg), path).ok_or("raccourci introuvable")?;
    if el.name != "file" {
        return Err("ce n'est pas un raccourci".into());
    }
    set_name(el, name);
    match el.child_mut("path") {
        Some(p) => p.text = target.to_string(),
        None => el.children.insert(0, Element::leaf("path", target)),
    }
    let directory = directory.map(str::trim).filter(|d| !d.is_empty());
    match (
        directory,
        el.children.iter().position(|c| c.name == "directory"),
    ) {
        (Some(d), Some(i)) => el.children[i].text = d.to_string(),
        (Some(d), None) => {
            let after = el
                .children
                .iter()
                .position(|c| c.name == "path")
                .map_or(0, |i| i + 1);
            el.children.insert(after, Element::leaf("directory", d));
        }
        (None, Some(i)) => {
            el.children.remove(i);
        }
        (None, None) => {}
    }
    config::set_entry_options(el, options);
    Ok(())
}

/// Icône d'un groupe (attribut `icon`, propre à PStart-rs).
pub fn set_group_icon(cfg: &mut Config, path: &[usize], icon: Option<&str>) -> Result<(), String> {
    let el = node_mut(files_mut(cfg), path).ok_or(tr("élément introuvable"))?;
    el.attrs.retain(|(k, _)| k != "icon");
    if let Some(i) = icon.map(str::trim).filter(|i| !i.is_empty()) {
        el.attrs.push(("icon".into(), i.to_string()));
    }
    Ok(())
}

/// Raccourcis directement contenus dans un groupe (lancement groupé).
pub fn group_entries(cfg: &Config, path: &[usize]) -> Vec<ItemPath> {
    let Some(el) = cfg.item(path) else {
        return Vec::new();
    };
    el.children
        .iter()
        .enumerate()
        .filter(|(_, c)| c.name == "file")
        .map(|(i, _)| {
            let mut p = path.to_vec();
            p.push(i);
            p
        })
        .collect()
}

/// Trouve un groupe de premier niveau par son nom, ou le crée à la fin.
pub fn find_or_create_group(cfg: &mut Config, name: &str) -> Result<ItemPath, String> {
    let files = files_mut(cfg);
    if let Some(i) = files
        .children
        .iter()
        .position(|c| c.name == "files" && c.attr("name").map(str::trim) == Some(name))
    {
        return Ok(vec![i]);
    }
    insert(cfg, &[], None, NewItem::Group { name: name.into() })
}

/// Duplique un raccourci juste après lui (sans statistiques, favori ni
/// raccourci clavier). Renvoie la position de la copie.
pub fn duplicate(cfg: &mut Config, path: &[usize]) -> Result<ItemPath, String> {
    let (&idx, parent) = path.split_last().ok_or("chemin vide")?;
    let group = node_mut(files_mut(cfg), parent).ok_or(tr("élément introuvable"))?;
    let original = group.children.get(idx).ok_or(tr("élément introuvable"))?;
    if original.name != "file" {
        return Err(tr("seuls les raccourcis peuvent être dupliqués").into());
    }
    let mut copy = original.clone();
    let name = crate::i18n::trf("{} (copie)", &[&crate::config::display_name(original)]);
    set_name(&mut copy, &name);
    copy.children.retain(|c| {
        !matches!(
            c.name.as_str(),
            "runcount" | "rundate" | "favorite" | "globalhotkey"
        )
    });
    group.children.insert(idx + 1, copy);
    let mut p = parent.to_vec();
    p.push(idx + 1);
    Ok(p)
}

/// Déplace plusieurs éléments à la fin du groupe `target`. Les éléments
/// déjà dans ce groupe, ou qui le contiennent, sont ignorés.
pub fn move_many(
    cfg: &mut Config,
    mut paths: Vec<ItemPath>,
    target: &[usize],
) -> Result<usize, String> {
    // Insérés tous à l'ancienne fin du groupe, du dernier au premier :
    // l'ordre d'origine est conservé.
    let end = group_mut(cfg, target)?.children.len();
    let mut target = target.to_vec();
    // Du dernier au premier : les positions restantes ne bougent pas.
    paths.sort();
    paths.dedup();
    let mut n = 0;
    for p in paths.iter().rev() {
        let Some((&idx, parent)) = p.split_last() else {
            continue;
        };
        if parent == target.as_slice() || target.starts_with(p) {
            continue;
        }
        move_item(cfg, p, &target, end)?;
        // Le retrait décale le groupe cible s'il suivait l'élément.
        if target.len() > parent.len() && target.starts_with(parent) && target[parent.len()] > idx {
            target[parent.len()] -= 1;
        }
        n += 1;
    }
    Ok(n)
}

/// Épingle ou désépingle plusieurs raccourcis.
pub fn set_favorites(cfg: &mut Config, paths: &[ItemPath], on: bool) -> Result<usize, String> {
    let mut n = 0;
    for p in paths {
        if cfg.item(p).is_some_and(|e| e.name == "file") {
            set_favorite(cfg, p, on)?;
            n += 1;
        }
    }
    Ok(n)
}

/// Fixe l'ordre de la barre des favoris (numéros 1, 2, 3…).
pub fn reorder_favorites(cfg: &mut Config, order: &[ItemPath]) -> Result<(), String> {
    for (i, p) in order.iter().enumerate() {
        let el = node_mut(files_mut(cfg), p).ok_or(tr("élément introuvable"))?;
        let mut o = EntryOptions::of(el);
        o.favorite = true;
        o.favorite_rank = i as u32 + 1;
        config::set_entry_options(el, &o);
    }
    Ok(())
}

/// Épingle ou désépingle un raccourci.
pub fn set_favorite(cfg: &mut Config, path: &[usize], on: bool) -> Result<(), String> {
    let el = node_mut(files_mut(cfg), path).ok_or(tr("élément introuvable"))?;
    let mut o = EntryOptions::of(el);
    o.favorite = on;
    config::set_entry_options(el, &o);
    Ok(())
}

/// Remplace la lettre de lecteur `old` par `new` dans les chemins absolus
/// (`<path>` et `<directory>`). Renvoie le nombre de balises modifiées.
pub fn retarget_drive(cfg: &mut Config, old: char, new: char) -> usize {
    let mut n = 0;
    visit_entries(files_mut(cfg), &mut |el| {
        for c in el.children.iter_mut() {
            if c.name == "path" || c.name == "directory" {
                if let Some(t) = crate::paths::replace_drive(&c.text, old, new) {
                    c.text = t;
                    n += 1;
                }
            }
        }
    });
    n
}

/// Rend relatifs les chemins absolus situés sur le lecteur de PStart.xml
/// (portables d'une lettre à l'autre). Renvoie le nombre de balises changées.
pub fn make_portable(cfg: &mut Config) -> usize {
    let base = cfg.base_dir.clone();
    let Some(letter) = crate::paths::drive_letter(&base) else {
        return 0;
    };
    let mut n = 0;
    visit_entries(files_mut(cfg), &mut |el| {
        for c in el.children.iter_mut() {
            if (c.name == "path" || c.name == "directory")
                && crate::paths::drive_letter(c.text.trim()) == Some(letter)
                && !crate::paths::is_url(&c.text)
            {
                let rel = crate::paths::make_relative(c.text.trim(), &base);
                if rel != c.text {
                    c.text = rel;
                    n += 1;
                }
            }
        }
    });
    n
}

/// Nombre de chemins absolus qui désignent le lecteur `letter`.
pub fn count_drive_refs(cfg: &Config, letter: char) -> usize {
    let mut n = 0;
    if let Some(files) = cfg.files() {
        count_refs(files, letter, &mut n);
    }
    n
}

fn count_refs(el: &Element, letter: char, n: &mut usize) {
    for c in &el.children {
        match c.name.as_str() {
            "files" | "file" => count_refs(c, letter, n),
            "path" | "directory"
                if crate::paths::replace_drive(&c.text, letter, letter).is_some() =>
            {
                *n += 1;
            }
            _ => {}
        }
    }
}

fn visit_entries(el: &mut Element, f: &mut impl FnMut(&mut Element)) {
    for c in el.children.iter_mut() {
        match c.name.as_str() {
            "files" => visit_entries(c, f),
            "file" => f(c),
            _ => {}
        }
    }
}

/// Raccourcis dont la cible est introuvable, à supprimer : chemins triés de
/// façon à pouvoir les retirer un par un sans décaler les suivants.
pub fn delete_many(cfg: &mut Config, mut paths: Vec<ItemPath>) -> Result<usize, String> {
    // Ordre inverse : les indices restants restent valides.
    paths.sort();
    paths.dedup();
    let mut n = 0;
    for p in paths.iter().rev() {
        delete(cfg, p)?;
        n += 1;
    }
    Ok(n)
}

/// Supprime un raccourci ou un groupe (avec son contenu).
pub fn delete(cfg: &mut Config, path: &[usize]) -> Result<Element, String> {
    let (&idx, parent) = path.split_last().ok_or("chemin vide")?;
    let group = node_mut(files_mut(cfg), parent).ok_or(tr("élément introuvable"))?;
    if idx >= group.children.len() {
        return Err(tr("élément introuvable").into());
    }
    Ok(group.children.remove(idx))
}

/// Déplace l'élément `from` dans le groupe `to_parent`, à la position
/// `to_index` exprimée *avant* le retrait (comme le voit l'utilisateur).
/// Renvoie la nouvelle position de l'élément.
pub fn move_item(
    cfg: &mut Config,
    from: &[usize],
    to_parent: &[usize],
    to_index: usize,
) -> Result<ItemPath, String> {
    let (&fi, fp) = from.split_last().ok_or("chemin vide")?;
    if to_parent.starts_with(from) {
        return Err(tr("impossible de déplacer un groupe dans lui-même").into());
    }
    group_mut(cfg, to_parent)?;

    let mut to_parent = to_parent.to_vec();
    let mut to_index = to_index;
    // Le retrait décale les frères suivants d'un cran.
    if to_parent.len() > fp.len() && to_parent.starts_with(fp) && to_parent[fp.len()] > fi {
        to_parent[fp.len()] -= 1;
    }
    if to_parent == fp && to_index > fi {
        to_index -= 1;
    }
    if to_parent == fp && to_index == fi {
        return Ok(from.to_vec()); // pas de changement
    }

    let el = delete(cfg, from)?;
    let group = group_mut(cfg, &to_parent)?;
    let idx = to_index.min(group.children.len());
    group.children.insert(idx, el);
    to_parent.push(idx);
    Ok(to_parent)
}

/// Nombre de raccourcis contenus dans un élément (lui-même compris).
pub fn count_entries(el: &Element) -> usize {
    match el.name.as_str() {
        "file" => 1,
        "files" => el.children.iter().map(count_entries).sum(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::display_name;

    const SAMPLE: &str = include_str!("../tests/fixtures/PStart.sample.xml");

    fn cfg() -> Config {
        Config::from_str(SAMPLE, "E:\\Portable").unwrap()
    }

    fn names(cfg: &Config, path: &[usize]) -> Vec<String> {
        let el = if path.is_empty() {
            cfg.files().unwrap()
        } else {
            cfg.item(path).unwrap()
        };
        el.children
            .iter()
            .map(|c| display_name(c).to_string())
            .collect()
    }

    // Racine du fixture : [0] Vidéo, [1] Musique, [2] Outils, [3] Vide, [4] Notepad2
    // Vidéo : [0] VLC, [1] MPC-HC 64, [2] Mes vidéos

    #[test]
    fn insert_entry_and_group() {
        let mut c = cfg();
        let p = insert(
            &mut c,
            &[3],
            None,
            NewItem::Entry {
                name: "Nouveau".into(),
                path: "Outils\\n.exe".into(),
                directory: Some(String::new()),
                options: EntryOptions::default(),
            },
        )
        .unwrap();
        assert_eq!(p, vec![3, 0]);
        let el = c.item(&p).unwrap();
        assert_eq!(el.child_text("path"), Some("Outils\\n.exe"));
        assert!(el.child("directory").is_none());

        let g = insert(
            &mut c,
            &[],
            Some(1),
            NewItem::Group {
                name: "Jeux".into(),
            },
        )
        .unwrap();
        assert_eq!(g, vec![1]);
        assert_eq!(names(&c, &[])[1], "Jeux");
        // Le groupe vide s'écrit comme PStart.
        assert!(c
            .to_xml()
            .contains("\t\t<files name=\"Jeux\">\r\n\t\t</files>\r\n"));
    }

    #[test]
    fn insert_into_entry_is_refused() {
        let mut c = cfg();
        let r = insert(&mut c, &[4], None, NewItem::Group { name: "x".into() });
        assert!(r.is_err());
    }

    #[test]
    fn update_keeps_counters() {
        let mut c = cfg();
        update_entry(
            &mut c,
            &[0, 0],
            "VLC",
            "Video\\VLC3\\vlc.exe",
            Some("Video\\VLC3"),
            &EntryOptions::default(),
        )
        .unwrap();
        let el = c.item(&[0, 0]).unwrap();
        let order: Vec<&str> = el.children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(order, ["path", "directory", "runcount", "rundate"]);
        assert_eq!(el.child_text("runcount"), Some("62"));
        update_entry(
            &mut c,
            &[0, 0],
            "VLC",
            "Video\\VLC3\\vlc.exe",
            None,
            &EntryOptions::default(),
        )
        .unwrap();
        assert!(c.item(&[0, 0]).unwrap().child("directory").is_none());
    }

    #[test]
    fn entry_options_round_trip() {
        let mut c = cfg();
        let opts = EntryOptions {
            parameters: "--fullscreen \"x y\"".into(),
            description: "Lecteur vidéo".into(),
            window: config::WindowState::Maximized,
            autorun: true,
            admin: true,
            favorite: true,
            favorite_rank: 0,
            hotkey: "Ctrl+Alt+V".into(),
        };
        update_entry(
            &mut c,
            &[0, 0],
            "VLC",
            "Video\\VLC\\vlc.exe",
            Some("Video"),
            &opts,
        )
        .unwrap();
        let el = c.item(&[0, 0]).unwrap();
        let order: Vec<&str> = el.children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            order,
            [
                "path",
                "parameters",
                "directory",
                "description",
                "windowstate",
                "autorun",
                "runasadmin",
                "favorite",
                "globalhotkey",
                "runcount",
                "rundate"
            ]
        );
        assert_eq!(el.child_text("windowstate"), Some("wsMaximized"));
        assert_eq!(EntryOptions::of(el), opts);
        // Relecture après écriture.
        let back = Config::from_str(&c.to_xml(), "E:\\Portable").unwrap();
        assert_eq!(EntryOptions::of(back.item(&[0, 0]).unwrap()), opts);
        assert!(back.flatten()[0].favorite);
        // Les valeurs par défaut retirent les balises.
        set_favorite(&mut c, &[0, 0], false).unwrap();
        update_entry(
            &mut c,
            &[0, 0],
            "VLC",
            "x.exe",
            None,
            &EntryOptions::default(),
        )
        .unwrap();
        let el = c.item(&[0, 0]).unwrap();
        let order: Vec<&str> = el.children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(order, ["path", "runcount", "rundate"]);
    }

    #[test]
    fn drive_letters() {
        let mut c = cfg();
        // Fixture : PowerISO sous C:\ (chemin et dossier).
        assert_eq!(count_drive_refs(&c, 'C'), 2);
        assert_eq!(count_drive_refs(&c, 'F'), 0);
        assert_eq!(retarget_drive(&mut c, 'c', 'D'), 2);
        let el = c.item(&[2, 0]).unwrap();
        assert_eq!(
            el.child_text("path"),
            Some("D:\\Program Files (x86)\\PowerISO\\PowerISO.exe")
        );
        assert_eq!(count_drive_refs(&c, 'C'), 0);
    }

    #[test]
    fn portable_paths() {
        let mut c = Config::from_str(SAMPLE, "C:\\Program Files (x86)\\Portable").unwrap();
        // PowerISO est sur C: comme la base : chemin et dossier deviennent relatifs.
        assert_eq!(make_portable(&mut c), 2);
        assert_eq!(
            c.item(&[2, 0]).unwrap().child_text("path"),
            Some("..\\PowerISO\\PowerISO.exe")
        );
        assert_eq!(make_portable(&mut c), 0);
    }

    #[test]
    fn separators_groups_and_icons() {
        let mut c = cfg();
        let s = insert(&mut c, &[0], Some(1), NewItem::Separator).unwrap();
        assert_eq!(c.item(&s).unwrap().name, "separator");
        assert_eq!(count_entries(c.item(&[0]).unwrap()), 3);
        assert!(c.to_xml().contains("\t\t\t<separator></separator>\r\n"));
        assert_eq!(c.flatten().len(), 10);
        assert_eq!(
            group_entries(&c, &[0]),
            vec![vec![0, 0], vec![0, 2], vec![0, 3]]
        );
        set_group_icon(&mut c, &[1], Some("Musique\\note.ico")).unwrap();
        assert_eq!(
            c.item(&[1]).unwrap().attr("icon"),
            Some("Musique\\note.ico")
        );
        set_group_icon(&mut c, &[1], None).unwrap();
        assert_eq!(c.item(&[1]).unwrap().attr("icon"), None);
        assert_eq!(find_or_create_group(&mut c, "Outils").unwrap(), vec![2]);
        let g = find_or_create_group(&mut c, "Nouveaux").unwrap();
        assert_eq!(g, vec![5]);
    }

    #[test]
    fn favorites_order() {
        let mut c = cfg();
        set_favorite(&mut c, &[0, 0], true).unwrap(); // VLC
        set_favorite(&mut c, &[4], true).unwrap(); // Notepad2
        let names = |c: &Config| {
            c.favorites()
                .iter()
                .map(|e| e.name.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&c), ["VLC media player", "Notepad2"]);
        reorder_favorites(&mut c, &[vec![4], vec![0, 0]]).unwrap();
        assert_eq!(names(&c), ["Notepad2", "VLC media player"]);
        assert_eq!(c.item(&[4]).unwrap().child_text("favorite"), Some("1"));
        // Un nouveau favori sans numéro va à la fin.
        set_favorite(&mut c, &[1, 0], true).unwrap();
        assert_eq!(
            names(&c).last().map(String::as_str),
            Some("Audacity® Sound Editor")
        );
        assert_eq!(crate::config::favorite_value("True"), (true, 0));
        assert_eq!(crate::config::favorite_value("abc"), (false, 0));
    }

    #[test]
    fn duplicate_entry() {
        let mut c = cfg();
        set_favorite(&mut c, &[0, 0], true).unwrap();
        let p = duplicate(&mut c, &[0, 0]).unwrap();
        assert_eq!(p, vec![0, 1]);
        let el = c.item(&p).unwrap();
        assert_eq!(display_name(el), "VLC media player (copie)");
        assert_eq!(el.child_text("path"), Some("Video\\VLC\\vlc.exe"));
        assert!(el.child("runcount").is_none() && el.child("favorite").is_none());
        assert!(duplicate(&mut c, &[0]).is_err());
    }

    #[test]
    fn move_several() {
        let mut c = cfg();
        // VLC et Mes vidéos (Vidéo), Notepad2 (racine) → Vide [3].
        let n = move_many(&mut c, vec![vec![0, 0], vec![4], vec![0, 2]], &[3]).unwrap();
        assert_eq!(n, 3);
        assert_eq!(
            names(&c, &[3]),
            ["VLC media player", "Mes vidéos", "Notepad2"]
        );
        assert_eq!(names(&c, &[0]), ["MPC-HC 64"]);
        // Groupe déplacé vers un groupe placé après lui.
        let mut c = cfg();
        move_many(&mut c, vec![vec![1]], &[3]).unwrap();
        assert_eq!(names(&c, &[2]), ["Musique"]);
        // Un groupe ne peut pas aller dans lui-même : ignoré.
        assert_eq!(move_many(&mut c, vec![vec![2]], &[2]).unwrap(), 0);
    }

    #[test]
    fn delete_several() {
        let mut c = cfg();
        let n = delete_many(&mut c, vec![vec![0, 0], vec![0, 2], vec![4]]).unwrap();
        assert_eq!(n, 3);
        assert_eq!(names(&c, &[0]), ["MPC-HC 64"]);
        assert_eq!(names(&c, &[]).len(), 4);
    }

    #[test]
    fn rename_and_delete() {
        let mut c = cfg();
        rename(&mut c, &[1], "Audio").unwrap();
        assert_eq!(names(&c, &[])[1], "Audio");
        let removed = delete(&mut c, &[0]).unwrap();
        assert_eq!(count_entries(&removed), 3);
        assert_eq!(names(&c, &[])[0], "Audio");
    }

    #[test]
    fn move_within_same_group() {
        let mut c = cfg();
        // VLC tout en bas de Vidéo (déposé « après » Mes vidéos).
        let p = move_item(&mut c, &[0, 0], &[0], 3).unwrap();
        assert_eq!(p, vec![0, 2]);
        assert_eq!(
            names(&c, &[0]),
            ["MPC-HC 64", "Mes vidéos", "VLC media player"]
        );
        // Et retour en tête.
        let p = move_item(&mut c, &[0, 2], &[0], 0).unwrap();
        assert_eq!(p, vec![0, 0]);
        assert_eq!(names(&c, &[0])[0], "VLC media player");
    }

    #[test]
    fn move_across_groups_adjusts_indices() {
        let mut c = cfg();
        // Le groupe Vidéo (index 0) déplacé dans « Vide » (index 3) :
        // après retrait, « Vide » devient l'index 2.
        let p = move_item(&mut c, &[0], &[3], 0).unwrap();
        assert_eq!(p, vec![2, 0]);
        assert_eq!(names(&c, &[2]), ["Vidéo"]);
        assert_eq!(names(&c, &[2, 0]).len(), 3);
    }

    #[test]
    fn move_into_itself_is_refused() {
        let mut c = cfg();
        assert!(move_item(&mut c, &[1], &[1, 1], 0).is_err());
        assert!(move_item(&mut c, &[1], &[1], 0).is_err());
    }

    #[test]
    fn insertion_points() {
        let c = cfg();
        assert_eq!(insertion_point(&c, None), (vec![], None));
        assert_eq!(insertion_point(&c, Some(&[1])), (vec![1], None));
        assert_eq!(insertion_point(&c, Some(&[0, 1])), (vec![0], Some(2)));
    }
}
