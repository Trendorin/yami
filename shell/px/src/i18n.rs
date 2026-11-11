//! Язык интерфейса — общий для шелла и его программ. Надписи в коде русские; английский — по таблице
//! каждой программы, остальные языки — по её таблицам от английского (нет строки — остаётся английская,
//! так язык можно переводить по частям). Пусто — по LC_ALL, LC_MESSAGES, LANG (первое непустое);
//! незнакомый язык — английский.

use std::sync::atomic::{AtomicU8, Ordering};

/// языки: код для настроек и имя на самом языке; порядок — и порядок выбора в настройках
pub const LANGS: &[(&str, &str)] = &[
    ("en", "English"),
    ("ru", "Русский"),
    ("pt-BR", "Português (Brasil)"),
    ("es", "Español"),
    ("de", "Deutsch"),
    ("fr", "Français"),
    ("uk", "Українська"),
    ("pl", "Polski"),
];

const EN: u8 = 0;
const RU: u8 = 1;

/// номер в LANGS
static LANGUAGE: AtomicU8 = AtomicU8::new(RU);

/// значение для настроек: код из LANGS («pt» любой страны — «pt-BR»); пусто — язык системы
pub fn normalize(tag: &str) -> &'static str {
    let tag = tag.trim().split(['.', '@']).next().unwrap_or("").replace('_', "-").to_ascii_lowercase();
    match tag.split('-').next().unwrap_or("") {
        "pt" => "pt-BR",
        "" => "",
        b => LANGS.iter().find(|l| l.0 == b).map_or("", |l| l.0),
    }
}

fn index(code: &str) -> Option<u8> {
    LANGS.iter().position(|l| l.0 == code).map(|i| i as u8)
}

fn resolve(explicit: &str, locales: &[&str]) -> u8 {
    let source = if explicit.is_empty() { locales.iter().copied().find(|s| !s.is_empty()).unwrap_or("") } else { explicit };
    index(normalize(source)).unwrap_or(EN)
}

pub fn set(lang: &str) {
    let locales: Vec<String> = ["LC_ALL", "LC_MESSAGES", "LANG"].iter().map(|k| std::env::var(k).unwrap_or_default()).collect();
    let refs: Vec<&str> = locales.iter().map(String::as_str).collect();
    LANGUAGE.store(resolve(lang, &refs), Ordering::Relaxed);
}

/// код текущего языка («ru», «en», «pt-BR», «de»…)
pub fn code() -> &'static str {
    LANGS.get(LANGUAGE.load(Ordering::Relaxed) as usize).map_or("en", |l| l.0)
}

/// имя языка на нём самом; пусто — нет такого
pub fn name(code: &str) -> &'static str {
    LANGS.iter().find(|l| l.0 == code).map_or("", |l| l.1)
}

pub fn en() -> bool {
    LANGUAGE.load(Ordering::Relaxed) == EN
}

pub fn pt_br() -> bool {
    code() == "pt-BR"
}

/// не русский: надписи идут через таблицы
pub fn translated() -> bool {
    LANGUAGE.load(Ordering::Relaxed) != RU
}

/// кнопка языка в настройках по кругу: авто, потом LANGS по порядку
pub fn next(lang: &str) -> &'static str {
    match index(normalize(lang)) {
        None => LANGS[0].0,
        Some(i) => LANGS.get(i as usize + 1).map_or("", |l| l.0),
    }
}

/// кусок шаблона формата: текст или {поле} (как написано, со спецификацией)
enum Piece<'a> {
    Text(String),
    Field(&'a str),
}

fn pieces(t: &str) -> Vec<Piece<'_>> {
    let (mut out, mut text, mut i) = (vec![], String::new(), 0);
    let b = t.as_bytes();
    while i < b.len() {
        match b[i] {
            b'{' if b.get(i + 1) == Some(&b'{') => {
                text.push('{');
                i += 2;
            }
            b'}' if b.get(i + 1) == Some(&b'}') => {
                text.push('}');
                i += 2;
            }
            b'{' => {
                let Some(j) = t[i..].find('}') else { break };
                if !text.is_empty() {
                    out.push(Piece::Text(std::mem::take(&mut text)));
                }
                out.push(Piece::Field(&t[i..i + j + 1]));
                i += j + 1;
            }
            _ => {
                let c = t[i..].chars().next().unwrap_or(' ');
                text.push(c);
                i += c.len_utf8();
            }
        }
    }
    if !text.is_empty() {
        out.push(Piece::Text(text));
    }
    out
}

