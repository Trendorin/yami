// Перекраска чужих программ по палитре (recolor.rs): Qt и KDE, GTK, шрифты, kitty, foot, cava, starship,
// btop, Firefox и его родня, fastfetch, niri, MangoHud. Каждая — только если отмечена в themeApps; свои
// файлы человека не перезаписываем: чужое в конфиге остаётся, прежнее лежит рядом как *.bak-yami.

use crate::recolor::*;
use crate::recolor_data::{FF_FONT, FF_MENUS, NIRI_KDL, QSS};
use std::io;
use std::path::{Path, PathBuf};

const UI_FONT: &str = "Tiny5";

fn gtk_dirs() -> [&'static str; 2] {
    ["3.0", "4.0"]
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// значение ключа key= в строке, как ^key= с заменой всей строки
fn set_lines(t: &str, starts: impl Fn(&str) -> bool, new: &str) -> String {
    t.split('\n').map(|l| if starts(l) { new } else { l }).collect::<Vec<_>>().join("\n")
}

/// key=val в секции: есть ключ — заменить, нет — дописать в конец своей секции (не в конец файла:
/// там чужая секция, и программа его не увидит)
pub fn ini_set(path: &Path, key: &str, val: &str, section: &str) -> Res {
    let t = if path.exists() { std::fs::read_to_string(path)? } else { format!("{section}\n") };
    let prefix = format!("{key}=");
    let line = format!("{key}={val}");
    let text = if t.split('\n').any(|l| l.starts_with(&prefix)) {
        set_lines(&t, |l| l.starts_with(&prefix), &line)
    } else if lines_of(&t).contains(&section) {
        let mut lines: Vec<String> = lines_of(&t).into_iter().map(String::from).collect();
        let mut i = lines.iter().position(|l| l == section).map_or(0, |i| i + 1);
        while i < lines.len() && !lines[i].starts_with('[') {
            i += 1;
        }
        // перед пустыми строками, что отделяют следующую секцию
        while i > 0 && lines[i - 1].trim().is_empty() {
            i -= 1;
        }
        lines.insert(i, line);
        lines.join("\n") + "\n"
    } else {
        format!("{}\n\n{section}\n{line}\n", t.trim_end_matches('\n'))
    };
    write(path, &text)
}

/// key=val только внутри section (в kdeglobals «Theme=» бывает и в других секциях)
fn ini_section_set(path: &Path, section: &str, key: &str, val: &str) -> Res {
    let text = if path.exists() { std::fs::read_to_string(path)? } else { String::new() };
    let mut lines: Vec<String> = lines_of(&text).into_iter().map(String::from).collect();
    if !lines.iter().any(|l| l == section) {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(section.to_string());
    }
    let i = lines.iter().position(|l| l == section).map_or(0, |i| i + 1);
    let mut j = i;
    let line = format!("{key}={val}");
    while j < lines.len() && !lines[j].starts_with('[') {
        if lines[j].split('=').next().unwrap_or("").trim() == key {
            lines[j] = line;
            return write(path, &(lines.join("\n") + "\n"));
        }
        j += 1;
    }
    while j > i && lines[j - 1].trim().is_empty() {
        j -= 1;
    }
    lines.insert(j, line);
    write(path, &(lines.join("\n") + "\n"))
}

// ── Qt: qt6ct ───────────────────────────────────────────────────────────────
pub fn qt6ct(cx: &Cx, p: &P) -> Res {
    if !cx.on("qt") {
        return Ok(());
    }
    let hi = p.hi();
    // порядок ролей QPalette: WindowText, Button, Light, Midlight, Dark, Mid, Text, BrightText,
    // ButtonText, Base, Window, Shadow, Highlight, HighlightedText, Link, LinkVisited,
    // AlternateBase, NoRole, ToolTipBase, ToolTipText, PlaceholderText
    let alt = mix(p.g("bg"), p.g("panel"), 0.6);
    let act: Vec<&str> = vec![p.g("text"), p.g("surface"), p.g("border"), p.g("surfaceHi"), p.g("codeBg"), p.g("panel"), p.g("text"), &hi, p.g("text"), p.g("bg"), p.g("panel"), "#000000", p.g("selection"), &hi, p.g("bright"), p.g("accent"), &alt, "#000000", p.g("panel"), p.g("text"), p.g("textFaint")];
    let mut dis = act.clone();
    for i in [0, 6, 8, 13, 19] {
        dis[i] = p.g("textFaint");
    }
    dis[12] = p.g("surfaceHi");
    let mut ina = act.clone();
    ina[12] = p.g("dim");
    let j = |xs: &[&str]| xs.iter().map(|h| format!("#ff{}", h.trim_start_matches('#'))).collect::<Vec<_>>().join(", ");
    write(&cx.cfg.join("qt6ct/colors/yami.conf"), &format!("[ColorScheme]\nactive_colors={}\ndisabled_colors={}\ninactive_colors={}\n", j(&act), j(&dis), j(&ina)))?;
    let d = cx.cfg.join("qt6ct/qss/yami");
    crate::recolor_art::qt_pixels(p, &d)?;
    // диалоги файлов Qt — через портал, то есть yami-files (как у GTK и браузеров)
    ini_set(&cx.cfg.join("qt6ct/qt6ct.conf"), "standard_dialogs", "xdgdesktopportal", "[Appearance]")?;
    let u = |n: &str| format!("url({}.png)", d.join(n).display());
    let sl = |n: &str| format!("border-image: {} 4 4 4 4 stretch stretch; border-width: 4px;", u(n));
    let arg = |e: &str| e.split('\'').nth(1).unwrap_or("").to_string();
    let css = pyf(QSS, p, &|e| if e.starts_with("sl(") { sl(&arg(e)) } else { u(&arg(e)) });
    write(&cx.cfg.join("qt6ct/qss/yami.qss"), &css)
}

// ── KDE: схема цветов и kdeglobals ──────────────────────────────────────────
type Sections = Vec<(String, Vec<(String, String)>)>;

fn ini_parse(text: &str) -> Sections {
    let mut res: Sections = vec![];
    let mut cur: Option<usize> = None;
    for line in lines_of(text) {
        let t = line.trim_end();
        if t.starts_with('[') && t.ends_with(']') {
            let at = res.iter().position(|(s, _)| s == t).unwrap_or_else(|| {
                res.push((t.to_string(), vec![]));
                res.len() - 1
            });
            cur = Some(at);
        } else if let (Some(at), Some((k, v))) = (cur, line.split_once('=')) {
            if !line.trim_start().starts_with('#') {
                kv_set(&mut res[at].1, k, v);
            }
        }
    }
    res
}

fn kv_set(kv: &mut Vec<(String, String)>, k: &str, v: &str) {
    match kv.iter_mut().find(|(x, _)| x == k) {
        Some(e) => e.1 = v.to_string(),
        None => kv.push((k.to_string(), v.to_string())),
    }
}

fn section<'a>(cfg: &'a mut Sections, name: &str) -> &'a mut Vec<(String, String)> {
    let at = cfg.iter().position(|(s, _)| s == name).unwrap_or_else(|| {
        cfg.push((name.to_string(), vec![]));
        cfg.len() - 1
    });
    &mut cfg[at].1
}

