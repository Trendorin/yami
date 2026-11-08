// Картинки перекраски (recolor.rs): обои шелла (свечение акцента дизерингом Байера и редкая точечная
// сетка), рамки и флажки для Qt, пиксельный курсор XCursor в палитре, значки программ yami, 闇 для
// fastfetch. Всё считается из палитры, ничего не скачивается.

use crate::recolor::*;
use crate::recolor_apps::ini_set;
use crate::recolor_data::{APP_ART, CURSOR_ALIASES, CURSOR_ART, KANJI};
use image::{ImageFormat, Rgba, RgbaImage};
use std::path::Path;

fn px4(h: &str) -> Rgba<u8> {
    let (r, g, b) = rgb(h);
    Rgba([r as u8, g as u8, b as u8, 255])
}

fn save_rgba(img: &RgbaImage, path: &Path) -> Res {
    img.save_with_format(path, ImageFormat::Png).map_err(std::io::Error::other)
}

pub fn wallpaper(cx: &Cx, p: &P) -> Res {
    std::fs::create_dir_all(&cx.state)?;
    let tmp = cx.state.join("wallpaper.tmp.png");
    wallpaper_img(p, 480, 270).save_with_format(&tmp, ImageFormat::Png).map_err(std::io::Error::other)?;
    std::fs::rename(tmp, cx.state.join("wallpaper.png"))
}

// ── Qt: рамки 9-slice и флажки ──────────────────────────────────────────────
/// рамка со срезанными углами, как pxbox шелла: срез 2 px, рамка 1 px, фаска светом сверху и тенью снизу
fn qt_box(d: &Path, name: &str, fill: &str, line: &str, bevel: bool, pressed: bool) -> Res {
    let n = 12i32;
    let mut img = RgbaImage::new(n as u32, n as u32);
    for y in 0..n {
        for x in 0..n {
            let (cx_, cy_) = (x.min(n - 1 - x), y.min(n - 1 - y));
            if cx_ + cy_ < 2 {
                continue;
            }
            let is_line = cx_ == 0 || cy_ == 0 || cx_ + cy_ == 2;
            let mut c = if is_line { line.to_string() } else { fill.to_string() };
            if !is_line && bevel && y == 1 {
                c = if pressed { mix(fill, "#000000", 0.25) } else { mix(fill, "#ffffff", 0.12) };
            } else if !is_line && bevel && y == n - 2 && !pressed {
                c = mix(fill, "#000000", 0.35);
            }
            img.put_pixel(x as u32, y as u32, px4(&c));
        }
    }
    save_rgba(&img, &d.join(format!("{name}.png")))
}

fn qt_mark(d: &Path, name: &str, line: &str, code: &str, dot: Option<&str>, shape: &str) -> Res {
    let mut img = RgbaImage::new(16, 16);
    for y in 0..16i32 {
        for x in 0..16i32 {
            let (a, b) = (x.min(15 - x), y.min(15 - y));
            if a + b < 2 {
                continue;
            }
            let mut c = if a == 0 || b == 0 || a + b == 2 { line } else { code };
            if let Some(dot) = dot {
                if (4..12).contains(&x) && (4..12).contains(&y) {
                    let skip = (shape == "dash" && !(7..9).contains(&y)) || (shape == "dot" && (x - 4).min(11 - x) + (y - 4).min(11 - y) < 2);
                    if !skip {
                        c = dot;
                    }
                }
            }
            img.put_pixel(x as u32, y as u32, px4(c));
        }
    }
    save_rgba(&img, &d.join(format!("{name}.png")))
}

