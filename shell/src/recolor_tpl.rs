// Свои шаблоны перекраски: файлы в ~/.config/yami/templates. Строка «yami-output: ~/путь» — куда писать,
// «yami-reload: USR1 программа» — кому послать сигнал (USR1, USR2 или HUP); {{accent}}, {{bg}}, {{color4}}…
// — цвета (.strip — без «#», .rgb — «r, g, b»). Пишем только в ~/.config, ~/.local/share и ~/.cache и
// только в свой файл: изменённый руками не перезаписываем, пока не попросят (force: прежний файл
// остаётся как *.bak-yami). Шаблон с неизвестным {{именем}} не пишется: опечатка не должна ломать
// конфиг программы. Итог каждого — в restyle.json.

use crate::recolor::*;
use crate::recolor_apps::ansi;
use serde_json::{json, Map, Value};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

/// {{ имя[.strip|.rgb] }} с позиции i: (имя, вид, конец)
fn key_at(s: &str, i: usize) -> Option<(&str, Option<&str>, usize)> {
    let t = s[i..].strip_prefix("{{")?;
    let lead = t.len() - t.trim_start().len();
    let body = &t[lead..];
    if !body.chars().next()?.is_ascii_alphabetic() {
        return None;
    }
    let end = body.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(body.len());
    let (name, mut rest) = body.split_at(end);
    let mut kind = None;
    for k in ["strip", "rgb"] {
        if let Some(r) = rest.strip_prefix('.').and_then(|r| r.strip_prefix(k)) {
            kind = Some(k);
            rest = r;
            break;
        }
    }
    let tail = rest.trim_start().strip_prefix("}}")?;
    Some((name, kind, s.len() - tail.len()))
}

/// все {{ключи}} подряд: f получает имя и вид, отдаёт замену
fn keys_map(s: &str, mut f: impl FnMut(&str, Option<&str>) -> String) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if let Some((name, kind, end)) = key_at(s, i) {
            out += &f(name, kind);
            i = end;
        } else {
            let ch = s[i..].chars().next().map_or(1, char::len_utf8);
            out.push_str(&s[i..i + ch]);
            i += ch;
        }
    }
    out
}

/// «yami-output: X» или «yami-reload: X» в строке: (output?, X без пробелов по краям)
fn directive(line: &str) -> Option<(bool, &str)> {
    let mut from = 0;
    while let Some(at) = line[from..].find("yami-") {
        let at = from + at;
        let rest = &line[at + 5..];
        let found = rest.strip_prefix("output:").map(|r| (true, r)).or_else(|| rest.strip_prefix("reload:").map(|r| (false, r)));
        if let Some((out, r)) = found {
            let v = r.trim();
            if !v.is_empty() {
                return Some((out, v));
            }
        }
        from = at + 5;
    }
    None
}

fn expand_user(cx: &Cx, s: &str) -> PathBuf {
    if s == "~" {
        return cx.home.clone();
    }
    if let Some(r) = s.strip_prefix("~/") {
        return cx.home.join(r);
    }
    // ~мояучётка/…
    if let (Some(r), Ok(me)) = (s.strip_prefix('~'), std::env::var("USER")) {
        if let Some(t) = r.strip_prefix(me.as_str()).and_then(|t| t.strip_prefix('/')) {
            return cx.home.join(t);
        }
    }
    PathBuf::from(s)
}

/// SHA-256: отпечаток записанного файла (сверка «менял ли его человек»)
pub fn sha256(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64).wrapping_mul(8)).to_be_bytes());
    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, b) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for (a, b) in h.iter_mut().zip(v) {
            *a = a.wrapping_add(b);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

fn good_prog(s: &str) -> bool {
    (1..=32).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
}

pub fn templates(cx: &Cx, p: &P, force: Option<&str>) -> Res {
    let d = cx.cfg.join("yami/templates");
    if !d.is_dir() {
        return cx.status("templates", Some(json!([])));
    }
    let mut cols: HashMap<String, String> = p.0.iter().filter(|(_, v)| v.starts_with('#')).map(|(k, v)| (k.clone(), v.clone())).collect();
    cols.extend(ansi(p));
    let state = cx.state.join("templates.json");
    let mut sums: Map<String, Value> = Map::new();
    if let Some(o) = std::fs::read_to_string(&state).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| v.as_object().cloned()) {
        for (k, v) in o {
            if v.is_string() {
                // ключи — «~/…» (до 2.7 были полные пути: переезд дома ломал сверку)
                sums.insert(if k.starts_with('/') { cx.tilde(Path::new(&k)) } else { k }, v);
            }
        }
    }
    let roots: Vec<String> = [cx.cfg.clone(), cx.home.join(".local/share"), cx.home.join(".cache")].iter().map(|r| format!("{}/", real_path(r).display())).collect();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&d)?.flatten().map(|e| e.path()).collect();
    files.sort();
    let mut rows = vec![];
    for f in files {
        let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if !f.is_file() || name.starts_with('.') || f.extension().is_some_and(|e| e == "md") {
            continue;
        }
        let row = one(cx, &f, &name, &cols, &mut sums, force, &roots);
        rows.push(Value::Object(row));
    }
    write(&state, &(serde_json::to_string_pretty(&Value::Object(sums)).unwrap_or_default() + "\n"))?;
    cx.status("templates", Some(Value::Array(rows)))
}

