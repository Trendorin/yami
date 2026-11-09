// Часть yami в конфиге niri. Конфиг niri — твой: yami его не переписывает, а подключается одной
// строкой include в начале (`yami niri connect`, первая настройка, установщик), так что всё, что
// написано в нём ниже, главнее. Своего конфига нет или это стартовый конфиг самого niri — yami
// кладёт свой стартовый (прежний — в копию рядом). Сама часть — файлы в ~/.config/yami/niri:
//   yami.kdl      его включает config.kdl: запуск шелла и те части ниже, что включены в настройках
//   base.kdl      вид niri от yami: ввод, раскладка окон, анимации, правила, окружение (niriLook)
//   colors.kdl cursor.kdl   цвета палитры и курсор, их пишет recolor (niriColors)
//   rules.kdl     размытие, игры, плавающие окна: Settings::fx_kdl (niriRules)
//   binds.kdl     клавиши и раскладки: keys.rs (niriKeys)
//   outputs.kdl   экраны: displays.rs (niriScreens)
// niri ≥ 26.04 понимает `include optional=true` и «~»; 25.11–26.03 — только обычный include:
// тогда путь относительный, а все части лежат всегда (пустые — с одной строкой комментария).

use crate::config::Settings;
use crate::dirs::niri_dir;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const BASE: &str = include_str!("../../config/niri/base.kdl");
const START: &str = include_str!("../../config/niri/config.kdl");

/// части в порядке включения: позже — главнее (у niri include позиционный)
const PARTS: [&str; 6] = ["base.kdl", "colors.kdl", "cursor.kdl", "rules.kdl", "binds.kdl", "outputs.kdl"];

fn part_on(s: &Settings, part: &str) -> bool {
    match part {
        "base.kdl" => s.niri_look,
        "colors.kdl" | "cursor.kdl" => s.niri_colors,
        "rules.kdl" => s.niri_rules,
        "binds.kdl" => s.niri_keys,
        "outputs.kdl" => s.niri_screens,
        _ => false,
    }
}

/// конфиг niri, который он прочтёт: NIRI_CONFIG или ~/.config/niri/config.kdl
pub fn config_path() -> PathBuf {
    match std::env::var_os("NIRI_CONFIG") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => yami_px::paths::config_dir().join("niri/config.kdl"),
    }
}

/// версия niri из `niri --version` («niri 26.04-yami.6 (…)», «niri 25.11 (…)»): (26, 4)
pub fn version() -> Option<(u32, u32)> {
    static V: OnceLock<Option<(u32, u32)>> = OnceLock::new();
    *V.get_or_init(|| {
        let o = std::process::Command::new("niri").arg("--version").output().ok()?;
        parse_version(&String::from_utf8_lossy(&o.stdout))
    })
}

fn parse_version(s: &str) -> Option<(u32, u32)> {
    let v = s.split_whitespace().nth(1)?;
    let mut it = v.split(|c: char| !c.is_ascii_digit()).filter(|x| !x.is_empty());
    Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
}

/// niri ≥ 26.04: include optional=true и «~». Версию не узнать — считаем новой
pub fn modern() -> bool {
    version().is_none_or(|v| v >= (26, 4))
}

/// include появился в niri 25.11: раньше подключиться нечем
pub fn too_old() -> bool {
    version().is_some_and(|v| v < (25, 11))
}

fn include(path: &str) -> String {
    if modern() {
        format!("include optional=true {}", crate::keys::kdl_str(path))
    } else {
        format!("include {}", crate::keys::kdl_str(path))
    }
}

/// путь от папки from до файла to («../yami/niri/yami.kdl»)
fn relative(from: &Path, to: &Path) -> String {
    let (a, b): (Vec<_>, Vec<_>) = (from.components().collect(), to.components().collect());
    let same = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    if same == 0 {
        return to.display().to_string();
    }
    let mut p = PathBuf::new();
    for _ in same..a.len() {
        p.push("..");
    }
    for c in &b[same..] {
        p.push(c);
    }
    p.display().to_string()
}

