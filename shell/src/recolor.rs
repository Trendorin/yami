// Перекраска всего вокруг шелла по одной палитре: `yami-shell recolor` (шелл зовёт его с пониженным
// приоритетом, установщик — напрямую). Всегда: цвета рамок niri, обои шелла, свои шаблоны. Чужие
// программы — только те, что в themeApps настроек (настройки → цвет → программы, установщик); курсор
// по ключу cursor, набор значков по iconTheme. Здесь палитра, настройки, запись файлов и порядок
// шагов; программы — recolor_apps.rs, шаблоны — recolor_tpl.rs, картинки — recolor_art.rs.
//
//   recolor [palette.json]              палитра от шелла (~/.local/state/yami/palette.json)
//   recolor --accent '#c4161c' [--light]   без шелла: вывести оттенки из акцента
//   recolor --cursor-to DIR [--accent …]   курсор экрана входа в DIR
//   recolor --cursor | --icons | --fonts | --fetch | --profiles | --templates [--force NAME]
//
// Что вышло у программ и шаблонов — в ~/.local/state/yami/restyle.json (restyle.rs это показывает).

use serde_json::{Map, Value};
use std::cell::OnceCell;
use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub use crate::recolor_math::*;

pub type Res = io::Result<()>;

/// кто пишет файлы: в заголовках того, что мы создаём
pub const WHO: &str = "yami recolor";

/// подставляет в заготовку {p['ключ']} и {выражение} (ev), {{ и }} — обычные скобки
pub fn pyf(t: &str, p: &P, ev: &dyn Fn(&str) -> String) -> String {
    let mut out = String::with_capacity(t.len() * 2);
    let mut rest = t;
    while let Some(i) = rest.find(['{', '}']) {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        if tail.starts_with("{{") {
            out.push('{');
            rest = &tail[2..];
        } else if tail.starts_with("}}") {
            out.push('}');
            rest = &tail[2..];
        } else if tail.starts_with('{') {
            let Some(end) = tail.find('}') else {
                out.push_str(tail);
                return out;
            };
            let e = &tail[1..end];
            if let Some(k) = e.strip_prefix("p['").and_then(|x| x.strip_suffix("']")) {
                out.push_str(p.g(k));
            } else if e == "hi(p)" {
                out.push_str(&p.hi());
            } else {
                out.push_str(&ev(e));
            }
            rest = &tail[end + 1..];
        } else {
            out.push('}');
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    out
}

// ── файлы ───────────────────────────────────────────────────────────────────
/// файл целиком: тот же текст — не трогаем; через временное имя рядом; папка чужая — на месте
pub fn write(path: &Path, text: &str) -> Res {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    if std::fs::read(path).is_ok_and(|b| b == text.as_bytes()) {
        return Ok(());
    }
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let tmp = path.with_file_name(format!("{name}.tmp"));
    match std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, path)) {
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => std::fs::write(path, text),
        r => r,
    }
}

/// текст с заменой испорченных байтов
pub fn read_lossy(p: &Path) -> io::Result<String> {
    Ok(String::from_utf8_lossy(&std::fs::read(p)?).into_owned())
}

/// строки как их делят INI-подобные конфиги: любой перевод строки, в том числе \r и \f
pub fn lines_of(s: &str) -> Vec<&str> {
    lines_keep(s).into_iter().map(|l| l.trim_end_matches(is_break)).collect()
}

fn is_break(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

/// то же, с переводами строк на концах
pub fn lines_keep(s: &str) -> Vec<&str> {
    let mut out = vec![];
    let (mut from, mut it) = (0, s.char_indices().peekable());
    while let Some((i, c)) = it.next() {
        if is_break(c) {
            let mut end = i + c.len_utf8();
            if c == '\r' && it.peek().is_some_and(|(_, n)| *n == '\n') {
                it.next();
                end += 1;
            }
            out.push(&s[from..end]);
            from = end;
        }
    }
    if from < s.len() {
        out.push(&s[from..]);
    }
    out
}

/// путь со всеми ссылками; несуществующий хвост — как есть
pub fn real_path(p: &Path) -> PathBuf {
    use std::ffi::OsString;
    let base = if p.is_absolute() { PathBuf::from("/") } else { std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")) };
    let push = |st: &mut Vec<OsString>, x: &Path| {
        for c in x.components().rev() {
            if let std::path::Component::Normal(_) | std::path::Component::ParentDir = c {
                st.push(c.as_os_str().to_os_string());
            }
        }
    };
    let (mut out, mut parts, mut hops) = (base, vec![], 0);
    push(&mut parts, p);
    while let Some(c) = parts.pop() {
        if c == ".." {
            out.pop();
            continue;
        }
        let cand = out.join(&c);
        match std::fs::read_link(&cand) {
            Ok(t) if hops < 40 => {
                hops += 1;
                if t.is_absolute() {
                    out = PathBuf::from("/");
                }
                push(&mut parts, &t);
            }
            _ => out = cand,
        }
    }
    out
}

pub fn which(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(name).is_file()))
}