pub fn kde(cx: &Cx, p: &P) -> Res {
    if !cx.on("qt") {
        return Ok(());
    }
    let c = |h: &str| {
        let (r, g, b) = rgb(h);
        format!("{r},{g},{b}")
    };
    let groups = [("Window", p.g("panel").to_string(), p.g("surface").to_string()), ("View", p.g("bg").to_string(), mix(p.g("bg"), p.g("panel"), 0.6)), ("Button", p.g("surface").to_string(), p.g("surfaceHi").to_string()), ("Selection", p.g("selection").to_string(), p.g("dim").to_string()), ("Tooltip", p.g("panel").to_string(), p.g("surface").to_string()), ("Complementary", p.g("codeBg").to_string(), p.g("panel").to_string()), ("Header", p.g("panel").to_string(), p.g("surface").to_string())];
    let mut out: Vec<String> = ["[General]", "ColorScheme=Yami", "Name=YAMI", "shadeSortColumn=true", "", "[KDE]", "contrast=4", ""].iter().map(|s| s.to_string()).collect();
    for (g, bg, alt) in &groups {
        let fg = if *g == "Selection" { p.hi() } else { p.g("text").to_string() };
        out.extend([format!("[Colors:{g}]"), format!("BackgroundNormal={}", c(bg)), format!("BackgroundAlternate={}", c(alt)), format!("ForegroundNormal={}", c(&fg)), format!("ForegroundInactive={}", c(p.g("textDim"))), format!("ForegroundActive={}", c(p.g("bright"))), format!("ForegroundLink={}", c(p.g("bright"))), format!("ForegroundVisited={}", c(p.g("accent"))), format!("ForegroundNegative={}", c("#ff4a3d")), format!("ForegroundNeutral={}", c(p.g("warn"))), format!("ForegroundPositive={}", c(p.g("ok"))), format!("DecorationFocus={}", c(p.g("accent"))), format!("DecorationHover={}", c(p.g("bright"))), String::new()]);
    }
    out.extend(["[Colors:Header][Inactive]".to_string(), format!("BackgroundNormal={}", c(p.g("bg"))), format!("BackgroundAlternate={}", c(p.g("panel"))), format!("ForegroundNormal={}", c(p.g("textDim"))), String::new(), "[WM]".to_string(), format!("activeBackground={}", c(p.g("panel"))), format!("activeForeground={}", c(p.g("text"))), format!("activeBlend={}", c(p.g("accent"))), format!("inactiveBackground={}", c(p.g("bg"))), format!("inactiveForeground={}", c(p.g("textFaint"))), format!("inactiveBlend={}", c(p.g("border"))), String::new()]);
    let scheme = out.join("\n");
    write(&cx.home.join(".local/share/color-schemes/Yami.colors"), &scheme)?;
    let target = cx.cfg.join("kdeglobals");
    let mut cfg = if target.exists() { ini_parse(&std::fs::read_to_string(&target)?) } else { vec![] };
    for (sec, kv) in ini_parse(&scheme) {
        if sec.starts_with("[Colors:") || sec == "[WM]" {
            *section(&mut cfg, &sec) = kv;
        } else if sec == "[General]" {
            kv_set(section(&mut cfg, &sec), "ColorScheme", "Yami");
        }
    }
    // программы KDE сами включают Breeze, если он стоит; Fusion — чтобы работал наш QSS
    kv_set(section(&mut cfg, "[KDE]"), "widgetStyle", "Fusion");
    let text = cfg.iter().map(|(s, kv)| format!("{s}\n{}", kv.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("\n"))).collect::<Vec<_>>().join("\n\n") + "\n";
    write(&target, &text)
}

// ── GTK 3/4 ─────────────────────────────────────────────────────────────────
/// наш стиль GTK — в своём файле yami.css; в gtk.css только строка @import (твой gtk.css остаётся
/// твоим). gtk.css, целиком написанный yami до 3.0, заменяется одной этой строкой
const GTK_IMPORT: &str = "@import url(\"yami.css\");";

fn gtk_attach(dir: &Path, css: &str) -> Res {
    write(&dir.join("yami.css"), css)?;
    let f = dir.join("gtk.css");
    let cur = std::fs::read_to_string(&f).unwrap_or_default();
    if cur.lines().any(|l| l.trim() == GTK_IMPORT) {
        return Ok(());
    }
    let text = if cur.trim().is_empty() || cur.starts_with("/* YAMI") {
        format!("/* yami: its colours live in yami.css; write your own rules below. */\n{GTK_IMPORT}\n")
    } else {
        // @import — до остальных правил
        format!("{GTK_IMPORT}\n{cur}")
    };
    if std::fs::symlink_metadata(&f).is_ok_and(|m| m.file_type().is_symlink()) {
        std::fs::write(&f, text)?;
    } else {
        write(&f, &text)?;
    }
    Ok(())
}

/// GTK выключили (или только GTK 4): наша строка и yami.css уходят, твои правила остаются
fn gtk_detach(dir: &Path) -> Res {
    let _ = std::fs::remove_file(dir.join("yami.css"));
    let f = dir.join("gtk.css");
    let Ok(cur) = std::fs::read_to_string(&f) else { return Ok(()) };
    if cur.starts_with("/* YAMI") {
        let _ = std::fs::remove_file(&f);
        return Ok(());
    }
    let keep: Vec<&str> = cur.lines().filter(|l| l.trim() != GTK_IMPORT && !l.starts_with("/* yami: its colours live in yami.css")).collect();
    if keep.len() == cur.lines().count() {
        return Ok(());
    }
    if keep.iter().all(|l| l.trim().is_empty()) {
        let _ = std::fs::remove_file(&f);
    } else {
        std::fs::write(&f, keep.join("\n") + "\n")?;
    }
    Ok(())
}

