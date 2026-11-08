// Язык скриншотера — тот же механизм, что в шелле: надписи по-русски, английский по
// таблице при отрисовке. Язык берётся из style.json.
// Третий язык — бразильский португальский: таблица PT_BR (i18n_pt_br.rs) от английского, у tf! —
// третий шаблон. Какой язык — решает yami_px::i18n (общий выбор для всех программ).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

pub use yami_px::i18n::{en, pt_br, set, translated};

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
    let missing = out.chars().any(|c| matches!(c, 'а'..='я' | 'А'..='Я' | 'ё' | 'Ё')) || (pt_br() && table().contains_key(s) && !pt_table().contains_key(english));
    if AUDIT.load(Ordering::Relaxed) && missing {
        if let Ok(mut m) = MISSED.lock() {
            if !m.iter().any(|x| x == out) {
                m.push(out.to_string());
            }
        }
    }
    out
}

/// пара прямо в коде (клавиши, doctor): русская и английская, португальская — по английской
pub fn select<'a>(ru: &'a str, en: &'a str) -> &'a str {
    if pt_br() {
        pt_table().get(en).copied().unwrap_or(en)
    } else if translated() {
        en
    } else {
        ru
    }
}

pub fn audit(on: bool) {
    AUDIT.store(on, Ordering::Relaxed);
}

/// надписи без перевода с прошлого вызова
pub fn take_missed() -> Vec<String> {
    MISSED.lock().map(|mut m| std::mem::take(&mut *m)).unwrap_or_default()
}

/// строка с подстановками: русский, английский и португальский шаблоны рядом (литералы: Rust
/// проверяет аргументы при сборке)
#[macro_export]
macro_rules! tf {
    ($ru:literal, $en:literal, $pt:literal $(, $a:expr)* $(,)?) => {
        if $crate::i18n::pt_br() {
            format!($pt $(, $a)*)
        } else if $crate::i18n::en() {
            format!($en $(, $a)*)
        } else {
            format!($ru $(, $a)*)
        }
    };
}

fn table() -> &'static HashMap<&'static str, &'static str> {
    static T: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    T.get_or_init(|| PAIRS.iter().copied().collect())
}

fn pt_table() -> &'static HashMap<&'static str, &'static str> {
    static T: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    T.get_or_init(|| PT_BR.iter().copied().collect())
}

include!("i18n_pt_br.rs");

const PAIRS: &[(&str, &str)] = &[
    ("QR-код", "QR code"),
    ("QR-код в буфер", "QR code to clipboard"),
    ("QR-код не найден", "no QR code found"),
    ("в буфер · ctrl+c", "copy · ctrl+c"),
    ("в папке", "show in folder"),
    ("выделение", "select"),
    ("выделение · 1", "select · 1"),
    ("маркер", "marker"),
    ("маркер · 5", "marker · 5"),
    ("мозаика", "mosaic"),
    ("мозаика · 6", "mosaic · 6"),
    ("номерок", "counter"),
    ("номерок · 7", "counter · 7"),
    ("нужен grim (пакет grim)", "grim is needed (package grim)"),
    ("область", "area"),
    ("окно", "window"),
    ("открыть", "open"),
    ("отмена · esc", "cancel · esc"),
    ("отменить · ctrl+z", "undo · ctrl+z"),
    ("перо", "pen"),
    ("перо · 2", "pen · 2"),
    ("пипетка", "colour picker"),
    ("пипетка: цвет в буфер · 8", "colour picker: hex to clipboard · 8"),
    ("рамка", "frame"),
    ("рамка · 3", "frame · 3"),
    ("скриншот в буфере обмена", "screenshot copied"),
    ("скриншот не вышел", "screenshot failed"),
    ("скриншот не сохранился", "screenshot not saved"),
    ("скриншот сохранён", "screenshot saved"),
    ("скриншот сохранён и скопирован", "screenshot saved and copied"),
    ("сохранить · enter", "save · enter"),
    ("сохранить как… · ctrl+s", "save as… · ctrl+s"),
    ("стрелка", "arrow"),
    ("стрелка · 4", "arrow · 4"),
    ("текст в буфер (tesseract)", "text to clipboard (tesseract)"),
    ("текст не распознан", "no text recognised"),
    ("текст со снимка", "text from the screenshot"),
    ("толщина · колесо", "thickness · wheel"),
    ("тяни — выделить · enter — весь экран · esc — отмена", "drag to select · enter for the whole screen · esc to cancel"),
    ("щёлкни или enter — весь монитор · esc — отмена", "click or enter for the whole monitor · esc to cancel"),
    ("щёлкни по окну · enter — окно под курсором · esc — отмена", "click a window · enter for the one under the cursor · esc to cancel"),
    ("экран", "screen"),
];