/// команда молча; её результат не важен
pub fn quiet(cmd: &[&str]) {
    let _ = Command::new(cmd[0]).args(&cmd[1..]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
}

pub fn gs_set(k: &str, v: &str) {
    quiet(&["gsettings", "set", "org.gnome.desktop.interface", k, v]);
}

pub fn output_of(cmd: &[&str]) -> Option<String> {
    let o = Command::new(cmd[0]).args(&cmd[1..]).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    Some(String::from_utf8_lossy(&o.stdout).into_owned())
}

pub fn signal(sig: &str, name: &str) {
    quiet(&["pkill", &format!("-{sig}"), "-x", name]);
}

// ── настройки шелла ─────────────────────────────────────────────────────────
pub struct Cx {
    pub home: PathBuf,
    pub cfg: PathBuf,
    pub state: PathBuf,
    pub data: PathBuf,
    pub st: Map<String, Value>,
    pub apps: HashSet<String>,
    screen: OnceCell<i64>,
}

/// программы, которые можно перекрашивать; до 2.7 ключ звался appThemes и выбирал только три сверх
/// постоянных
const APPS: [&str; 10] = ["gtk", "qt", "kitty", "fastfetch", "btop", "firefox", "mangohud", "foot", "cava", "starship"];
const OPT_IN_OLD: [&str; 3] = ["foot", "cava", "starship"];

fn env_dir(k: &str, fallback: PathBuf) -> PathBuf {
    std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or(fallback)
}

/// символы, которых в имени шрифта или набора быть не должно: строки из настроек идут в kitty.conf,
/// settings.ini, qt6ct и KDL, перевод строки из чужой темы иначе дописал бы туда свою команду
fn clean_name(s: &str) -> String {
    s.chars().filter(|c| !(c.is_control() || "\"\\'`$;,{}<>|&#=".contains(*c))).take(64).collect()
}

/// число как его записали руками: 24, 24.5, "24", true; нет — None
fn lenient_int(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f.trunc() as i64)),
        Value::Bool(b) => Some(*b as i64),
        Value::String(s) => s.trim().replace('_', "").parse().ok(),
        _ => None,
    }
}

pub fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