pub fn gtk(cx: &Cx, p: &P) -> Res {
    if !cx.on("gtk") {
        for v in gtk_dirs() {
            gtk_detach(&cx.cfg.join(format!("gtk-{v}")))?;
        }
        return Ok(());
    }
    let side = mix(p.g("bg"), p.g("panel"), 0.5);
    let d: Vec<(&str, &str)> = vec![("accent_color", p.g("bright")), ("accent_bg_color", p.g("accent")), ("accent_fg_color", p.g("onAccent")), ("destructive_color", "#ff4a3d"), ("destructive_bg_color", "#8a1a14"), ("destructive_fg_color", "#ffffff"), ("success_color", p.g("ok")), ("warning_color", p.g("warn")), ("error_color", "#ff4a3d"), ("window_bg_color", p.g("panel")), ("window_fg_color", p.g("text")), ("view_bg_color", p.g("bg")), ("view_fg_color", p.g("text")), ("headerbar_bg_color", p.g("panel")), ("headerbar_fg_color", p.g("text")), ("headerbar_border_color", p.g("border")), ("headerbar_backdrop_color", p.g("bg")), ("sidebar_bg_color", &side), ("sidebar_fg_color", p.g("text")), ("sidebar_backdrop_color", p.g("bg")), ("card_bg_color", p.g("surface")), ("card_fg_color", p.g("text")), ("dialog_bg_color", p.g("panel")), ("dialog_fg_color", p.g("text")), ("popover_bg_color", p.g("panel")), ("popover_fg_color", p.g("text")), ("thumbnail_bg_color", p.g("surface")), ("thumbnail_fg_color", p.g("text")), ("borders", p.g("border"))];
    let mut css = format!("/* YAMI: пишет {WHO} по палитре шелла. */\n"); // i18n-ok
    for (k, v) in &d {
        css += &format!("@define-color {k} {v};\n");
    }
    // жёсткие тени: в светлой теме — слабее, иначе панели обведены сажей
    let light = p.light();
    let sh = if light { "0.22" } else { "0.6" };
    css += &format!("@define-color headerbar_shade_color rgba(0, 0, 0, {sh});\n@define-color card_shade_color rgba(0, 0, 0, {sh});\n@define-color shade_color rgba(0, 0, 0, {sh});\n@define-color scrollbar_outline_color {};\n\n* {{ border-radius: 0; }}\n", if light { "#ffffff" } else { "#000000" });
    css += &format!("selection, *:selected {{ background-color: {}; color: {}; }}\n", p.g("selection"), p.hi());
    css += &format!("popover > contents, menu, .menu, tooltip {{ border: 1px solid {}; }}\n", p.g("border"));
    // пиксельный вид: без плавных переходов, кнопки с фаской и жёсткой тенью, как в шелле
    css += "* { transition: none; -gtk-icon-shadow: none; }\n";
    let k: f64 = if light { 0.4 } else { 1.0 };
    let edge = mix(p.g("border"), p.g("accent"), 0.5);
    css += &format!("button {{ border: 1px solid {}; box-shadow: inset 1px 1px 0 alpha(white, {}), 2px 2px 0 alpha(black, {:.2}); }}\n", p.g("border"), if light { "0.5" } else { "0.06" }, 0.55 * k);
    css += &format!("button:hover {{ border-color: {}; }}\n", p.g("accent"));
    css += &format!("button:active, button:checked {{ box-shadow: inset 2px 2px 0 alpha(black, {:.2}); }}\n", 0.45 * k);
    css += &format!("button.suggested-action {{ background: {}; color: {}; }}\n", p.g("accent"), p.g("onAccent"));
    css += &format!("entry, spinbutton, dropdown > button, combobox > box > button {{ background: {}; border: 1px solid {}; }}\n", p.g("codeBg"), p.g("border"));
    css += &format!("entry:focus-within {{ border-color: {}; outline: none; }}\n", p.g("accent"));
    css += &format!("window.csd, window.dialog, messagedialog, dialog-host > dialog {{ border: 1px solid {edge}; box-shadow: 4px 4px 0 alpha(black, {:.2}); }}\n", 0.6 * k);
    // GTK 4 (libadwaita: Nautilus, приложения GNOME) можно оставить в их собственном виде (#31)
    let gtk4 = cx.st.get("themeGtk4").and_then(|v| v.as_bool()).unwrap_or(true);
    for v in gtk_dirs() {
        let dir = cx.cfg.join(format!("gtk-{v}"));
        if v == "4.0" && !gtk4 {
            gtk_detach(&dir)?;
        } else {
            gtk_attach(&dir, &css)?;
        }
    }
    gtk_mode(cx, light)?;
    // акцент libadwaita/GNOME — ближайший из их набора по оттенку
    let (r, g, b) = unit(p.g("accent"));
    let (h, _, sat) = rgb_to_hls(r, g, b);
    let name = if sat < 0.18 {
        "slate"
    } else {
        let hues = [(0.0, "red"), (30.0, "orange"), (52.0, "yellow"), (120.0, "green"), (175.0, "teal"), (215.0, "blue"), (275.0, "purple"), (330.0, "pink"), (360.0, "red")];
        let mut best = hues[0];
        for x in hues {
            if (x.0 - h * 360.0_f64).abs() < (best.0 - h * 360.0_f64).abs() {
                best = x;
            }
        }
        best.1
    };
    if which("gsettings") {
        gs_set("accent-color", name);
    }
    Ok(())
}

/// светлая или тёмная основа GTK: тема adw-gtk3(-dark) и prefer-dark в settings.ini, color-scheme в
/// gsettings (его читают GTK4/libadwaita и портал). Остальные ключи не трогаем
fn gtk_mode(cx: &Cx, light: bool) -> Res {
    if !cx.on("gtk") {
        return Ok(());
    }
    let theme = if light { "adw-gtk3" } else { "adw-gtk3-dark" };
    for v in gtk_dirs() {
        let ini = cx.cfg.join(format!("gtk-{v}/settings.ini"));
        if !ini.exists() {
            continue;
        }
        let text = std::fs::read_to_string(&ini)?;
        let lines = lines_of(&text);
        let mut want: Vec<(&str, String)> = vec![("gtk-theme-name", theme.to_string()), ("gtk-application-prefer-dark-theme", if light { "false" } else { "true" }.to_string())];
        let icons = lines.iter().find_map(|l| l.strip_prefix("gtk-icon-theme-name=")).map_or("", |s| s.trim());
        if let Some(other) = icon_pair(cx, icons, light) {
            want.push(("gtk-icon-theme-name", other));
        }
        let mut out: Vec<String> = vec![];
        for ln in &lines {
            let k = ln.split('=').next().unwrap_or("").trim();
            match want.iter().position(|(x, _)| *x == k) {
                Some(i) => {
                    let (x, v2) = want.remove(i);
                    out.push(format!("{x}={v2}"));
                }
                None => out.push(ln.to_string()),
            }
        }
        out.extend(want.iter().map(|(k, v2)| format!("{k}={v2}")));
        write(&ini, &(out.join("\n") + "\n"))?;
    }
    if which("gsettings") {
        gs_set("gtk-theme", theme);
        gs_set("color-scheme", if light { "prefer-light" } else { "prefer-dark" });
        let cur = output_of(&["gsettings", "get", "org.gnome.desktop.interface", "icon-theme"]).unwrap_or_default();
        if let Some(other) = icon_pair(cx, cur.trim().trim_matches('\''), light) {
            gs_set("icon-theme", &other);
        }
    }
    Ok(())
}

/// тёмный или светлый вариант набора значков под тему (Foo-Dark ↔ Foo / Foo-Light), если он
/// установлен. Только для набора «как в системе»: выбранный в шелле руками ставится как есть
fn icon_pair(cx: &Cx, name: &str, light: bool) -> Option<String> {
    if !cx.s("iconTheme").is_empty() {
        return None;
    }
    let base = ["-Dark", "-dark", "-Light", "-light"].iter().find_map(|s| name.strip_suffix(s)).unwrap_or(name);
    let want = if light { vec![format!("{base}-Light"), format!("{base}-light"), base.to_string()] } else { vec![format!("{base}-Dark"), format!("{base}-dark")] };
    want.into_iter().find(|w| w != name && cx.icon_dir(w, false).is_some())
}

