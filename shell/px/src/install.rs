//! Установка модуля из одного файла: `yami-x install` спрашивает настройки, показывает, что изменит,
//! и ставит только после «да». Всё, что модулю нужно на диске (.desktop, порталы, службы D-Bus),
//! вшито в бинарь. Без терминала и без --yes не ставит ничего.

use crate::{i18n, paths};
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

/// текст на трёх языках: русский, английский, бразильский португальский
pub type Tr = [&'static str; 3];

// установка модуля — русский, английский, португальский; остальные языки — по-английски
fn tr(t: &Tr) -> &'static str {
    if i18n::pt_br() {
        t[2]
    } else if i18n::translated() {
        t[1]
    } else {
        t[0]
    }
}

/// файл внутри XDG_DATA_HOME; @BIN@ в тексте заменяется путём к установленному бинарю
pub struct Asset {
    pub path: &'static str,
    pub body: &'static str,
}

/// то, что меняет поведение системы, — только если человек согласился (по умолчанию «нет»)
pub struct Choice {
    pub ask: Tr,
    pub assets: &'static [Asset],
    /// сделать модуль программой по умолчанию для типов файлов из его .desktop
    pub default_app: bool,
    /// что сделать после установки самому
    pub note: Option<Tr>,
}

pub struct Spec {
    pub name: &'static str,
    pub version: &'static str,
    pub about: Tr,
    pub deps: &'static str,
    pub assets: &'static [Asset],
    pub choices: &'static [Choice],
}

/// install, uninstall, deps; другая команда — None, программа работает дальше как обычно.
/// args — без имени программы
pub fn dispatch(spec: &Spec, args: &[String]) -> Option<i32> {
    let (cmd, rest) = args.split_first()?;
    match cmd.as_str() {
        "install" => Some(run(rest, |o| install(spec, o))),
        "uninstall" => Some(run(rest, |o| uninstall(spec, o))),
        "deps" => {
            println!("{}", spec.deps);
            Some(0)
        }
        _ => None,
    }
}

#[derive(Default, Debug, PartialEq)]
struct Opts {
    yes: bool,
    all: bool,
    look: Option<String>,
    lang: Option<String>,
}

fn parse(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-y" | "--yes" => o.yes = true,
            "--all" => o.all = true,
            "--look" => match it.next().map(String::as_str) {
                Some(v @ ("plain" | "yami")) => o.look = Some(v.to_string()),
                _ => return Err("--look plain|yami".into()),
            },
            "--lang" => match it.next().map(|v| i18n::normalize(v)) {
                Some(v) if !v.is_empty() => o.lang = Some(v.to_string()),
                _ => return Err("--lang ru|en|pt-BR".into()),
            },
            other => return Err(format!("unknown option: {other} (--yes, --all, --look plain|yami, --lang ru|en|pt-BR)")),
        }
    }
    Ok(o)
}

fn run(args: &[String], f: impl FnOnce(&Opts) -> i32) -> i32 {
    match parse(args) {
        Ok(o) => {
            i18n::set(o.lang.as_deref().unwrap_or(""));
            f(&o)
        }
        Err(e) => {
            eprintln!("{e}");
            2
        }
    }
}

fn line() -> Option<String> {
    let mut s = String::new();
    match std::io::stdin().lock().read_line(&mut s) {
        Ok(n) if n > 0 => Some(s.trim().to_string()),
        _ => None,
    }
}

fn prompt(text: &str) {
    print!("{text}");
    let _ = std::io::stdout().flush();
}

fn is_yes(a: &str) -> bool {
    a.chars().next().is_some_and(|c| matches!(c.to_lowercase().next(), Some('y' | 'д' | 's')))
}

/// да или нет; пустой ввод и конец ввода — нет
fn ask_yes(q: &str) -> bool {
    prompt(&format!("{q} [y/N] "));
    line().is_some_and(|a| is_yes(&a))
}