/// строка include части yami для config.kdl
fn our_include(cfg: &Path) -> String {
    let target = niri_dir().join("yami.kdl");
    if modern() {
        let p = match target.strip_prefix(yami_px::paths::home()) {
            Ok(r) => format!("~/{}", r.display()),
            Err(_) => target.display().to_string(),
        };
        include(&p)
    } else {
        include(&relative(cfg.parent().unwrap_or(Path::new("/")), &target))
    }
}

const BLOCK_HEAD: &str = "// yami: its niri look, keys, window rules, colours and screens (~/.config/yami/niri).\n// What you write below wins. `yami niri disconnect` takes these lines out.\n";

fn is_our_include(l: &str) -> bool {
    let t = l.trim_start();
    t.starts_with("include") && t.contains("yami/niri/yami.kdl")
}

/// yami подключён: в конфиге есть include его yami.kdl
pub fn connected(text: &str) -> bool {
    text.lines().any(is_our_include)
}

/// niri в PATH
fn niri_exe() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join("niri")).find(|p| p.is_file())
}

/// `needle` — кусок файла `file`; файл отображается в память, а не читается (бинарь niri — десятки МБ)
fn file_contains(file: &Path, needle: &[u8]) -> bool {
    use std::os::unix::io::AsRawFd;
    let Ok(f) = std::fs::File::open(file) else { return false };
    let len = f.metadata().map(|m| m.len() as usize).unwrap_or(0);
    if needle.is_empty() || len < needle.len() {
        return false;
    }
    // SAFETY: отображение только для чтения своего открытого файла, снимается здесь же; memmem не
    // выходит за переданные длины
    unsafe {
        let m = libc::mmap(std::ptr::null_mut(), len, libc::PROT_READ, libc::MAP_PRIVATE, f.as_raw_fd(), 0);
        if m == libc::MAP_FAILED {
            return false;
        }
        let hit = !libc::memmem(m, len, needle.as_ptr().cast(), needle.len()).is_null();
        libc::munmap(m, len);
        hit
    }
}

/// конфиг — тот самый, что niri пишет при первом запуске, без единой правки: его текст вшит в бинарь
/// niri. Поправленный руками (первая строка та же) — уже твой конфиг, его заменять нельзя
fn niri_default(text: &str) -> bool {
    text.trim_start().starts_with("// This config is in the KDL format: https://kdl.dev") && niri_exe().is_some_and(|n| file_contains(&n, text.as_bytes()))
}

/// файл, который на самом деле правится: ссылка (stow, свой репозиторий настроек) — то, куда она
/// ведёт; запись поверх ссылки превратила бы её в обычный файл. Ссылка в /nix/store (конфиг собрал
/// home-manager) только для чтения: строку человек добавит сам
fn real_config(cfg: &Path) -> Result<PathBuf, String> {
    if !std::fs::symlink_metadata(cfg).is_ok_and(|m| m.file_type().is_symlink()) {
        return Ok(cfg.to_path_buf());
    }
    let real = std::fs::canonicalize(cfg).unwrap_or_else(|_| cfg.to_path_buf());
    if real.starts_with("/nix/store") {
        return Err(crate::tf!("{} собран Nix: добавь в него сам строку\n  {}", "{} is built by Nix: add this line to it yourself\n  {}", "{} é gerado pelo Nix: adicione esta linha você mesmo\n  {}", yami_px::paths::pretty(cfg), our_include(cfg)));
    }
    Ok(real)
}

/// метка «yami убрали из конфига niri нарочно»: обновление не подключит его обратно
fn off_mark() -> PathBuf {
    crate::config::state_dir().join("niri-off")
}

