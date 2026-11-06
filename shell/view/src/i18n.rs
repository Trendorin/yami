// UI strings are written in Russian and translated when drawn.
// English is the reference catalog; Brazilian Portuguese uses the same keys.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

pub use yami_px::i18n::{en, pt_br, set, translated};

static AUDIT: AtomicBool = AtomicBool::new(false);
static MISSED: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn tr(s: &str) -> &str {
    if !translated() {
        return s;
    }
    let english = table().get(s).copied().unwrap_or(s);
    let out = select(s, english);
    let missing = out.chars().any(|c| matches!(c, 'а'..='я' | 'А'..='Я' | 'ё' | 'Ё'))
        || (pt_br() && table().contains_key(s) && !pt_table().contains_key(english));
    if AUDIT.load(Ordering::Relaxed) && missing {
        if let Ok(mut m) = MISSED.lock() {
            if !m.iter().any(|x| x == out) {
                m.push(out.to_string());
            }
        }
    }
    out
}

/// Inline bilingual labels, such as keybinding descriptions.
pub fn select<'a>(ru: &'a str, en: &'a str) -> &'a str {
    if pt_br() { pt_table().get(en).copied().unwrap_or(en) } else if translated() { en } else { ru }
}

pub fn audit(on: bool) {
    AUDIT.store(on, Ordering::Relaxed);
}

pub fn take_missed() -> Vec<String> {
    MISSED.lock().map(|mut m| std::mem::take(&mut *m)).unwrap_or_default()
}

/// Literal templates keep Rust's compile-time format and argument checks.
#[macro_export]
macro_rules! tf {
    ($ru:literal, $en:literal, $pt:literal $(, $a:expr)* $(,)?) => {
        if $crate::i18n::pt_br() { format!($pt $(, $a)*) }
        else if $crate::i18n::en() { format!($en $(, $a)*) }
        else { format!($ru $(, $a)*) }
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
    ("сохранено", "saved"),
    ("пусто", "empty"),
    ("правка не сохранена: ctrl+s — сохранить, ctrl+z — отменить", "edit not saved: ctrl+s to save, ctrl+z to undo"),
    ("нечего отменять", "nothing to undo"),
    ("выдели область мышью", "select an area with the mouse"),
    ("изменений нет", "no changes"),
    ("анимацию не перезаписываю: ctrl+shift+s — сохранить как png", "not overwriting an animation: ctrl+shift+s saves a png"),
    ("в корзине", "moved to trash"),
    ("обои поменяются через секунду", "the wallpaper changes in a second"),
    ("скопировано", "copied"),
    ("слайд-шоу: каждые 4 с", "slideshow: every 4 s"),
    ("слайд-шоу выключено", "slideshow off"),
    ("нет картинок", "no images"),
    ("открываю", "opening"),
    ("назад · ←", "back · ←"),
    ("вперёд · →", "forward · →"),
    ("мельче · −", "smaller · −"),
    ("вписать · 0", "fit · 0"),
    ("крупнее · +", "bigger · +"),
    ("повернуть влево · shift+r", "rotate left · shift+r"),
    ("повернуть вправо · r", "rotate right · r"),
    ("отразить · h", "flip · h"),
    ("кадрировать · c", "crop · c"),
    ("в буфер · ctrl+c", "copy · ctrl+c"),
    ("сохранить · ctrl+s", "save · ctrl+s"),
    ("сохранить как… · ctrl+shift+s", "save as… · ctrl+shift+s"),
    ("сделать обоями · w", "set as wallpaper · w"),
    ("показать в папке · e", "show in folder · e"),
    ("в корзину · del", "move to trash · del"),
    ("сведения · i", "details · i"),
    ("слайд-шоу · пробел", "slideshow · space"),
    ("во весь экран · f", "fullscreen · f"),
    ("обрезать", "crop"),
    ("отмена", "cancel"),
    ("тяни по картинке · enter — обрезать · esc — отмена", "drag over the image · enter to crop · esc to cancel"),
    ("сведения", "details"),
    ("размер", "size"),
    ("файл", "file"),
    ("формат", "format"),
    ("изменён", "modified"),
    ("папка", "folder"),
    ("прозрачность", "transparency"),
    ("есть", "yes"),
    ("нет", "no"),
];