// ── шрифты ──────────────────────────────────────────────────────────────────
/// есть ли внутри файл с таким началом имени
fn has_file(d: &Path, prefix: &str) -> bool {
    let Ok(rd) = std::fs::read_dir(d) else { return false };
    rd.flatten().any(|e| e.file_name().to_string_lossy().starts_with(prefix) || (e.file_type().is_ok_and(|t| t.is_dir()) && has_file(&e.path(), prefix)))
}

/// Monocraft, если он есть (кэш fontconfig сразу после установки бывает ещё старым — смотрим и сами
/// файлы), иначе Terminess из комплекта
fn mono_font(cx: &Cx) -> String {
    let listed = std::process::Command::new("fc-list").args(["-q", "Monocraft"]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().is_ok_and(|s| s.success());
    let dirs = [cx.home.join(".local/share/fonts"), cx.home.join(".fonts"), PathBuf::from("/usr/share/fonts"), PathBuf::from("/usr/local/share/fonts")];
    if listed || dirs.iter().any(|d| has_file(d, "Monocraft")) { "Monocraft".into() } else { "Terminess Nerd Font Mono".into() }
}

/// (семейство, pt): fontUi из настроек — "" Tiny5 по сетке, "sans" системный, иначе своё
fn ui_font(cx: &Cx) -> (String, i64) {
    let fam = cx.s("fontUi").trim().to_string();
    if fam.is_empty() {
        return (UI_FONT.to_string(), cx.font_px() * 3 / 4);
    }
    let pt = match cx.n("fontPt") {
        0 => 11,
        v => v,
    };
    (if fam == "sans" { "Sans".to_string() } else { fam }, pt)
}

/// GTK 3/4, gsettings, qt6ct, kitty: шрифты из настроек шелла. Пиксельные — ровно по своей сетке
/// (96 dpi: 16 px = 12 pt), иначе они мылятся
pub fn fonts(cx: &Cx, _p: &P) -> Res {
    let (fam, pt) = ui_font(cx);
    let px = cx.font_px();
    let own = cx.s("fontMono").trim().to_string();
    let mono = if own.is_empty() { mono_font(cx) } else { own };
    let mono_pt = if mono == "Monocraft" || mono == "Terminess Nerd Font Mono" { px * 3 / 4 } else { pt.max(11) };
    if cx.on("gtk") {
        for v in gtk_dirs() {
            ini_set(&cx.cfg.join(format!("gtk-{v}/settings.ini")), "gtk-font-name", &format!("{fam} {pt}"), "[Settings]")?;
        }
    }
    let qt = cx.cfg.join("qt6ct/qt6ct.conf");
    if cx.on("qt") && qt.exists() {
        let q = |f: &str, size: i64| format!("\"{f},{size},-1,5,400,0,0,0,0,0,0,0,0,0,0,1\"");
        let t = std::fs::read_to_string(&qt)?;
        let t = set_lines(&t, |l| l.starts_with("general="), &format!("general={}", q(&fam, pt)));
        let t = set_lines(&t, |l| l.starts_with("fixed="), &format!("fixed={}", q(&mono, mono_pt)));
        write(&qt, &t)?;
    }
    if cx.on("gtk") && which("gsettings") {
        gs_set("font-name", &format!("{fam} {pt}"));
        gs_set("document-font-name", &format!("{fam} {pt}"));
        gs_set("monospace-font-name", &format!("{mono} {mono_pt}"));
    }
    if cx.on("kitty") {
        write(&cx.cfg.join("kitty/yami-font.conf"), &format!("# YAMI: written by {WHO} (settings → fonts)\nfont_family {mono}\nfont_size {mono_pt}.0\n"))?;
    }
    Ok(())
}

// ── терминалы ───────────────────────────────────────────────────────────────
/// 16 цветов терминала и основные: одни на kitty, foot, starship и свои шаблоны
pub fn ansi(p: &P) -> Vec<(String, String)> {
    let acc = p.g("accent");
    let light = p.light();
    // ANSI-цвета чуть в сторону акцента; на светлом фоне — темнее, иначе пастель не читается
    let t = |base: &str| if light { scale_v(&mix(base, acc, 0.18), 0.7) } else { mix(base, acc, 0.18) };
    let rows: Vec<(&str, String)> = vec![
        ("foreground", p.g("text").into()),
        ("background", p.g("bg").into()),
        ("cursor", p.g("bright").into()),
        ("selection_foreground", p.hi()),
        ("selection_background", p.g("selection").into()),
        // в светлой «чёрный» (0) остаётся тёмным: программы пишут им текст
        ("color0", if light { p.g("textDim") } else { p.g("panel") }.into()),
        ("color8", p.g("textFaint").into()),
        ("color1", if light { "#b02a22" } else { "#d0352d" }.into()),
        ("color9", if light { "#d0352d" } else { "#ff5a4c" }.into()),
        ("color2", t("#78b86a")),
        ("color10", t("#a2d692")),
        ("color3", t("#d9a03a")),
        ("color11", t("#f0c46a")),
        ("color4", t("#5a7fa8")),
        ("color12", t("#86a6c8")),
        ("color5", t("#9a5aa0")),
        ("color13", t("#c486c8")),
        ("color6", t("#4f9a96")),
        ("color14", t("#7cc2bc")),
        ("color7", p.g("textDim").into()),
        ("color15", p.g("text").into()),
    ];
    rows.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

fn a_get<'a>(a: &'a [(String, String)], k: &str) -> &'a str {
    a.iter().find(|(x, _)| x == k).map_or("#000000", |(_, v)| v.as_str())
}

/// kitty больше не красим: из kitty.conf уходят наши строки, остальное в нём — человека
fn kitty_detach(cx: &Cx) -> Res {
    let conf = cx.cfg.join("kitty/kitty.conf");
    let Ok(t) = std::fs::read_to_string(&conf) else { return Ok(()) };
    let lines = lines_of(&t);
    let keep: Vec<&str> = lines.iter().copied().filter(|l| l.trim() != "include yami.conf" && !l.starts_with("# YAMI")).collect();
    if keep != lines {
        let text = keep.join("\n").trim_end_matches('\n').to_string() + "\n";
        // ссылка (stow, Home Manager) остаётся ссылкой: пишем сквозь неё
        if std::fs::symlink_metadata(&conf).is_ok_and(|m| m.file_type().is_symlink()) {
            std::fs::write(&conf, text)?;
        } else {
            write(&conf, &text)?;
        }
        signal("USR1", "kitty");
    }
    Ok(())
}

pub fn kitty(cx: &Cx, p: &P) -> Res {
    if !cx.on("kitty") {
        return kitty_detach(cx);
    }
    let a = ansi(p);
    let acc = p.g("accent");
    let mut cols: Vec<(String, String)> = [("foreground", a_get(&a, "foreground")), ("background", a_get(&a, "background")), ("selection_foreground", a_get(&a, "selection_foreground")), ("selection_background", a_get(&a, "selection_background")), ("cursor", a_get(&a, "cursor")), ("cursor_text_color", p.g("bg")), ("url_color", p.g("bright")), ("active_border_color", acc), ("inactive_border_color", p.g("border")), ("bell_border_color", p.g("bright")), ("visual_bell_color", p.g("border")), ("active_tab_foreground", p.g("onAccent")), ("active_tab_background", acc), ("inactive_tab_foreground", p.g("textDim")), ("inactive_tab_background", p.g("panel")), ("tab_bar_background", p.g("bg")), ("mark1_background", acc)].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    for i in 0..16 {
        let k = format!("color{i}");
        let v = a_get(&a, &k).to_string();
        cols.push((k, v));
    }
    let body: String = cols.iter().map(|(k, v)| format!("{k:<24}{v}\n")).collect();
    write(&cx.cfg.join("kitty/yami-colors.conf"), &format!("# YAMI: пишет {WHO} по палитре шелла\n{body}"))?; // i18n-ok
    signal("USR1", "kitty");
    Ok(())
}