/// картинки для QSS: рамки, флажки как в настройках шелла, стрелка
pub fn qt_pixels(p: &P, d: &Path) -> Res {
    std::fs::create_dir_all(d)?;
    let hi = p.hi();
    qt_box(d, "btn", p.g("surface"), p.g("border"), true, false)?;
    qt_box(d, "btn-hover", p.g("surfaceHi"), p.g("accent"), true, false)?;
    qt_box(d, "btn-press", p.g("deep"), p.g("accent"), true, true)?;
    qt_box(d, "btn-accent", p.g("accent"), p.g("bright"), true, false)?;
    qt_box(d, "btn-accent-hover", p.g("bright"), &hi, true, false)?;
    qt_box(d, "btn-off", p.g("panel"), p.g("border"), false, false)?;
    qt_box(d, "field", p.g("codeBg"), p.g("border"), false, false)?;
    qt_box(d, "field-focus", p.g("codeBg"), p.g("accent"), false, false)?;
    let code = p.g("codeBg");
    qt_mark(d, "check", p.g("border"), code, None, "")?;
    qt_mark(d, "check-hover", p.g("accent"), code, None, "")?;
    qt_mark(d, "check-on", p.g("border"), code, Some(p.g("bright")), "box")?;
    qt_mark(d, "check-on-hover", p.g("accent"), code, Some(p.g("bright")), "box")?;
    qt_mark(d, "check-part", p.g("border"), code, Some(p.g("bright")), "dash")?;
    qt_mark(d, "radio", p.g("border"), code, None, "")?;
    qt_mark(d, "radio-hover", p.g("accent"), code, None, "")?;
    qt_mark(d, "radio-on", p.g("border"), code, Some(p.g("bright")), "dot")?;
    let mut arrow = RgbaImage::new(7, 7);
    for (y, row) in [".......", ".......", "#######", ".#####.", "..###..", "...#...", "......."].iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            if ch == '#' {
                arrow.put_pixel(x as u32, y as u32, px4(p.g("text")));
            }
        }
    }
    save_rgba(&arrow, &d.join("arrow.png"))
}

// ── курсор ──────────────────────────────────────────────────────────────────
struct Cur {
    nominal: u32,
    size: u32,
    xhot: u32,
    yhot: u32,
    delay: u32,
    px: Vec<u32>,
}

/// кадры → байты файла XCursor
fn xcursor(images: &[Cur]) -> Vec<u8> {
    let n = images.len() as u32;
    let mut out = Vec::new();
    let le = |o: &mut Vec<u8>, v: u32| o.extend_from_slice(&v.to_le_bytes());
    out.extend_from_slice(b"Xcur");
    le(&mut out, 16);
    le(&mut out, 0x10000);
    le(&mut out, n);
    let mut pos = 16 + n * 12;
    let mut chunks: Vec<u8> = Vec::new();
    for c in images {
        le(&mut out, 0xFFFD0002);
        le(&mut out, c.nominal);
        le(&mut out, pos);
        let from = chunks.len();
        for v in [36, 0xFFFD0002, c.nominal, 1, c.size, c.size, c.xhot, c.yhot, c.delay] {
            le(&mut chunks, v);
        }
        for v in &c.px {
            le(&mut chunks, *v);
        }
        pos += (chunks.len() - from) as u32;
    }
    out.extend(chunks);
    out
}

/// сетка 12×12 → ARGB в масштабе k, с тенью на клетку
fn render(rows: &[String], k: usize, pal: &[(char, u32)]) -> (u32, Vec<u32>) {
    let size = 12 * k + k;
    let mut px = vec![0u32; size * size];
    for (y, row) in rows.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            if ch != '.' && x + 1 < 13 && y + 1 < 13 {
                for j in 0..k {
                    for i in 0..k {
                        let q = ((y + 1) * k + j) * size + (x + 1) * k + i;
                        if q < px.len() && px[q] == 0 {
                            px[q] = 0x50000000;
                        }
                    }
                }
            }
        }
    }
    for (y, row) in rows.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            if let Some((_, c)) = pal.iter().find(|(n, _)| *n == ch) {
                for j in 0..k {
                    for i in 0..k {
                        px[(y * k + j) * size + x * k + i] = *c;
                    }
                }
            }
        }
    }
    (size as u32, px)
}

