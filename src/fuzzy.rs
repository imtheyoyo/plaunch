//! Recherche tolérante : sous-chaîne, initiales (« np2 » → Notepad2,
//! « vmp » → VLC media player) et petites fautes de frappe (« notpad »).
//! Le score sert à classer les résultats ; `None` = pas de correspondance.

/// Minuscules sans accents, pour comparer « Vidéo » et « video ».
pub fn fold(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' | 'ì' => 'i',
            'ô' | 'ö' | 'ó' | 'ò' | 'õ' => 'o',
            'û' | 'ü' | 'ú' | 'ù' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            'ÿ' => 'y',
            c => c,
        })
        .collect()
}

/// Score d'un nom pour une requête déjà normalisée (`fold`).
pub fn score(query: &str, name: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let name = fold(name);
    if name == query {
        return Some(1000);
    }
    if name.starts_with(query) {
        return Some(900 - name.len() as i32);
    }
    let words = words(&name);
    if words.iter().any(|w| w.starts_with(query)) {
        return Some(800 - name.len() as i32);
    }
    if name.contains(query) {
        return Some(700 - name.len() as i32);
    }
    // Initiales des mots (et chiffres) : « vmp », « np2 ».
    let initials: String = words
        .iter()
        .filter_map(|w| w.chars().next())
        .collect::<String>();
    if initials.starts_with(query) && query.chars().count() >= 2 {
        return Some(600);
    }
    // Lettres dans l'ordre, au début des mots de préférence.
    if let Some(gaps) = subsequence(query, &name) {
        if query.chars().count() >= 2 {
            return Some(500 - gaps.min(200) as i32);
        }
    }
    // Faute de frappe : un mot du nom à 1 (ou 2 pour les longs mots) près.
    let n = query.chars().count();
    if n >= 4 {
        let allowed = if n >= 7 { 2 } else { 1 };
        let best = words
            .iter()
            .map(|w| {
                // Compare au début du mot, de longueur voisine.
                let prefix: String = w.chars().take(n + 1).collect();
                distance(query, &prefix).min(distance(query, w))
            })
            .min()?;
        if best <= allowed {
            return Some(300 - 50 * best as i32);
        }
    }
    None
}

/// Mots du nom : séparateurs, et passage lettres ↔ chiffres (« notepad2 »).
fn words(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut prev_digit = None;
    for c in name.chars() {
        if !c.is_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev_digit = None;
            continue;
        }
        let digit = c.is_ascii_digit();
        if prev_digit.is_some_and(|p| p != digit) && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        cur.push(c);
        prev_digit = Some(digit);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Les lettres de `query` apparaissent-elles dans l'ordre ? Renvoie le
/// nombre de caractères sautés.
fn subsequence(query: &str, name: &str) -> Option<usize> {
    let mut gaps = 0;
    let mut it = name.chars();
    for q in query.chars() {
        loop {
            let c = it.next()?;
            if c == q {
                break;
            }
            gaps += 1;
        }
    }
    Some(gaps)
}

/// Distance de Damerau-Levenshtein (transpositions comprises).
pub fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    let mut d = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[n][m]
}

/// Score final d'un raccourci : correspondance du nom (ou à défaut du
/// chemin) plus un bonus d'usage.
pub fn rank(query: &str, name: &str, target: &str, runcount: u32) -> Option<i32> {
    let s = score(query, name).or_else(|| {
        // Le chemin n'est cherché qu'en sous-chaîne, avec un score faible.
        fold(target).contains(query).then_some(200)
    })?;
    let usage = (((runcount as f32) + 1.0).ln() * 15.0) as i32;
    Some(s + usage.min(120))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches() {
        assert!(score("vlc", "VLC media player").unwrap() > score("vlc", "Outil VLC").unwrap());
        assert!(score("np2", "Notepad2").is_some());
        assert!(score("vmp", "VLC media player").is_some());
        assert!(score("notpad", "Notepad2").is_some()); // faute de frappe
        assert!(score("ntoepad", "Notepad2").is_some()); // lettres inversées
        assert!(score("video", "Mes vidéos").is_some()); // accents
        assert_eq!(score("xyz", "Notepad2"), None);
        assert_eq!(score("zz", "Audacity"), None);
        assert_eq!(fold("Éè Ç"), "ee c");
        assert_eq!(words("notepad2 x"), ["notepad", "2", "x"]);
        assert_eq!(distance("abc", "acb"), 1);
        assert_eq!(distance("kitten", "sitting"), 3);
    }

    #[test]
    fn usage_bonus() {
        let rare = rank("au", "Audacity", "x", 0).unwrap();
        let often = rank("au", "Audacity", "x", 200).unwrap();
        assert!(often > rare);
        assert_eq!(rank("ttt", "Audacity", "C:\\ttt\\a.exe", 0), Some(200));
    }
}