const FOOT_MARK: &str = "# YAMI: settings → color → apps → foot off takes these lines out";

/// foot.ini без наших строк: include на yami-colors.ini, пометка и [main] под ней
fn foot_strip(t: &str) -> String {
    let lines = lines_keep(t);
    let mut out = String::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim() == FOOT_MARK {
            i += 1;
            if i < lines.len() && lines[i].trim() == "[main]" {
                i += 1;
            }
            continue;
        }
        let l = lines[i].trim();
        if l.strip_prefix("include").is_some_and(|r| r.trim_start().starts_with('=')) && l.ends_with("yami-colors.ini") {
            i += 1;
            continue;
        }
        out.push_str(lines[i]);
        i += 1;
    }
    let body = out.trim_end_matches('\n');
    if body.is_empty() { String::new() } else { format!("{body}\n") }
}

/// (мажор, минор) из первого «1.22» в тексте
fn first_version(s: &str) -> Option<(u32, u32)> {
    let b = s.as_bytes();
    for i in 0..b.len() {
        if !b[i].is_ascii_digit() || (i > 0 && b[i - 1].is_ascii_digit()) {
            continue;
        }
        let dot = (i..b.len()).find(|&j| !b[j].is_ascii_digit())?;
        if b[dot] != b'.' || !b.get(dot + 1).is_some_and(u8::is_ascii_digit) {
            continue;
        }
        let end = (dot + 1..b.len()).find(|&j| !b[j].is_ascii_digit()).unwrap_or(b.len());
        return Some((s[i..dot].parse().ok()?, s[dot + 1..end].parse().ok()?));
    }
    None
}

/// foot: цвета в foot/yami-colors.ini, include на него — в конце foot.ini под своим [main]: так наши
/// цвета перекрывают [colors] выше, пока foot отмечен. Не отмечен — наши строки уходят
pub fn foot(cx: &Cx, p: &P) -> Res {
    let ini = cx.cfg.join("foot/foot.ini");
    if !cx.on("foot") {
        if ini.exists() && read_lossy(&ini)?.contains("yami-colors.ini") {
            write(&ini, &foot_strip(&read_lossy(&ini)?))?;
        }
        return Ok(());
    }
    if !(which("foot") || cx.cfg.join("foot").is_dir()) {
        return Ok(());
    }
    let a = ansi(p);
    let s = |k: &str| a_get(&a, k).trim_start_matches('#').to_string();
    // foot 1.22: секция [colors] стала [colors-dark] (старое имя ещё читается, но с предупреждением)
    let sec = if output_of(&["foot", "--version"]).and_then(|o| first_version(&o)).is_some_and(|v| v >= (1, 22)) { "colors-dark" } else { "colors" };
    let mut lines = vec![format!("# YAMI: пишет {WHO} по палитре шелла (include в foot.ini)"), format!("[{sec}]"), format!("foreground={}", s("foreground")), format!("background={}", s("background")), format!("selection-foreground={}", s("selection_foreground")), format!("selection-background={}", s("selection_background")), format!("cursor={} {}", p.g("bg").trim_start_matches('#'), s("cursor")), format!("urls={}", p.g("bright").trim_start_matches('#'))]; // i18n-ok
    lines.extend((0..8).map(|i| format!("regular{i}={}", s(&format!("color{i}")))));
    lines.extend((0..8).map(|i| format!("bright{i}={}", s(&format!("color{}", i + 8)))));
    let inc = cx.cfg.join("foot/yami-colors.ini");
    write(&inc, &(lines.join("\n") + "\n"))?;
    // foot раскрывает «~/» в include сам; вне дома (XDG_CONFIG_HOME где-то ещё) — полный путь
    let body = if ini.exists() { foot_strip(&read_lossy(&ini)?) } else { String::new() };
    let block = format!("{FOOT_MARK}\n[main]\ninclude={}\n", cx.tilde(&inc));
    write(&ini, &format!("{body}{}{block}", if body.is_empty() { "" } else { "\n" }))
}

/// `[слово]` в начале строки: слово
fn head_word(line: &str) -> Option<&str> {
    let t = line.trim_start().strip_prefix('[')?;
    let end = t.find(|c: char| !is_word(c))?;
    (end > 0 && t[end..].starts_with(']')).then(|| &t[..end])
}

/// `слово =` в начале строки: слово
fn key_word(line: &str) -> Option<&str> {
    let t = line.trim_start();
    let end = t.find(|c: char| !is_word(c)).unwrap_or(t.len());
    (end > 0 && t[end..].trim_start().starts_with('=')).then(|| &t[..end])
}

/// cava: градиент полос от глубокого акцента к яркому в [color] её конфига; открытая cava
/// перечитывает цвета по SIGUSR2
pub fn cava(cx: &Cx, p: &P) -> Res {
    let conf = cx.cfg.join("cava/config");
    if !cx.on("cava") || !conf.exists() {
        return Ok(());
    }
    let ours = [("gradient", "1".to_string()), ("gradient_count", "4".to_string()), ("gradient_color_1", format!("'{}'", p.g("deep"))), ("gradient_color_2", format!("'{}'", p.g("accent"))), ("gradient_color_3", format!("'{}'", p.g("bright"))), ("gradient_color_4", format!("'{}'", mix(p.g("bright"), p.g("text"), 0.5)))];
    let rows = || ours.iter().map(|(k, v)| format!("{k} = {v}"));
    let text = std::fs::read_to_string(&conf)?;
    let (mut out, mut in_color, mut done): (Vec<String>, bool, bool) = (vec![], false, false);
    for line in lines_of(&text) {
        if let Some(h) = head_word(line) {
            in_color = h == "color";
            out.push(line.to_string());
            if in_color && !done {
                out.extend(rows());
                done = true;
            }
            continue;
        }
        if in_color && key_word(line).is_some_and(|k| ours.iter().any(|(x, _)| *x == k)) {
            continue;
        }
        out.push(line.to_string());
    }
    if !done {
        out.push(String::new());
        out.push("[color]".to_string());
        out.extend(rows());
    }
    let new = out.join("\n") + "\n";
    if new != text {
        write(&conf, &new)?;
        signal("USR2", "cava");
    }
    Ok(())
}

/// `[имя]` в начале строки: имя как есть
fn table_name(line: &str) -> Option<&str> {
    let t = line.trim_start().strip_prefix('[')?;
    let end = t.find(']')?;
    (end > 0).then(|| &t[..end])
}