/// программы, которые спорят с yami, если конфиг niri их запускает: вид спора — для подписи
const OTHERS: &[(&str, &str)] = &[
    ("waybar", "bar"),
    ("ironbar", "bar"),
    ("yambar", "bar"),
    ("hyprpanel", "bar"),
    ("eww", "bar"),
    ("quickshell", "shell"),
    ("qs", "shell"),
    ("noctalia-shell", "shell"),
    ("dms", "shell"),
    ("ags", "shell"),
    ("mako", "notif"),
    ("dunst", "notif"),
    ("swaync", "notif"),
    ("fnott", "notif"),
    ("swaybg", "wall"),
    ("swww-daemon", "wall"),
    ("swayidle", "idle"),
    ("hypridle", "idle"),
];

/// чужая программа в конфиге niri: строка spawn-at-startup с ней
#[derive(Clone, Debug)]
pub struct Other {
    pub prog: &'static str,
    pub kind: &'static str,
    /// строка целиком, как в файле: по ней и закомментируем (номер мог уехать)
    pub line: String,
}

/// первое слово команды в строке spawn: имя программы без пути
fn spawned(l: &str) -> Option<&str> {
    let t = l.trim_start();
    let rest = t.strip_prefix("spawn-at-startup").or_else(|| t.strip_prefix("spawn-sh-at-startup"))?;
    let cmd = rest.trim_start().strip_prefix('"')?;
    let first = cmd.split(['"', ' ']).next()?;
    Some(first.rsplit('/').next().unwrap_or(first))
}

/// что в конфиге niri (только в нём самом, не в его include) запускает второй бар, свои
/// уведомления, обои или ожидание
pub fn others() -> Vec<Other> {
    let Ok(text) = std::fs::read_to_string(config_path()) else { return vec![] };
    let mut v: Vec<Other> = vec![];
    for l in text.lines() {
        let Some(p) = spawned(l) else { continue };
        if let Some((prog, kind)) = OTHERS.iter().find(|(n, _)| *n == p) {
            if !v.iter().any(|o| o.prog == *prog) {
                v.push(Other { prog, kind, line: l.to_string() });
            }
        }
    }
    v
}

/// закомментировать запуск чужой программы (копия файла — рядом); вернуть — убрать «// yami: »
pub fn comment_out(o: &Other) -> Result<String, String> {
    let cfg = real_config(&config_path())?;
    let text = std::fs::read_to_string(&cfg).map_err(|e| e.to_string())?;
    if !text.lines().any(|l| l == o.line) {
        return Err(crate::tf!("строки с {} в конфиге уже нет", "the {} line is no longer in the config", "a linha do {} já não está na configuração", o.prog));
    }
    let new: Vec<String> = text.lines().map(|l| if l == o.line { format!("// yami: {l}") } else { l.to_string() }).collect();
    let saved = backup(&cfg, &text)?;
    crate::config::write_atomic(&cfg, &(new.join("\n") + "\n")).map_err(|e| e.to_string())?;
    if let Err(e) = validate(&cfg) {
        let _ = crate::config::write_atomic(&cfg, &text);
        return Err(e);
    }
    Ok(crate::tf!("{} больше не запускается со входом (копия: {})", "{} no longer starts at login (a copy: {})", "{} não inicia mais com o login (cópia: {})", o.prog, yami_px::paths::pretty(&saved)))
}

/// конфиг, который yami до 3.0 писал целиком (его перезаписывало каждое обновление)
fn old_yami(text: &str) -> bool {
    text.contains("YAMI // niri") && text.contains("\"yami-binds.kdl\"")
}