/// номер из списка; пустой или неверный ввод — первый пункт
fn ask_pick(q: &str, items: &[(&'static str, Tr)]) -> &'static str {
    println!("{q}");
    for (i, (_, label)) in items.iter().enumerate() {
        println!("  {}) {}", i + 1, tr(label));
    }
    prompt(&format!("[1-{}] ", items.len()));
    let i = line().and_then(|a| a.parse::<usize>().ok()).filter(|n| (1..=items.len()).contains(n)).unwrap_or(1);
    items[i - 1].0
}

fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

const NO_TTY: Tr = [
    "Здесь не у кого спросить подтверждение. Запусти в терминале или добавь --yes.",
    "There is no one to ask for confirmation. Run it in a terminal or add --yes.",
    "Não há quem confirme. Execute em um terminal ou adicione --yes.",
];
const CANCELLED: Tr = ["Отменено, ничего не изменено.", "Cancelled, nothing was changed.", "Cancelado, nada foi alterado."];

fn bin_dir() -> PathBuf {
    std::env::var_os("PREFIX").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| paths::home().join(".local")).join("bin")
}

fn short(p: &Path) -> String {
    match p.strip_prefix(paths::home()) {
        Ok(r) => format!("~/{}", r.display()),
        Err(_) => p.display().to_string(),
    }
}

fn desktop_of(spec: &Spec) -> Option<&'static Asset> {
    spec.assets.iter().find(|a| a.path.starts_with("applications/") && a.path.ends_with(&format!("/{}.desktop", spec.name)))
}

fn mimes(body: &str) -> Vec<String> {
    body.lines().find_map(|l| l.strip_prefix("MimeType=")).map(|v| v.split(';').filter(|m| !m.is_empty()).map(String::from).collect()).unwrap_or_default()
}

/// копия рядом и переименование: работающая программа остаётся при старом файле, полный диск не
/// оставит половину нового. Установленный бинарь сам себя не копирует
fn put_self(to: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let me = paths::self_exe();
    if let (Ok(a), Ok(b)) = (me.canonicalize(), to.canonicalize()) {
        if a == b {
            return Ok(());
        }
    }
    let dir = to.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{}.new", to.file_name().unwrap_or_default().to_string_lossy()));
    let done = std::fs::copy(&me, &tmp).and_then(|_| std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))).and_then(|_| std::fs::rename(&tmp, to));
    if done.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    done
}

fn install(spec: &Spec, o: &Opts) -> i32 {
    let ask = !o.yes;
    if ask && !interactive() {
        eprintln!("{}", tr(&NO_TTY));
        return 2;
    }
    println!("{} {}: {}\n", spec.name, spec.version, tr(&spec.about));

    // общий вид пишет шелл или первая установка; чужие настройки установщик не трогает
    let style = paths::style_file();
    let mut settings = None;
    if !style.exists() && !paths::shell_settings().exists() {
        let lang = match (&o.lang, ask) {
            (Some(l), _) => l.clone(),
            (None, false) => String::new(),
            (None, true) => {
                // названия языков — на них самих
                let mut items: Vec<(&'static str, Tr)> = vec![("", ["как в системе", "same as the system", "o mesmo do sistema"])];
                items.extend(i18n::LANGS.iter().map(|(c, n)| (*c, [*n; 3])));
                ask_pick(tr(&["Язык:", "Language:", "Idioma:"]), &items).to_string()
            }
        };
        i18n::set(&lang);
        let look = match (&o.look, ask) {
            (Some(l), _) => l.clone(),
            (None, false) => "plain".to_string(),
            (None, true) => ask_pick(tr(&["Вид:", "Look:", "Aparência:"]), &[("plain", ["нейтральный", "neutral", "neutra"]), ("yami", ["пиксельный yami", "the yami pixel look", "o visual em pixels do yami"])]).to_string(),
        };
        let mut v = serde_json::json!({ "preset": look });
        if look == "yami" {
            v["palette"] = "ash".into();
        }
        if !lang.is_empty() {
            v["lang"] = lang.into();
        }
        settings = Some(v);
    }

    let picked: Vec<&Choice> = spec.choices.iter().filter(|c| o.all || (ask && ask_yes(tr(&c.ask)))).collect();

    let bin = bin_dir().join(spec.name);
    let data = paths::data_dir();
    let assets: Vec<&Asset> = spec.assets.iter().chain(picked.iter().flat_map(|c| c.assets.iter())).collect();
    let default_for = if picked.iter().any(|c| c.default_app) { desktop_of(spec).map(|a| mimes(a.body)).unwrap_or_default() } else { vec![] };

    println!("\n{}", tr(&["Будет сделано:", "This will be done:", "Será feito:"]));
    println!("  + {}", short(&bin));
    for a in &assets {
        println!("  + {}", short(&data.join(a.path)));
    }
    if !default_for.is_empty() {
        println!("  + {} ({})", tr(&["программа по умолчанию, типов файлов", "the default program, file types", "o programa padrão, tipos de arquivo"]), default_for.len());
    }
    if let Some(v) = &settings {
        println!("  + {}  {v}", short(&style));
    }
    if ask && !ask_yes(&format!("\n{}", tr(&["Поставить?", "Install?", "Instalar?"]))) {
        println!("{}", tr(&CANCELLED));
        return 1;
    }

    let bin_text = bin.to_string_lossy().to_string();
    let wrote = assets.iter().try_for_each(|a| paths::write_atomic(&data.join(a.path), &a.body.replace("@BIN@", &bin_text))).and_then(|_| settings.as_ref().map_or(Ok(()), |v| paths::write_atomic(&style, &format!("{v}\n")))).and_then(|_| put_self(&bin));
    if let Err(e) = wrote {
        eprintln!("{}: {e}", spec.name);
        return 1;
    }
    if !default_for.is_empty() {
        let ok = std::process::Command::new("xdg-mime").arg("default").arg(format!("{}.desktop", spec.name)).args(&default_for).status().is_ok_and(|s| s.success());
        if !ok {
            eprintln!("{}", tr(&["Не вышло назначить программой по умолчанию: нужен xdg-mime.", "Could not set the default program: xdg-mime is needed.", "Não foi possível definir o programa padrão: é preciso o xdg-mime."]));
        }
    }
    println!("\n{}: {}", spec.name, short(&bin));
    for c in &picked {
        if let Some(n) = &c.note {
            println!("{}", tr(n));
        }
    }
    let in_path = std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| Some(d.as_path()) == bin.parent()));
    if !in_path {
        println!("{} {}", tr(&["Этой папки нет в PATH:", "This folder is not in PATH:", "Esta pasta não está no PATH:"]), bin.parent().map(short).unwrap_or_default());
    }
    0
}