/// значение `palette = …` верхнего уровня
fn palette_value(line: &str) -> Option<&str> {
    let t = line.trim_start().strip_prefix("palette")?.trim_start().strip_prefix('=')?.trim_start();
    let t = t.strip_prefix(['\'', '"']).unwrap_or(t);
    let end = t.find(|c: char| c == '\'' || c == '"' || c == '#' || c.is_whitespace()).unwrap_or(t.len());
    Some(&t[..end])
}

/// starship: палитра [palettes.yami] и palette = "yami", если палитры не выбрано: выбранную свою не
/// трогаем, [palettes.yami] лежит рядом
pub fn starship(cx: &Cx, p: &P) -> Res {
    let conf = cx.cfg.join("starship.toml");
    if !cx.on("starship") || !conf.exists() {
        return Ok(());
    }
    let a = ansi(p);
    let pal: Vec<(&str, &str)> = vec![("black", a_get(&a, "color0")), ("red", a_get(&a, "color1")), ("green", a_get(&a, "color2")), ("yellow", a_get(&a, "color3")), ("blue", a_get(&a, "color4")), ("purple", a_get(&a, "color5")), ("cyan", a_get(&a, "color6")), ("white", a_get(&a, "color7")), ("bright-black", a_get(&a, "color8")), ("bright-red", a_get(&a, "color9")), ("bright-green", a_get(&a, "color10")), ("bright-yellow", a_get(&a, "color11")), ("bright-blue", a_get(&a, "color12")), ("bright-purple", a_get(&a, "color13")), ("bright-cyan", a_get(&a, "color14")), ("bright-white", a_get(&a, "color15")), ("accent", p.g("accent")), ("accent_deep", p.g("deep")), ("accent_bright", p.g("bright")), ("text", p.g("text")), ("text_dim", p.g("textDim")), ("text_faint", p.g("textFaint")), ("bg", p.g("bg")), ("panel", p.g("panel"))];
    let text = std::fs::read_to_string(&conf)?;
    // своя таблица палитры — заново; строка palette = … верхнего уровня — только если её нет
    let (mut out, mut skip, mut seen_table, mut chosen): (Vec<String>, bool, bool, Option<String>) = (vec![], false, false, None);
    for line in lines_of(&text) {
        if let Some(h) = table_name(line) {
            seen_table = true;
            skip = h.trim() == "palettes.yami";
            if skip {
                continue;
            }
        }
        if skip {
            continue;
        }
        if !seen_table {
            if let Some(v) = palette_value(line) {
                chosen = Some(v.to_string());
            }
        }
        out.push(line.to_string());
    }
    if chosen.is_none() {
        out.insert(0, "palette = \"yami\"".to_string());
    }
    cx.status("starship", chosen.as_deref().filter(|c| *c != "yami").map(|c| serde_json::json!({ "palette": c })))?;
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    out.extend([String::new(), format!("# YAMI: пишет {WHO} по палитре шелла"), "[palettes.yami]".to_string()]); // i18n-ok
    out.extend(pal.iter().map(|(k, v)| format!("{k} = \"{v}\"")));
    let new = out.join("\n") + "\n";
    if new != text {
        write(&conf, &new)?;
    }
    Ok(())
}

// ── btop ────────────────────────────────────────────────────────────────────
/// тема btop в палитре: графики от тусклого к яркому акценту, рамки — как у панелей
pub fn btop(cx: &Cx, p: &P) -> Res {
    if !cx.on("btop") {
        return Ok(());
    }
    let (acc, deep, bright, border) = (p.g("accent"), p.g("deep"), p.g("bright"), p.g("border"));
    let m1 = mix(p.g("textFaint"), acc, 0.5);
    let m2 = mix(deep, p.g("textDim"), 0.5);
    let m3 = mix(acc, p.g("text"), 0.3);
    let th: Vec<(&str, &str)> = vec![("main_bg", p.g("bg")), ("main_fg", p.g("text")), ("title", p.g("text")), ("hi_fg", bright), ("selected_bg", deep), ("selected_fg", "#ffffff"), ("inactive_fg", p.g("textFaint")), ("graph_text", p.g("textDim")), ("meter_bg", p.g("surfaceHi")), ("proc_misc", acc), ("cpu_box", border), ("mem_box", border), ("net_box", border), ("proc_box", border), ("div_line", border), ("temp_start", &m1), ("temp_mid", acc), ("temp_end", bright), ("cpu_start", deep), ("cpu_mid", acc), ("cpu_end", bright), ("free_start", border), ("free_mid", deep), ("free_end", acc), ("cached_start", border), ("cached_mid", &m2), ("cached_end", p.g("textDim")), ("available_start", border), ("available_mid", deep), ("available_end", acc), ("used_start", deep), ("used_mid", acc), ("used_end", bright), ("download_start", border), ("download_mid", deep), ("download_end", acc), ("upload_start", border), ("upload_mid", &m3), ("upload_end", p.g("text")), ("process_start", deep), ("process_mid", acc), ("process_end", bright)];
    let body: String = th.iter().map(|(k, v)| format!("theme[{k}]=\"{v}\"\n")).collect();
    write(&cx.cfg.join("btop/themes/yami.theme"), &format!("# YAMI: пишет {WHO} по палитре шелла\n{body}"))?; // i18n-ok
    let conf = cx.cfg.join("btop/btop.conf");
    if conf.exists() {
        let t = std::fs::read_to_string(&conf)?;
        let is_key = |l: &str| l.strip_prefix("color_theme").is_some_and(|r| r.trim_start().starts_with('='));
        let t = if t.split('\n').any(is_key) { set_lines(&t, is_key, "color_theme = \"yami\"") } else { t + "\ncolor_theme = \"yami\"\n" };
        write(&conf, &t)
    } else {
        write(&conf, "#? Config file for btop (YAMI: only the theme is set, btop fills the rest)\ncolor_theme = \"yami\"\ntheme_background = false\n")
    }
}

// ── Firefox, LibreWolf, Floorp, Zen ─────────────────────────────────────────
/// профили из profiles.ini (бывают и вне папки браузера, IsRelative=0) и просто папки с prefs.js
pub fn firefox_profiles(cx: &Cx) -> Vec<PathBuf> {
    let mut roots = vec![cx.home.join(".mozilla/firefox"), cx.home.join(".librewolf"), cx.cfg.join("librewolf/librewolf"), cx.home.join(".floorp"), cx.home.join(".zen"), cx.cfg.join("mozilla/firefox")];
    // Tor Browser (torbrowser-launcher): профиль внутри скачанного браузера
    if let Ok(rd) = std::fs::read_dir(cx.home.join(".local/share/torbrowser/tbb")) {
        let mut tor: Vec<PathBuf> = rd.flatten().map(|e| e.path().join("tor-browser/Browser/TorBrowser/Data/Browser")).filter(|p| p.exists()).collect();
        tor.sort();
        roots.extend(tor);
    }
    let mut out: Vec<PathBuf> = vec![];
    for r in roots.iter().filter(|r| r.is_dir()) {
        let mut cands: Vec<PathBuf> = std::fs::read_dir(r).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default();
        let ini = r.join("profiles.ini");
        if ini.exists() {
            let (mut rel, mut path) = ("1".to_string(), None::<String>);
            let text = read_lossy(&ini).unwrap_or_default();
            for line in lines_of(&text).into_iter().chain(["["]) {
                let line = line.trim();
                if line.starts_with('[') {
                    if let Some(p) = path.take() {
                        cands.push(if rel == "1" { r.join(p) } else { PathBuf::from(p) });
                    }
                    rel = "1".to_string();
                } else if let Some(v) = line.strip_prefix("IsRelative=") {
                    rel = v.to_string();
                } else if let Some(v) = line.strip_prefix("Path=") {
                    path = Some(v.to_string());
                }
            }
        }
        for d in cands {
            let real = real_path(&d);
            if d.join("prefs.js").exists() && !out.iter().any(|o| real_path(o) == real) {
                out.push(d);
            }
        }
    }
    out
}