fn argb(h: &str) -> u32 {
    let (r, g, b) = rgb(h);
    0xFF000000 | ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

const SIZES: [(u32, usize); 4] = [(24, 2), (32, 3), (48, 4), (64, 5)];

/// тема пиксельного курсора в палитре p в папке d (d/cursors, d/index.theme)
pub fn cursor_theme(p: &P, d: &Path, name: &str) -> Res {
    if d.join("cursors/default").exists() {
        return Ok(());
    }
    // тело — цвет текста, обводка — к фону: в светлой теме курсор тёмный со светлым контуром
    let edge = if p.light() { p.g("codeBg").to_string() } else { mix(p.g("bg"), "#000000", 0.5) };
    let pal = [('o', argb(&edge)), ('a', argb(p.g("accent"))), ('b', argb(p.g("bright"))), ('w', argb(p.g("text")))];
    let dir = d.join("cursors");
    std::fs::create_dir_all(&dir)?;
    for (base, hot, rows) in CURSOR_ART {
        let rows: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
        let imgs: Vec<Cur> = SIZES
            .iter()
            .map(|&(nominal, k)| {
                let (size, px) = render(&rows, k, &pal);
                Cur { nominal, size, xhot: hot.0 * k as u32, yhot: hot.1 * k as u32, delay: 0, px }
            })
            .collect();
        std::fs::write(dir.join(base), xcursor(&imgs))?;
    }
    // ожидание: восемь точек по кругу, бегущий огонёк — восемь кадров
    let ring = [(5, 1), (8, 2), (9, 5), (8, 8), (5, 9), (2, 8), (1, 5), (2, 2)];
    let mut imgs = vec![];
    for &(nominal, k) in &SIZES {
        for f in 0..8i32 {
            let mut grid = vec![vec!['.'; 12]; 12];
            for (i, (x, y)) in ring.iter().enumerate() {
                let i = i as i32;
                let ch = if i == f { 'b' } else if [1, 2].contains(&(f - i).rem_euclid(8)) { 'a' } else { 'o' };
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    grid[y + dy][x + dx] = ch;
                }
            }
            let rows: Vec<String> = grid.iter().map(|r| r.iter().collect()).collect();
            let (size, px) = render(&rows, k, &pal);
            imgs.push(Cur { nominal, size, xhot: 6 * k as u32, yhot: 6 * k as u32, delay: 90, px });
        }
    }
    std::fs::write(dir.join("wait"), xcursor(&imgs))?;
    for (base, names) in CURSOR_ALIASES {
        for n in names {
            let l = dir.join(n);
            if !l.exists() {
                let _ = std::os::unix::fs::symlink(base, l);
            }
        }
    }
    write(&d.join("index.theme"), &format!("[Icon Theme]\nName={name}\nComment=yami pixel cursor ({})\nInherits=Adwaita\n", p.g("accent")))
}