impl Cx {
    pub fn new() -> Cx {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
        let cfg = env_dir("XDG_CONFIG_HOME", home.join(".config"));
        let state = env_dir("XDG_STATE_HOME", home.join(".local/state")).join("yami");
        let data = env_dir("XDG_DATA_HOME", home.join(".local/share"));
        // настройки шелла: ~/.config/yami/shell.json, до переноса — старый ~/.config/yami-shell.json
        let sj = if cfg.join("yami/shell.json").exists() || !cfg.join("yami-shell.json").exists() { cfg.join("yami/shell.json") } else { cfg.join("yami-shell.json") };
        let mut st = std::fs::read_to_string(&sj).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| v.as_object().cloned()).unwrap_or_default();
        for k in ["fontUi", "fontMono", "fontShell", "fontFiles", "fontView", "cursor", "iconTheme"] {
            let v = st.get(k).and_then(|x| x.as_str()).map(clean_name).unwrap_or_default();
            st.insert(k.into(), v.into());
        }
        let lang = st.get("lang").and_then(|l| l.as_str()).unwrap_or("").trim().replace('_', "-").to_lowercase();
        let tag = lang.split('.').next().unwrap_or("").split('@').next().unwrap_or("").split('-').next().unwrap_or("").to_string();
        st.insert("lang".into(), yami_px::i18n::normalize(&tag).into());
        for (k, lo, hi, dflt) in [("cursorSize", 16, 96, 24), ("fontSize", 0, 64, 0), ("fontPt", 0, 48, 0), ("wallPixel", 1, 64, 1)] {
            let v = lenient_int(st.get(k).or(Some(&Value::from(dflt)))).map_or(dflt, |n| n.clamp(lo, hi));
            st.insert(k.into(), v.into());
        }
        let list = |v: &Value| v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect::<HashSet<String>>());
        let apps = match (st.get("themeApps").and_then(list), st.get("appThemes").and_then(list)) {
            (Some(v), _) => v,
            (_, Some(old)) => APPS.iter().filter(|a| !OPT_IN_OLD.contains(a)).map(|a| a.to_string()).chain(old).collect(),
            _ => APPS.iter().map(|a| a.to_string()).collect(),
        };
        Cx { home, cfg, state, data, st, apps, screen: OnceCell::new() }
    }

    pub fn on(&self, app: &str) -> bool {
        self.apps.contains(app)
    }
    pub fn s(&self, k: &str) -> &str {
        self.st.get(k).and_then(|v| v.as_str()).unwrap_or("")
    }
    pub fn n(&self, k: &str) -> i64 {
        self.st.get(k).and_then(|v| v.as_i64()).unwrap_or(0)
    }

    /// логическая высота самого большого экрана (масштаб niri уже учтён)
    fn screen_h(&self) -> i64 {
        *self.screen.get_or_init(|| {
            let t = output_of(&["niri", "msg", "-j", "outputs"]).unwrap_or_default();
            let v: Value = serde_json::from_str(&t).unwrap_or(Value::Null);
            v.as_object().and_then(|o| o.values().filter_map(|x| x["logical"]["height"].as_i64()).max()).unwrap_or(1080)
        })
    }

    /// размер UI-шрифта: из настроек (fontSize) или по экрану; Tiny5 чёток только на 16, 24, 32
    pub fn font_px(&self) -> i64 {
        let v = self.n("fontSize");
        if [16, 24, 32].contains(&v) {
            return v;
        }
        match self.screen_h() {
            h if h <= 1600 => 16,
            h if h <= 2200 => 24,
            _ => 32,
        }
    }

    /// папка набора значков (или курсоров) с таким именем, если он установлен
    pub fn icon_dir(&self, name: &str, cursors: bool) -> Option<PathBuf> {
        if name.is_empty() || name.contains('/') || name.starts_with('.') {
            return None;
        }
        let dirs = std::env::var("XDG_DATA_DIRS").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| "/usr/local/share:/usr/share".into());
        let extra = dirs.split(':').filter(|d| d.starts_with('/')).map(|d| Path::new(d).join("icons")).collect::<Vec<_>>();
        [self.data.join("icons"), self.home.join(".icons")].into_iter().chain(extra).map(|d| d.join(name)).find(|t| if cursors { t.join("cursors").is_dir() } else { t.join("index.theme").is_file() })
    }

    /// «~/…» внутри дома: для людей и для ключей состояния
    pub fn tilde(&self, p: &Path) -> String {
        match p.strip_prefix(&self.home) {
            Ok(r) => format!("~/{}", r.display()),
            Err(_) => p.display().to_string(),
        }
    }

    /// restyle.json: что вышло у программ и своих шаблонов; None убирает запись
    pub fn status(&self, key: &str, value: Option<Value>) -> Res {
        let path = self.state.join("restyle.json");
        let mut d = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| v.as_object().cloned()).unwrap_or_default();
        if key == "templates" {
            d.insert(key.into(), value.unwrap_or(Value::Array(vec![])));
        } else {
            let mut apps = d.get("apps").and_then(|a| a.as_object().cloned()).unwrap_or_default();
            match value {
                None => {
                    apps.remove(key);
                }
                Some(v) => {
                    apps.insert(key.into(), v);
                }
            }
            d.insert("apps".into(), apps.into());
        }
        write(&path, &(serde_json::to_string_pretty(&Value::Object(d)).unwrap_or_default() + "\n"))
    }

    fn palette(&self, args: &[String]) -> P {
        if let Some(a) = args.iter().position(|a| a == "--accent").and_then(|i| args.get(i + 1)) {
            return derive(a, 1.0, args.iter().any(|x| x == "--light"));
        }
        self.palette_at(&self.state.join("palette.json"))
    }

    fn palette_at(&self, path: &Path) -> P {
        std::fs::read_to_string(path).ok().and_then(|t| P::parse(&t)).unwrap_or_else(|| derive("#c4161c", 1.0, false))
    }
}