/// часть yami: base.kdl из своей копии и yami.kdl по настройкам. Пишется, только если другая:
/// каждая запись — перечитывание конфига niri
pub fn write_yami(set: &Settings) {
    let d = niri_dir();
    let _ = std::fs::create_dir_all(&d);
    put(&d.join("base.kdl"), BASE);
    let mut t = String::from(
        "// yami: written by yami-shell at every start and when its settings change, do not edit.\n\
         // Your niri config.kdl includes this file; what you write there after the include wins.\n\
         // The parts are switched in yami's settings → system → niri.\n",
    );
    for p in PARTS {
        if part_on(set, p) {
            t.push_str(&include(p));
            t.push('\n');
        }
    }
    t.push_str("\n// the shell (a second copy exits by itself) and the clipboard history\n");
    t.push_str("spawn-sh-at-startup \"n=0; until yami-shell; do n=$((n+1)); [ $n -ge 10 ] && break; sleep 1; done\"\n");
    t.push_str("spawn-sh-at-startup \"wl-paste --type text --watch cliphist store\"\n");
    t.push_str("spawn-sh-at-startup \"wl-paste --type image --watch cliphist store\"\n");
    put(&d.join("yami.kdl"), &t);
    // старый niri: include без optional — файл должен быть, хоть и пустой
    if !modern() {
        for p in PARTS {
            let f = d.join(p);
            if !f.exists() {
                let _ = crate::config::write_atomic(&f, "// yami: empty for now, the shell writes it\n");
            }
        }
    }
}

fn put(p: &Path, text: &str) {
    if std::fs::read_to_string(p).ok().as_deref() != Some(text) {
        let _ = crate::config::write_atomic(p, text);
    }
}

/// стартовый конфиг: свой, со строкой include под этот niri; от yami до 3.0 остаётся user.kdl
fn start_config(cfg: &Path, old: Option<&str>) -> String {
    let line = our_include(cfg);
    let mut t: String = START.lines().map(|l| if is_our_include(l) { line.clone() } else { l.to_string() }).collect::<Vec<_>>().join("\n") + "\n";
    if old.is_some_and(old_yami) && cfg.with_file_name("user.kdl").exists() {
        t.push_str(&format!("\n// your tweaks from before yami 3.0\n{}\n", include("user.kdl")));
    }
    t
}

/// свой конфиг: строки yami — после шапки из комментариев, всё остальное ниже и главнее
fn insert_block(text: &str, cfg: &Path) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let at = lines.iter().position(|l| !(l.trim().is_empty() || l.trim_start().starts_with("//"))).unwrap_or(lines.len());
    let mut out = lines[..at].join("\n");
    if !out.is_empty() && !out.ends_with("\n\n") {
        out.push_str(if out.ends_with('\n') { "\n" } else { "\n\n" });
    }
    out.push_str(BLOCK_HEAD);
    out.push_str(&our_include(cfg));
    out.push_str("\n\n");
    out.push_str(&lines[at..].join("\n"));
    out.push('\n');
    out
}

/// копия рядом с конфигом: config.kdl.bak-yami-ГГГГММДД-ЧЧММСС
fn backup(cfg: &Path, text: &str) -> Result<PathBuf, String> {
    let t = yami_px::time::local_now();
    let name = format!("{}.bak-yami-{:04}{:02}{:02}-{:02}{:02}{:02}", cfg.file_name().and_then(|n| n.to_str()).unwrap_or("config.kdl"), t.year, t.mon, t.day, t.hour, t.min, t.sec);
    let mut p = cfg.with_file_name(&name);
    let mut n = 2;
    while p.exists() {
        p = cfg.with_file_name(format!("{name}-{n}"));
        n += 1;
    }
    std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(p)
}

/// niri сам проверяет конфиг (есть niri в PATH); ошибка — его последние строки
fn validate(cfg: &Path) -> Result<(), String> {
    let Ok(o) = std::process::Command::new("niri").arg("validate").arg("-c").arg(cfg).output() else { return Ok(()) };
    if o.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&o.stderr);
    let tail: Vec<&str> = err.lines().filter(|l| !l.trim().is_empty()).collect();
    Err(tail[tail.len().saturating_sub(4)..].join("\n"))
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    /// своего конфига нет, стартовый niri или старый yami — свой стартовый; иначе строка в начало
    Auto,
    /// свой стартовый конфиг в любом случае (прежний — в копию)
    Replace,
    /// только строка include, даже в стартовый конфиг niri
    Insert,
}

#[derive(Clone, Debug, Default)]
pub struct Status {
    pub path: PathBuf,
    pub exists: bool,
    pub connected: bool,
    /// конфиг — стартовый от самого niri: его лучше заменить своим
    pub niri_default: bool,
}