/// тема yami-<акцент>, niri и программы переключаются на неё (~/.config/niri/yami-cursor.kdl). В
/// настройках: пиксельный, любой установленный набор или «не трогать», размер
pub fn cursor(cx: &Cx, p: &P) -> Res {
    let kind = match cx.s("cursor") {
        "" => "yami",
        k => k,
    };
    let size = cx.n("cursorSize");
    let icons = cx.data.join("icons");
    let kdl = cx.cfg.join("niri/yami-cursor.kdl");
    if kind == "system" {
        if kdl.exists() {
            std::fs::remove_file(&kdl)?;
        }
        return Ok(());
    }
    let name = if kind != "yami" && cx.icon_dir(kind, true).is_some() {
        // свой набор курсоров: как есть, только размер
        kind.to_string()
    } else {
        // имя меняется с цветом и темой: niri перечитывает курсор, только когда сменилось имя
        let n = format!("yami-{}{}", p.g("accent").trim_start_matches('#').to_lowercase(), if p.light() { "-l" } else { "" });
        cursor_theme(p, &icons.join(&n), &n)?;
        n
    };
    // старые цвета — убрать
    if let Ok(rd) = std::fs::read_dir(&icons) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n != name && is_old_cursor(&n) && e.path().is_dir() {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    write(&kdl, &format!("// yami: written by {WHO} (shell settings → look → cursor)\ncursor {{\n    xcursor-theme \"{name}\"\n    xcursor-size {size}\n}}\nenvironment {{\n    XCURSOR_THEME \"{name}\"\n    XCURSOR_SIZE \"{size}\"\n}}\n"))?;
    // X11-программам (xwayland-satellite) — тема по умолчанию
    write(&icons.join("default/index.theme"), &format!("[Icon Theme]\nName=default\nInherits={name}\n"))?;
    for v in ["3.0", "4.0"] {
        let ini = cx.cfg.join(format!("gtk-{v}/settings.ini"));
        if ini.exists() {
            ini_set(&ini, "gtk-cursor-theme-name", &name, "[Settings]")?;
            ini_set(&ini, "gtk-cursor-theme-size", &size.to_string(), "[Settings]")?;
        }
    }
    if which("gsettings") {
        gs_set("cursor-theme", &name);
        gs_set("cursor-size", &size.to_string());
    }
    Ok(())
}

/// yami-<шесть цифр>[-l]: так мы называем свои темы курсора
fn is_old_cursor(n: &str) -> bool {
    let Some(rest) = n.strip_prefix("yami-") else { return false };
    let hex = rest.strip_suffix("-l").unwrap_or(rest);
    hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

// ── значки программ и 闇 ─────────────────────────────────────────────────────
/// значки yami-files, yami-view и yami-play в цветах палитры (SVG клетками, crispEdges)
pub fn app_icons(cx: &Cx, p: &P) -> Res {
    let shade = mix(p.g("bg"), "#000000", 0.55);
    let pal = [('o', shade.as_str()), ('d', p.g("deep")), ('a', p.g("accent")), ('b', p.g("bright")), ('w', p.g("onAccent"))];
    let d = cx.data.join("icons/hicolor/scalable/apps");
    for (name, rows) in APP_ART {
        let mut rects = vec![];
        for (y, row) in rows.iter().enumerate() {
            let cs: Vec<char> = row.chars().collect();
            let mut x = 0;
            while x < cs.len() {
                let ch = cs[x];
                let mut n = 1;
                while x + n < cs.len() && cs[x + n] == ch {
                    n += 1;
                }
                if let Some((_, c)) = pal.iter().find(|(k, _)| *k == ch) {
                    rects.push(format!("<rect x=\"{x}\" y=\"{y}\" width=\"{n}\" height=\"1\" fill=\"{c}\"/>"));
                }
                x += n;
            }
        }
        write(&d.join(format!("{name}.svg")), &format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 16 16\" width=\"16\" height=\"16\" shape-rendering=\"crispEdges\">\n<!-- yami: written by {WHO} -->\n{}\n</svg>\n", rects.join("\n")))?;
    }
    Ok(())
}

/// иероглиф полублоками (▀: верхний пиксель — цвет текста, нижний — фон): квадратные пиксели, полосы
/// от светлого к акценту сверху вниз и тень на пиксель, как у панелей шелла
pub fn kanji_logo(p: &P) -> String {
    let (gw, gh) = (KANJI[0].len() as i32, KANJI.len() as i32);
    let (w, h) = (gw + 1, gh + 1);
    let bands = [p.g("bright").to_string(), mix(p.g("bright"), p.g("accent"), 0.5), p.g("accent").to_string(), mix(p.g("accent"), p.g("deep"), 0.5)];
    let shadow = mix(p.g("deep"), p.g("bg"), 0.45);
    let on = |x: i32, y: i32| y >= 0 && y < gh && x >= 0 && x < gw && KANJI[y as usize].as_bytes()[x as usize] == b'#';
    let px = |x: i32, y: i32| -> Option<&str> {
        if on(x, y) {
            return Some(&bands[((y * 4 / gh) as usize).min(3)]);
        }
        // тень только снаружи (справа и снизу), чтобы не забивать просветы знака
        (on(x - 1, y - 1) && (x >= gw || y >= gh - 1)).then_some(shadow.as_str())
    };
    let mut rows = vec![];
    for y in (0..h).step_by(2) {
        let mut line = String::new();
        for x in 0..w {
            match (px(x, y), px(x, y + 1)) {
                (None, None) => line += "\u{1b}[0m ",
                (Some(t), None) => {
                    let (r, g, b) = rgb(t);
                    line += &format!("\u{1b}[0m\u{1b}[38;2;{r};{g};{b}m▀");
                }
                (None, Some(b_)) => {
                    let (r, g, b) = rgb(b_);
                    line += &format!("\u{1b}[0m\u{1b}[38;2;{r};{g};{b}m▄");
                }
                (Some(t), Some(b_)) => {
                    let ((r, g, b), (r2, g2, b2)) = (rgb(t), rgb(b_));
                    line += &format!("\u{1b}[38;2;{r};{g};{b}m\u{1b}[48;2;{r2};{g2};{b2}m▀");
                }
            }
        }
        rows.push(format!("{line}\u{1b}[0m"));
    }
    rows.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xcursor_header() {
        let b = xcursor(&[Cur { nominal: 24, size: 1, xhot: 0, yhot: 0, delay: 0, px: vec![0xff000000] }]);
        assert_eq!(&b[..4], b"Xcur");
        assert_eq!(b.len(), 16 + 12 + 36 + 4);
    }

    #[test]
    fn own_cursor_names() {
        assert!(is_old_cursor("yami-c4161c"));
        assert!(is_old_cursor("yami-c4161c-l"));
        assert!(!is_old_cursor("yami-cursor"));
    }
}