/// строка с подстановками на другом языке без форматирования заново: out — что вышло по английскому
/// шаблону en; значения полей вынимаются из out между кусками текста en и встают в шаблон to на места
/// тех же {полей} (одинаковые поля — по порядку). Два поля подряд или out не по шаблону — None
/// (тогда остаётся английская строка)
pub fn refill(en: &str, out: &str, to: &str) -> Option<String> {
    let src = pieces(en);
    let mut vals: Vec<(&str, &str)> = vec![];
    let mut pos = 0;
    for (i, p) in src.iter().enumerate() {
        match p {
            Piece::Text(s) => {
                if !out[pos..].starts_with(s.as_str()) {
                    return None;
                }
                pos += s.len();
            }
            Piece::Field(f) => {
                let end = match src.get(i + 1) {
                    None => out.len(),
                    Some(Piece::Text(s)) => pos + out[pos..].find(s.as_str())?,
                    Some(Piece::Field(_)) => return None,
                };
                vals.push((f, &out[pos..end]));
                pos = end;
            }
        }
    }
    if pos != out.len() {
        return None;
    }
    let mut taken: Vec<&str> = vec![];
    let mut s = String::with_capacity(to.len() + out.len());
    for p in pieces(to) {
        match p {
            Piece::Text(t) => s.push_str(&t),
            Piece::Field(f) => {
                let k = taken.iter().filter(|x| **x == f).count();
                s.push_str(vals.iter().filter(|v| v.0 == f).nth(k)?.1);
                taken.push(f);
            }
        }
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_aliases_and_settings_values() {
        for tag in ["pt-BR", "pt_BR", "pt_BR.UTF-8", "PT-br", "pt", "pt_PT.UTF-8"] {
            assert_eq!(normalize(tag), "pt-BR", "{tag}");
            assert_eq!(LANGS[resolve(tag, &["ru_RU.UTF-8"]) as usize].0, "pt-BR");
        }
        assert_eq!(normalize("ru_RU.UTF-8"), "ru");
        assert_eq!(normalize("en-US"), "en");
        assert_eq!(normalize("de_AT.UTF-8"), "de");
        assert_eq!(normalize("uk_UA.UTF-8"), "uk");
        assert_eq!(normalize("ja_JP.UTF-8"), "");
        assert_eq!(normalize("invalid\ncommand"), "");
    }

    #[test]
    fn system_language_precedence_and_empty_variables() {
        let r = |e: &str, l: &[&str]| LANGS[resolve(e, l) as usize].0;
        assert_eq!(r("", &["", "", "pt_BR.UTF-8"]), "pt-BR");
        assert_eq!(r("", &["", "ru_RU.UTF-8", "pt_BR.UTF-8"]), "ru");
        assert_eq!(r("", &["C.UTF-8", "pt_BR.UTF-8", "pt_BR.UTF-8"]), "en");
        assert_eq!(r("pt-BR", &["en_US.UTF-8"]), "pt-BR");
        assert_eq!(r("", &["de_DE.UTF-8"]), "de");
        assert_eq!(r("", &["ja_JP.UTF-8"]), "en");
    }

    #[test]
    fn settings_cycle_includes_all_languages() {
        let mut lang = "";
        let mut seen = vec![];
        loop {
            lang = next(lang);
            if lang.is_empty() {
                break;
            }
            seen.push(lang);
        }
        assert_eq!(seen, LANGS.iter().map(|l| l.0).collect::<Vec<_>>());
    }

    #[test]
    fn refill_moves_values_into_another_template() {
        assert_eq!(refill("up {} d {} h", "up 3 d 4 h", "{} d {} h aktiv").as_deref(), Some("3 d 4 h aktiv"));
        // поля со спецификацией и по имени; повтор одного поля — по порядку
        assert_eq!(refill("{:.1} GB of {:.1} GB", "1.5 GB of 8.0 GB", "{:.1} Go sur {:.1} Go").as_deref(), Some("1.5 Go sur 8.0 Go"));
        assert_eq!(refill("error: {e}", "error: no such file", "Fehler: {e}").as_deref(), Some("Fehler: no such file"));
        assert_eq!(refill("{{x}} {} left", "{x} 5 left", "zostało {} {{x}}").as_deref(), Some("zostało 5 {x}"));
        // не по шаблону, два поля подряд — нет
        assert_eq!(refill("{} of {}", "5 from 6", "{} de {}"), None);
        assert_eq!(refill("{}{}", "56", "{}{}"), None);
        assert_eq!(refill("{} files", "5 files", "{} {} Dateien"), None);
    }
}