pub fn status() -> Status {
    let path = config_path();
    let text = std::fs::read_to_string(&path).ok();
    Status { exists: text.is_some(), connected: text.as_deref().is_some_and(connected), niri_default: text.as_deref().is_some_and(niri_default), path }
}

/// подключить yami к конфигу niri; Ok — что сделано (для человека)
pub fn connect(mode: Mode) -> Result<String, String> {
    if too_old() {
        let (a, b) = version().unwrap_or_default();
        return Err(crate::tf!("niri {}.{:02} не умеет include: нужен niri 25.11 или новее", "niri {}.{:02} has no include: yami needs niri 25.11 or newer", "o niri {}.{:02} não tem include: o yami precisa do niri 25.11 ou mais novo", a, b));
    }
    crate::dirs::migrate();
    write_yami(&Settings::load());
    let cfg = real_config(&config_path())?;
    let old = std::fs::read_to_string(&cfg).ok();
    // заменить просили явно — заменяем; иначе подключённый конфиг трогать незачем
    if mode != Mode::Replace && old.as_deref().is_some_and(connected) {
        let _ = std::fs::remove_file(off_mark());
        return Ok(crate::tf!("yami уже подключён к {}", "yami is already in {}", "o yami já está em {}", yami_px::paths::pretty(&cfg)));
    }
    let fresh = match (&old, mode) {
        (None, _) | (Some(_), Mode::Replace) => true,
        (Some(t), Mode::Auto) => niri_default(t) || old_yami(t),
        (Some(_), Mode::Insert) => false,
    };
    let text = if fresh { start_config(&cfg, old.as_deref()) } else { insert_block(old.as_deref().unwrap_or(""), &cfg) };
    let saved = match old.as_deref() {
        Some(t) if !old_yami(t) => Some(backup(&cfg, t)?),
        _ => None,
    };
    if let Some(d) = cfg.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    crate::config::write_atomic(&cfg, &text).map_err(|e| format!("{}: {e}", cfg.display()))?;
    if let Err(e) = validate(&cfg) {
        match &old {
            Some(t) => {
                let _ = crate::config::write_atomic(&cfg, t);
            }
            None => {
                let _ = std::fs::remove_file(&cfg);
            }
        }
        return Err(crate::tf!("niri не принял конфиг, всё как было:\n{}", "niri rejected the config, nothing changed:\n{}", "o niri recusou a configuração, nada mudou:\n{}", e));
    }
    let _ = std::fs::remove_file(off_mark());
    let what = if fresh { crate::tf!("стартовый конфиг yami: {}", "yami's starting config: {}", "configuração inicial do yami: {}", yami_px::paths::pretty(&cfg)) } else { crate::tf!("одна строка yami в начале {}", "one yami line at the top of {}", "uma linha do yami no início de {}", yami_px::paths::pretty(&cfg)) };
    Ok(match saved {
        Some(b) => format!("{what} ({} {})", crate::tf!("прежний —", "the previous one is", "o anterior está em"), yami_px::paths::pretty(&b)),
        None => what,
    })
}