/// mimeapps.list без данного .desktop; другие записи остаются как есть
fn mime_without(text: &str, app: &str) -> String {
    let mut out = String::new();
    for l in text.lines() {
        match l.split_once('=') {
            Some((k, v)) if !l.starts_with('[') && !l.starts_with('#') && v.split(';').any(|a| a == app) => {
                let rest: Vec<&str> = v.split(';').filter(|a| !a.is_empty() && *a != app).collect();
                if !rest.is_empty() {
                    out.push_str(&format!("{k}={};\n", rest.join(";")));
                }
            }
            _ => {
                out.push_str(l);
                out.push('\n');
            }
        }
    }
    out
}

fn uninstall(spec: &Spec, o: &Opts) -> i32 {
    if !o.yes && !interactive() {
        eprintln!("{}", tr(&NO_TTY));
        return 2;
    }
    let data = paths::data_dir();
    let mut files = vec![bin_dir().join(spec.name)];
    files.extend(spec.assets.iter().chain(spec.choices.iter().flat_map(|c| c.assets.iter())).map(|a| data.join(a.path)));
    files.retain(|f| f.exists());
    if files.is_empty() {
        println!("{}: {}", spec.name, tr(&["не установлен", "not installed", "não instalado"]));
        return 0;
    }
    println!("{}", tr(&["Будет удалено (настройки останутся):", "This will be removed (settings stay):", "Será removido (as configurações ficam):"]));
    for f in &files {
        println!("  - {}", short(f));
    }
    if !o.yes && !ask_yes(tr(&["Удалить?", "Remove?", "Remover?"])) {
        println!("{}", tr(&CANCELLED));
        return 1;
    }
    let mut code = 0;
    for f in &files {
        if let Err(e) = std::fs::remove_file(f) {
            eprintln!("{}: {e}", f.display());
            code = 1;
        }
    }
    let list = paths::config_dir().join("mimeapps.list");
    if let Ok(text) = std::fs::read_to_string(&list) {
        let cleaned = mime_without(&text, &format!("{}.desktop", spec.name));
        if cleaned != text && paths::write_atomic(&list, &cleaned).is_err() {
            code = 1;
        }
    }
    if code == 0 {
        println!("{}", tr(&["Удалено.", "Removed.", "Removido."]));
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(a: &[&str]) -> Vec<String> {
        a.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn options() {
        assert_eq!(parse(&s(&["--yes", "--all", "--look", "yami", "--lang", "pt_BR.UTF-8"])).unwrap(), Opts { yes: true, all: true, look: Some("yami".into()), lang: Some("pt-BR".into()) });
        assert!(parse(&s(&["--look", "shiny"])).is_err());
        assert!(parse(&s(&["--lang", "ja"])).is_err());
        assert!(parse(&s(&["--force"])).is_err());
    }

    #[test]
    fn only_a_clear_yes_counts() {
        for a in ["y", "Y", "yes", "д", "Да", "s", "sim"] {
            assert!(is_yes(a), "{a}");
        }
        for a in ["", "n", "нет", "1"] {
            assert!(!is_yes(a), "{a}");
        }
    }

    #[test]
    fn other_commands_are_left_to_the_program() {
        let spec = Spec { name: "x", version: "1", about: ["", "", ""], deps: "none", assets: &[], choices: &[] };
        assert_eq!(dispatch(&spec, &s(&["shot"])), None);
        assert_eq!(dispatch(&spec, &[]), None);
        assert_eq!(dispatch(&spec, &s(&["deps"])), Some(0));
        assert_eq!(dispatch(&spec, &s(&["install", "--nope"])), Some(2));
    }

    #[test]
    fn mime_types_come_from_the_desktop_file() {
        assert_eq!(mimes("[Desktop Entry]\nMimeType=image/png;image/gif;\nExec=x"), vec!["image/png", "image/gif"]);
        assert!(mimes("[Desktop Entry]\nExec=x").is_empty());
    }

    #[test]
    fn removal_keeps_every_other_default() {
        let t = "[Default Applications]\nimage/png=yami-view.desktop\nimage/gif=gimp.desktop;yami-view.desktop;\ntext/plain=kate.desktop\n# yami-view.desktop=x\n";
        assert_eq!(mime_without(t, "yami-view.desktop"), "[Default Applications]\nimage/gif=gimp.desktop;\ntext/plain=kate.desktop\n# yami-view.desktop=x\n");
    }
}
