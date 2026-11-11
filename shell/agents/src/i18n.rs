// Язык агентов — тот же механизм, что в шелле: надписи по-русски, английский по
// таблице при отрисовке. Язык берётся из style.json.
// Третий язык — бразильский португальский: таблица PT_BR (i18n_pt_br.rs) от английского, у tf! —
// третий шаблон. Какой язык — решает yami_px::i18n (общий выбор для всех программ).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

pub use yami_px::i18n::{code, pt_br, set, translated};

/// `shot`: собирать надписи, которые остались без перевода
static AUDIT: AtomicBool = AtomicBool::new(false);
static MISSED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// перевод целой надписи (английский по PAIRS, португальский по PT_BR от английского); нет — как есть
pub fn tr(s: &str) -> &str {
    if !translated() {
        return s;
    }
    let english = table().get(s).copied().unwrap_or(s);
    let out = select(s, english);
    // без перевода: русская надпись не из таблицы; у португальского — ещё и нет строки в его таблице
    // (остальные языки переводятся по частям: без строки — английская)
    let known = table().contains_key(s);
    // названия языков (Русский, Українська) не переводятся нарочно
    let name = yami_px::i18n::LANGS.iter().any(|l| l.1 == s);
    let missing = (!known && !name && s.chars().any(|c| matches!(c, 'а'..='я' | 'А'..='Я' | 'ё' | 'Ё'))) || (pt_br() && known && !lang_table().is_some_and(|t| t.contains_key(english)));
    if AUDIT.load(Ordering::Relaxed) && missing {
        if let Ok(mut m) = MISSED.lock() {
            if !m.iter().any(|x| x == out) {
                m.push(out.to_string());
            }
        }
    }
    out
}

/// пара прямо в коде (клавиши, doctor): русская и английская, остальные языки — по английской
pub fn select<'a>(ru: &'a str, en: &'a str) -> &'a str {
    if !translated() {
        return ru;
    }
    lang_table().and_then(|t| t.get(en).copied()).unwrap_or(en)
}

pub fn audit(on: bool) {
    AUDIT.store(on, Ordering::Relaxed);
}

/// надписи без перевода с прошлого вызова
pub fn take_missed() -> Vec<String> {
    MISSED.lock().map(|mut m| std::mem::take(&mut *m)).unwrap_or_default()
}

/// строка с подстановками: русский, английский и португальский шаблоны рядом (литералы: Rust
/// проверяет аргументы при сборке); остальные языки — по английскому шаблону и таблице языка (localize)
#[macro_export]
macro_rules! tf {
    ($ru:literal, $en:literal, $pt:literal $(, $a:expr)* $(,)?) => {
        if $crate::i18n::pt_br() {
            format!($pt $(, $a)*)
        } else if $crate::i18n::translated() {
            $crate::i18n::localize($en, format!($en $(, $a)*))
        } else {
            format!($ru $(, $a)*)
        }
    };
}

fn table() -> &'static HashMap<&'static str, &'static str> {
    static T: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    T.get_or_init(|| PAIRS.iter().copied().collect())
}

/// таблица текущего языка от английского (португальский и остальные); у русского и английского её нет
fn lang_table() -> Option<&'static HashMap<&'static str, &'static str>> {
    static T: [OnceLock<HashMap<&'static str, &'static str>>; 6] = [const { OnceLock::new() }; 6];
    let (i, rows): (usize, &'static [(&'static str, &'static str)]) = match code() {
        "pt-BR" => (0, PT_BR),
        "es" => (1, ES),
        "de" => (2, DE),
        "fr" => (3, FR),
        "uk" => (4, UK),
        "pl" => (5, PL),
        _ => return None,
    };
    Some(T[i].get_or_init(|| rows.iter().copied().collect()))
}

/// строка tf! на языке без своего шаблона в коде: out собрана по английскому шаблону en, значения
/// переезжают в шаблон языка (yami_px::i18n::refill); нет строки в таблице — остаётся английская
pub fn localize(en: &str, out: String) -> String {
    match lang_table().and_then(|t| t.get(en)) {
        Some(to) => yami_px::i18n::refill(en, &out, to).unwrap_or(out),
        None => out,
    }
}

include!("i18n_pt_br.rs");
include!("i18n_es.rs");
include!("i18n_de.rs");
include!("i18n_fr.rs");
include!("i18n_uk.rs");
include!("i18n_pl.rs");

const PAIRS: &[(&str, &str)] = &[
    ("enter — подтвердить", "enter — confirm"),
    ("enter — показать · esc — отмена", "enter — share · esc — cancel"),
    ("нажми, чтобы ввести пароль", "click to enter the password"),
    ("нет gio (пакет glib2)", "no gio (the glib2 package)"),
    ("нет udisksctl (пакет udisks2)", "no udisksctl (the udisks2 package)"),
    ("нужен пароль", "password required"),
    ("ок", "ok"),
    ("окна", "windows"),
    ("открыть", "open"),
    ("отмена", "cancel"),
    ("пароль", "password"),
    ("показать", "share"),
    ("проверяю…", "checking…"),
    ("разблокируй телефон и выбери «передача файлов»", "unlock the phone and choose “file transfer”"),
    ("что показать", "what to share"),
    ("экраны", "screens"),
];