/// убрать строки yami из конфига niri (его вид, клавиши и правила уходят со следующим перечитыванием)
pub fn disconnect() -> Result<String, String> {
    let cfg = real_config(&config_path())?;
    let Ok(text) = std::fs::read_to_string(&cfg) else { return Ok(crate::tf!("конфига niri нет", "no niri config", "sem configuração do niri")) };
    if !connected(&text) {
        return Ok(crate::tf!("yami и так не подключён", "yami is not connected", "o yami não está conectado"));
    }
    let lines: Vec<&str> = text.lines().collect();
    let mut drop = vec![false; lines.len()];
    for (i, l) in lines.iter().enumerate() {
        if is_our_include(l) {
            drop[i] = true;
            // наши комментарии прямо над строкой
            let mut j = i;
            while j > 0 && BLOCK_HEAD.lines().any(|h| h == lines[j - 1]) {
                j -= 1;
                drop[j] = true;
            }
        }
    }
    // пустая строка, которую добавил connect, уходит вместе с блоком: две пустые подряд — одна
    for i in 1..lines.len() {
        if drop[i - 1] && !drop[i] && lines[i].trim().is_empty() && (0..i).rev().find(|&j| !drop[j]).is_none_or(|j| lines[j].trim().is_empty()) {
            drop[i] = true;
        }
    }
    let out: Vec<&str> = lines.iter().zip(&drop).filter(|(_, d)| !**d).map(|(l, _)| *l).collect();
    crate::config::write_atomic(&cfg, &(out.join("\n") + "\n")).map_err(|e| e.to_string())?;
    validate(&cfg).inspect_err(|_| {
        let _ = crate::config::write_atomic(&cfg, &text);
    })?;
    let _ = std::fs::create_dir_all(crate::config::state_dir());
    let _ = std::fs::write(off_mark(), "");
    Ok(crate::tf!("строки yami убраны из {}", "yami's lines are out of {}", "as linhas do yami saíram de {}", yami_px::paths::pretty(&cfg)))
}

/// конфиг до 3.0, который yami писал целиком: теперь стартовый, со строкой include; свой config.kdl,
/// где кто-то включал наши старые файлы, — эти include заменяет одна строка
pub fn migrate_old_config(niri: &Path) {
    let Ok(cfg) = real_config(&niri.join("config.kdl")) else { return };
    let Ok(text) = std::fs::read_to_string(&cfg) else { return };
    let new = if old_yami(&text) {
        start_config(&cfg, Some(&text))
    } else {
        let olds: Vec<String> = crate::dirs::OLD_NIRI.iter().map(|(o, _)| format!("\"{o}\"")).chain(["\"yami-local.kdl\"".to_string()]).collect();
        if !text.lines().any(|l| l.trim_start().starts_with("include") && olds.iter().any(|o| l.contains(o.as_str()))) {
            return;
        }
        let kept: Vec<&str> = text.lines().filter(|l| !(l.trim_start().starts_with("include") && olds.iter().any(|o| l.contains(o.as_str())))).collect();
        let t = kept.join("\n") + "\n";
        if connected(&t) {
            t
        } else {
            insert_block(&t, &cfg)
        }
    };
    if new != text {
        let _ = crate::config::write_atomic(&cfg, &new);
    }
}

/// всё, что шелл пишет сам при старте, — для установщика и терминала (`yami niri write`)
pub fn write_all() {
    crate::dirs::migrate();
    let set = Settings::load();
    write_yami(&set);
    crate::keys::write(&set);
    let lvl = crate::niri::config_level(crate::niri::fork_level());
    let p = niri_dir().join("rules.kdl");
    put(&p, &set.fx_kdl(lvl, false));
}