/// только меню браузера: квадратные, в палитре. Тему, панели и вкладки не трогаем; пиксельный шрифт
/// в интерфейсе — по желанию (browserFont)
pub fn firefox(cx: &Cx, p: &P) -> Res {
    if !cx.on("firefox") {
        return Ok(());
    }
    let menus = cx.st.get("browserMenus").is_none() || truthy(cx.st.get("browserMenus"));
    let font = truthy(cx.st.get("browserFont"));
    let px = cx.font_px();
    let edge = mix(p.g("border"), p.g("accent"), 0.5);
    let shadow = if p.light() { "#00000038" } else { "#000000b4" };
    let ev = |e: &str| match e {
        "edge" => edge.clone(),
        "shadow" => shadow.to_string(),
        "UI_FONT" => UI_FONT.to_string(),
        "px" => px.to_string(),
        "px + 8" => (px + 8).to_string(),
        _ => String::new(),
    };
    let mut css = String::new();
    if menus {
        css += &pyf(FF_MENUS, p, &ev);
    }
    if font {
        if css.is_empty() {
            css = format!("/* YAMI: written by {WHO} (settings → fonts → browser). Delete the file to drop it. */\n");
        }
        css += &pyf(FF_FONT, p, &ev);
    }
    for prof in firefox_profiles(cx) {
        let f = prof.join("chrome/userChrome.css");
        // свой userChrome.css пользователя не трогаем
        if f.exists() && !read_lossy(&f)?.starts_with("/* YAMI:") {
            continue;
        }
        if css.is_empty() {
            if f.exists() {
                std::fs::remove_file(&f)?;
            }
            continue;
        }
        write(&f, &css)?;
        let uj = prof.join("user.js");
        let t = if uj.exists() { std::fs::read_to_string(&uj)? } else { String::new() };
        if !t.contains("toolkit.legacyUserProfileCustomizations.stylesheets") {
            write(&uj, &format!("{t}// yami: lets the browser read chrome/userChrome.css (menus in the palette)\nuser_pref(\"toolkit.legacyUserProfileCustomizations.stylesheets\", true);\n"))?;
        }
    }
    Ok(())
}