fn one(cx: &Cx, f: &Path, name: &str, cols: &HashMap<String, String>, sums: &mut Map<String, Value>, force: Option<&str>, roots: &[String]) -> Map<String, Value> {
    let mut row = Map::new();
    row.insert("file".into(), name.into());
    let mark = |row: &mut Map<String, Value>, s: &str| {
        row.insert("state".into(), s.into());
    };
    let Ok(src) = std::fs::read_to_string(f) else {
        mark(&mut row, "unreadable");
        return row;
    };
    let (mut out, mut reload, mut body) = (None::<PathBuf>, None::<Vec<String>>, String::new());
    for line in lines_keep(&src) {
        match directive(line) {
            Some((true, v)) => out = Some(expand_user(cx, v)),
            Some((false, v)) => reload = Some(v.split_whitespace().map(String::from).collect()),
            None => body.push_str(line),
        }
    }
    let Some(out) = out else {
        mark(&mut row, "nooutput");
        return row;
    };
    row.insert("output".into(), cx.tilde(&out).into());
    let real = real_path(&out);
    let real_s = real.display().to_string();
    if !out.is_absolute() || !roots.iter().any(|r| real_s.starts_with(r.as_str())) {
        mark(&mut row, "outside");
        return row;
    }
    let mut unknown = BTreeSet::new();
    keys_map(&body, |n, _| {
        if !cols.contains_key(n) {
            unknown.insert(n.to_string());
        }
        String::new()
    });
    if !unknown.is_empty() {
        mark(&mut row, "unknown");
        row.insert("keys".into(), unknown.into_iter().map(Value::from).collect::<Vec<_>>().into());
        return row;
    }
    let text = keys_map(&body, |n, kind| {
        let v = cols.get(n).map_or("", String::as_str);
        match kind {
            Some("strip") => v.trim_start_matches('#').to_string(),
            Some(_) => {
                let (r, g, b) = rgb(v);
                format!("{r}, {g}, {b}")
            }
            None => v.to_string(),
        }
    });
    let (key, new) = (cx.tilde(&real), sha256(text.as_bytes()));
    if real.exists() {
        let Ok(have) = std::fs::read(&real).map(|b| sha256(&b)) else {
            mark(&mut row, "unreadable");
            return row;
        };
        if sums.get(&key).and_then(|v| v.as_str()) != Some(have.as_str()) && have != new {
            if force != Some(name) {
                mark(&mut row, "hand");
                return row;
            }
            let bak = real.with_file_name(format!("{}.bak-yami", real.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
            let _ = std::fs::copy(&real, bak);
        }
    }
    if write(&real, &text).is_err() {
        mark(&mut row, "unreadable");
        return row;
    }
    sums.insert(key, new.into());
    mark(&mut row, "ok");
    if let Some(r) = reload.filter(|r| r.len() == 2 && ["USR1", "USR2", "HUP"].contains(&r[0].as_str()) && good_prog(&r[1])) {
        signal(&r[0], &r[1]);
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_vectors() {
        assert_eq!(sha256(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn template_syntax() {
        assert_eq!(key_at("{{ accent.strip }}x", 0).map(|(a, b, c)| (a.to_string(), b.map(String::from), c)), Some(("accent".into(), Some("strip".into()), 18)));
        assert!(key_at("{{1a}}", 0).is_none());
        assert_eq!(keys_map("a{{b}}c {{ b.foo }}", |n, _| n.to_uppercase()), "aBc {{ b.foo }}");
        assert_eq!(directive("# yami-output: ~/.config/x  \n"), Some((true, "~/.config/x")));
        assert_eq!(directive("-- yami-reload:   USR1 foot"), Some((false, "USR1 foot")));
        assert_eq!(directive("yami-output:   "), None);
    }
}