pub fn cli(y: &crate::cmd::Sty, args: &[&str]) -> i32 {
    match args.first().copied().unwrap_or("status") {
        "status" => {
            let s = status();
            let v = version().map(|(a, b)| format!("{a}.{b:02}")).unwrap_or_else(|| "?".into());
            println!("{} {}", y.dim("niri"), v);
            println!("{} {}", y.dim(&crate::tf!("конфиг", "config", "configuração")), yami_px::paths::pretty(&s.path));
            let state = if !s.exists {
                crate::tf!("нет: `yami niri connect` положит стартовый", "missing: `yami niri connect` writes a starting one", "ausente: `yami niri connect` cria uma inicial")
            } else if s.connected {
                crate::tf!("yami подключён", "yami is connected", "o yami está conectado")
            } else if s.niri_default {
                crate::tf!("стартовый конфиг niri, yami не подключён: `yami niri connect`", "niri's starting config, yami not connected: `yami niri connect`", "configuração inicial do niri, o yami não está conectado: `yami niri connect`")
            } else {
                crate::tf!("твой конфиг, yami не подключён: `yami niri connect` добавит одну строку", "your config, yami not connected: `yami niri connect` adds one line", "sua configuração, o yami não está conectado: `yami niri connect` adiciona uma linha")
            };
            println!("{} {}", y.dim(&crate::tf!("состояние", "state", "estado")), state);
            let set = Settings::load();
            let parts: Vec<&str> = PARTS.iter().filter(|p| part_on(&set, p)).copied().collect();
            println!("{} {}", y.dim(&crate::tf!("части", "parts", "partes")), parts.join(" "));
            0
        }
        "connect" => {
            // обновление и починка (`--update`): если yami убрали из конфига нарочно, так и остаётся
            if args.contains(&"--unless-off") && off_mark().exists() {
                println!("{}", crate::tf!("yami убран из конфига niri, оставляю так (yami niri connect вернёт)", "yami was taken out of niri's config, leaving it so (yami niri connect brings it back)", "o yami foi removido da configuração do niri, deixando assim (yami niri connect o traz de volta)"));
                return 0;
            }
            let mode = if args.contains(&"--replace") {
                Mode::Replace
            } else if args.contains(&"--insert") {
                Mode::Insert
            } else {
                Mode::Auto
            };
            report(y, connect(mode))
        }
        "disconnect" => report(y, disconnect()),
        "write" => {
            write_all();
            // установщик ждёт ответа: часть, которую niri не примет, остановит обновление, и точка
            // отката вернёт прежние файлы
            let cfg = config_path();
            if status().connected {
                if let Err(e) = validate(&cfg) {
                    eprintln!("{} {}\n{e}", y.bad("✗"), crate::tf!("niri не принимает часть yami", "niri rejects yami's part", "o niri recusa a parte do yami"));
                    return 1;
                }
            }
            0
        }
        _ => {
            eprintln!("yami niri status | connect [--replace|--insert|--unless-off] | disconnect | write");
            2
        }
    }
}

fn report(y: &crate::cmd::Sty, r: Result<String, String>) -> i32 {
    match r {
        Ok(m) => {
            println!("{} {m}", y.ok("✓"));
            0
        }
        Err(e) => {
            eprintln!("{} {e}", y.bad("✗"));
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_and_paths() {
        assert_eq!(parse_version("niri 26.04-yami.6 (78cbb03a)"), Some((26, 4)));
        assert_eq!(parse_version("niri 25.11 (abc)"), Some((25, 11)));
        assert_eq!(relative(Path::new("/h/.config/niri"), Path::new("/h/.config/yami/niri/yami.kdl")), "../yami/niri/yami.kdl");
    }

    #[test]
    fn a_text_inside_a_binary_is_found_whole() {
        let d = yami_px::paths::private_tmp("yami-test-niri").unwrap();
        let f = d.join("niri");
        std::fs::write(&f, b"\x7fELF\0\0// config\nbinds {}\n\0\0tail").unwrap();
        assert!(file_contains(&f, b"// config\nbinds {}\n"));
        assert!(!file_contains(&f, b"// config\nbinds {}\n// my tweak\n"));
        assert!(!file_contains(&f, b""));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn spawned_programs() {
        assert_eq!(super::spawned(r#"spawn-at-startup "waybar""#), Some("waybar"));
        assert_eq!(super::spawned(r#"  spawn-at-startup "/usr/bin/mako" "--x""#), Some("mako"));
        assert_eq!(super::spawned(r#"spawn-sh-at-startup "qs -c noctalia-shell""#), Some("qs"));
        assert_eq!(super::spawned(r#"// spawn-at-startup "waybar""#), None);
        assert_eq!(super::spawned("binds {"), None);
    }

    #[test]
    fn our_lines_go_after_the_header_and_come_out_whole() {
        let cfg = Path::new("/tmp/x/config.kdl");
        let mine = "// my niri\n// header\n\ninput {\n}\nbinds {\n}\n";
        let t = insert_block(mine, cfg);
        assert!(connected(&t));
        let at = t.find("include").unwrap();
        assert!(at > t.find("// header").unwrap() && at < t.find("input {").unwrap());
        assert!(!connected(mine));
    }
}