// ── fastfetch ───────────────────────────────────────────────────────────────
enum J {
    S(String),
    I(i64),
    O(Vec<(&'static str, J)>),
    A(Vec<J>),
}

fn s(v: impl Into<String>) -> J {
    J::S(v.into())
}

fn jstr(t: &str) -> String {
    serde_json::to_string(t).unwrap_or_default()
}

/// JSON с порядком ключей как написан (fastfetch читают и глазами)
fn jdump(j: &J, depth: usize, out: &mut String) {
    let pad = |n: usize| "  ".repeat(n);
    match j {
        J::S(t) => out.push_str(&jstr(t)),
        J::I(n) => out.push_str(&n.to_string()),
        J::O(o) if o.is_empty() => out.push_str("{}"),
        J::A(a) if a.is_empty() => out.push_str("[]"),
        J::O(o) => {
            out.push_str("{\n");
            for (i, (k, v)) in o.iter().enumerate() {
                out.push_str(&format!("{}{}: ", pad(depth + 1), jstr(k)));
                jdump(v, depth + 1, out);
                out.push_str(if i + 1 < o.len() { ",\n" } else { "\n" });
            }
            out.push_str(&pad(depth));
            out.push('}');
        }
        J::A(a) => {
            out.push_str("[\n");
            for (i, v) in a.iter().enumerate() {
                out.push_str(&pad(depth + 1));
                jdump(v, depth + 1, out);
                out.push_str(if i + 1 < a.len() { ",\n" } else { "\n" });
            }
            out.push_str(&pad(depth));
            out.push(']');
        }
    }
}

pub fn fastfetch(cx: &Cx, p: &P) -> Res {
    if !cx.on("fastfetch") {
        return Ok(());
    }
    let e = |h: &str| {
        let (r, g, b) = rgb(h);
        format!("38;2;{r};{g};{b}")
    };
    let esc = |h: &str| format!("\u{1b}[{}m", e(h));
    let (a, d, f, t) = (p.g("accent"), p.g("border"), p.g("textFaint"), p.g("text"));
    let logo_path = cx.cfg.join("fastfetch/yami-logo.txt");
    write(&logo_path, &crate::recolor_art::kanji_logo(p))?;
    let swatch: String = [p.g("bg"), p.g("panel"), p.g("border"), p.g("deep"), a, p.g("bright"), p.g("textDim"), t]
        .iter()
        .map(|c| {
            let (r, g, b) = rgb(c);
            format!("\u{1b}[48;2;{r};{g};{b}m   ")
        })
        .collect();
    // мини (по умолчанию): кто, система и окружение — без железа; расширенный (fetchFull): всё
    let full = truthy(cx.st.get("fetchFull"));
    let line = format!("{}{}", esc(d), "─".repeat(if full { 44 } else { 30 }));
    let module = |typ: &'static str, key: &'static str, extra: Vec<(&'static str, J)>| {
        let mut o = vec![("type", s(typ)), ("key", s(key))];
        o.extend(extra);
        J::O(o)
    };
    let fmt = |v: &str| vec![("format", s(v))];
    let custom = |v: String| J::O(vec![("type", s("custom")), ("format", J::S(v))]);
    let mut mods = vec![custom(format!("{}YAMI {}// {}niri", esc(a), esc(f), esc(p.g("textDim")))), custom(line.clone())];
    mods.push(module("title", "host", fmt("{user-name}@{host-name}")));
    mods.push(module("os", "os", fmt(if full { "{pretty-name} {arch}" } else { "{pretty-name}" })));
    if full {
        mods.push(module("kernel", "kernel", fmt("{release}")));
        mods.push(module("uptime", "uptime", vec![]));
        mods.push(module("packages", "pkgs", vec![]));
    }
    mods.push(module("wm", "wm", fmt("{pretty-name}")));
    mods.push(module("shell", "shell", fmt("{pretty-name}")));
    mods.push(module("terminal", "term", fmt("{pretty-name}")));
    if full {
        mods.push(custom(line));
        mods.push(module("cpu", "cpu", fmt("{name} x{cores-logical}")));
        mods.push(module("gpu", "gpu", vec![("format", s("{name}")), ("hideType", s("integrated"))]));
        mods.push(module("memory", "mem", fmt("{percentage-bar} {used} / {total}")));
        mods.push(module("disk", "disk", vec![("folders", s("/")), ("format", s("{size-percentage-bar} {size-used} / {size-total}"))]));
        mods.push(module("display", "display", fmt("{width}x{height} @ {refresh-rate} Hz")));
    } else {
        mods.push(module("uptime", "uptime", vec![]));
    }
    mods.push(s("break"));
    mods.push(custom(format!("{swatch}\u{1b}[0m")));
    let conf = J::O(vec![
        ("$schema", s("https://github.com/fastfetch-cli/fastfetch/raw/dev/doc/json_schema.json")),
        ("logo", J::O(vec![("type", s("file-raw")), ("source", s(logo_path.display().to_string())), ("padding", J::O(vec![("top", J::I(1)), ("left", J::I(2)), ("right", J::I(4))]))])),
        (
            "display",
            J::O(vec![
                ("separator", s("  ")),
                ("color", J::O(vec![("keys", J::S(e(a))), ("title", J::S(e(p.g("bright")))), ("output", J::S(e(t))), ("separator", J::S(e(d)))])),
                ("key", J::O(vec![("width", J::I(if full { 9 } else { 8 }))])),
                ("bar", J::O(vec![("char", J::O(vec![("elapsed", s("■")), ("total", s("·"))])), ("width", J::I(12)), ("border", J::O(vec![("left", s("")), ("right", s(""))]))])),
                ("percent", J::O(vec![("type", J::I(3)), ("color", J::O(vec![("green", J::S(e(p.g("ok")))), ("yellow", J::S(e(p.g("warn")))), ("red", J::S(e(p.g("bright"))))]))])),
            ]),
        ),
        ("modules", J::A(mods)),
    ]);
    // config.jsonc — наш, пока его первая строка наша (или его нет); свой не перезаписываем: наш вид
    // ложится рядом, в yami.jsonc (fastfetch -c ~/.config/fastfetch/yami.jsonc)
    let main = cx.cfg.join("fastfetch/config.jsonc");
    let own = read_lossy(&main).ok().is_some_and(|t| !t.trim_start().split('\n').next().unwrap_or("").starts_with("// YAMI"));
    let target = if own { cx.cfg.join("fastfetch/yami.jsonc") } else { main };
    let mut body = format!("// YAMI: пишет {WHO} по палитре шелла\n"); // i18n-ok
    jdump(&conf, 0, &mut body);
    body.push('\n');
    write(&target, &body)?;
    cx.status("fastfetch", own.then(|| serde_json::json!({ "file": cx.tilde(&target) })))
}

// ── niri ────────────────────────────────────────────────────────────────────
pub fn niri(cx: &Cx, p: &P) -> Res {
    write(&cx.cfg.join("yami/niri/colors.kdl"), &pyf(NIRI_KDL, p, &|_| String::new()))
}

// ── MangoHud ────────────────────────────────────────────────────────────────
const MANGO_MARKS: [&str; 2] = ["# yami: written by yami recolor", "# yami: written by tools/theme.py"];

/// ~/.config/MangoHud/MangoHud.conf в цветах палитры, шрифт Tiny5 по сетке; свой конфиг не трогаем
pub fn mangohud(cx: &Cx, p: &P) -> Res {
    if !cx.on("mangohud") {
        return Ok(());
    }
    let path = cx.cfg.join("MangoHud/MangoHud.conf");
    if path.exists() {
        let t = read_lossy(&path)?;
        if !MANGO_MARKS.iter().any(|m| t.starts_with(m)) {
            return Ok(());
        }
    }
    // без MangoHud конфиг ему не нужен: папку в ~/.config не заводим
    if !path.exists() && !path.parent().is_some_and(Path::is_dir) && !which("mangohud") {
        return Ok(());
    }
    let h = |k: &str| p.g(k).trim_start_matches('#').to_string();
    let font = [cx.home.join(".local/share/fonts/yami/Tiny5.ttf"), cx.home.join(".local/share/fonts/Tiny5.ttf")].into_iter().find(|f| f.exists());
    let mut lines: Vec<String> = vec![format!("{} (shell palette; delete this line to keep your own edits)", MANGO_MARKS[0]), "legacy_layout=false".into(), "horizontal".into(), "hud_compact".into(), "round_corners=0".into(), "background_alpha=0.7".into(), format!("background_color={}", h("bg")), format!("text_color={}", h("text")), format!("gpu_color={}", h("accent")), format!("cpu_color={}", h("bright")), format!("vram_color={}", h("deep")), format!("ram_color={}", h("deep")), format!("frametime_color={}", h("accent")), "fps_color_change".into(), format!("fps_color={},{},{}", h("warn"), h("bright"), h("ok")), "fps_value=30,60".into(), "fps".into(), "frametime".into(), "gpu_stats".into(), "cpu_stats".into(), "vram".into(), "ram".into(), "frame_timing".into(), "position=top-left".into(), "toggle_hud=Shift_R+F12".into(), "font_size=16".into()];
    if let Some(f) = font {
        lines.push(format!("font_file={}", f.display()));
    }
    write(&path, &(lines.join("\n") + "\n"))
}

// ── набор значков ───────────────────────────────────────────────────────────
/// набор, выбранный в шелле (iconTheme): GTK (settings.ini, gsettings) и Qt (qt6ct, kdeglobals) — те,
/// что перекрашиваются. Пусто — набор системы не трогаем
pub fn icons(cx: &Cx, _p: &P) -> Res {
    let name = cx.s("iconTheme").to_string();
    if cx.icon_dir(&name, false).is_none() {
        return Ok(());
    }
    if cx.on("gtk") {
        for v in gtk_dirs() {
            let ini = cx.cfg.join(format!("gtk-{v}/settings.ini"));
            if ini.exists() {
                ini_set(&ini, "gtk-icon-theme-name", &name, "[Settings]")?;
            }
        }
        if which("gsettings") {
            gs_set("icon-theme", &name);
        }
    }
    if cx.on("qt") {
        let qt = cx.cfg.join("qt6ct/qt6ct.conf");
        if qt.exists() {
            ini_set(&qt, "icon_theme", &name, "[Appearance]")?;
        }
        ini_section_set(&cx.cfg.join("kdeglobals"), "[Icons]", "Theme", &name)?;
    }
    Ok(())
}

// io нужен сигнатурам шагов в recolor.rs
#[allow(dead_code)]
fn _io(_: io::ErrorKind) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_of_config_lines() {
        assert_eq!(head_word("  [color] # x"), Some("color"));
        assert_eq!(head_word("[a b]"), None);
        assert_eq!(key_word("  gradient = 1"), Some("gradient"));
        assert_eq!(palette_value("palette = 'x' # y"), Some("x"));
        assert_eq!(palette_value("palette=\"my_pal\""), Some("my_pal"));
        assert_eq!(first_version("foot version: 1.22.3 -pgo"), Some((1, 22)));
        assert_eq!(first_version("nothing"), None);
    }

    #[test]
    fn foot_lines_come_out() {
        let t = "[main]\nfont=x\n\n# YAMI: settings → color → apps → foot off takes these lines out\n[main]\ninclude=~/.config/foot/yami-colors.ini\n";
        assert_eq!(foot_strip(t), "[main]\nfont=x\n");
    }
}