/// `yami-shell recolor …`
pub fn run(args: &[String]) -> i32 {
    use crate::{recolor_apps as a, recolor_art as art, recolor_tpl as tpl};
    let cx = Cx::new();
    let first = args.first().map(String::as_str);
    let report = |what: &str, r: Res| {
        if let Err(e) = r {
            eprintln!("recolor: {what}: {e}");
        }
    };
    match first {
        Some("--cursor") => report("cursor", art::cursor(&cx, &cx.palette(&[]))),
        Some("--icons") => {
            let p = cx.palette(&[]);
            report("icons", a::icons(&cx, &p));
            report("app_icons", art::app_icons(&cx, &p));
        }
        Some("--cursor-to") if args.len() > 1 => report("cursor", art::cursor_theme(&cx.palette(&args[2..]), Path::new(&args[1]), "yami-cursor")),
        Some("--profiles") => {
            for d in a::firefox_profiles(&cx) {
                println!("{}", d.display());
            }
        }
        Some("--fonts") => {
            let p = cx.palette(&[]);
            report("fonts", a::fonts(&cx, &p));
            report("firefox", a::firefox(&cx, &p));
            signal("USR1", "kitty");
        }
        Some("--templates") => {
            let force = if args.get(1).map(String::as_str) == Some("--force") { args.get(2).map(String::as_str) } else { None };
            report("templates", tpl::templates(&cx, &cx.palette(&[]), force));
        }
        Some("--fetch") => report("fastfetch", a::fastfetch(&cx, &cx.palette(&[]))),
        _ => {
            let p = match first {
                Some("--accent") if args.len() > 1 => {
                    let rest: Vec<&String> = args[2..].iter().filter(|a| *a != "--light").collect();
                    derive(&args[1], rest.first().and_then(|t| t.parse().ok()).unwrap_or(1.0), args.iter().any(|x| x == "--light"))
                }
                Some(path) => cx.palette_at(Path::new(path)),
                None => cx.palette(&[]),
            };
            // до 3.0 перекраска жила в скрипте рядом с состоянием
            let _ = std::fs::remove_file(cx.state.join("theme.py"));
            type Step = fn(&Cx, &P) -> Res;
            let steps: [(&str, Step); 18] = [("wallpaper", art::wallpaper), ("qt6ct", a::qt6ct), ("kde", a::kde), ("gtk", a::gtk), ("icons", a::icons), ("app_icons", art::app_icons), ("fonts", a::fonts), ("kitty", a::kitty), ("foot", a::foot), ("cava", a::cava), ("starship", a::starship), ("btop", a::btop), ("mangohud", a::mangohud), ("templates", |c, p| tpl::templates(c, p, None)), ("firefox", a::firefox), ("fastfetch", a::fastfetch), ("niri", a::niri), ("cursor", art::cursor)];
            // одна программа не должна ломать остальные
            for (name, step) in steps {
                report(name, step(&cx, &p));
            }
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_as_the_shell_writes_them() {
        assert_eq!(round_even(0.5), 0.0);
        assert_eq!(round_even(1.5), 2.0);
        assert_eq!(round_even(2.5), 2.0);
        assert_eq!(mix("#000000", "#ffffff", 0.5), "#808080");
        let p = derive("#c4161c", 1.0, false);
        assert_eq!(p.g("accent"), "#c4161c");
        assert_eq!(p.hi(), "#ffffff");
        assert!(derive("#c4161c", 1.0, true).light());
    }

    #[test]
    fn templates_of_text() {
        let p = P::of(vec![("bg", "#010203".to_string())]);
        assert_eq!(pyf("a {{b}} {p['bg']} {x}", &p, &|e| e.to_uppercase()), "a {b} #010203 X");
        assert_eq!(lines_of("a\r\nb\nc\x0cd"), vec!["a", "b", "c", "d"]);
        assert_eq!(lines_keep("a\nb"), vec!["a\n", "b"]);
    }

    #[test]
    fn names_from_settings_are_cleaned() {
        assert_eq!(clean_name("Noto\nSans;rm\"x"), "NotoSansrmx");
    }
}
